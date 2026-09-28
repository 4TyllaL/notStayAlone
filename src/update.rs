//! Atualização automática pelas releases do GitHub.
//!
//! 1. Uma vez por dia (se ligado), um processo filho `--procurar-versao` pergunta
//!    à API do GitHub qual é a última release deste repositório.
//! 2. Se houver versão mais nova, o painel mostra "Atualizar". Ao clicar, outro
//!    filho `--baixar-versao` baixa o .exe **só** do endereço de releases deste
//!    repositório, confere o SHA-256 informado pelo GitHub e a assinatura Ed25519
//!    (`dontStayAlone.exe.sig`) feita com a chave de quem publica, que não fica no GitHub.
//! 3. Com tudo conferido, o app mostra o que foi verificado e o que mudou, e só
//!    instala se você confirmar (senão apaga o arquivo baixado).
//! 4. O app renomeia o .exe em uso para `.old`, põe o novo no lugar e abre o novo
//!    com `--atualizado <pid>`; o novo espera o antigo fechar e apaga o `.old`.
//!
//! O processo do mascote nunca abre conexões: só os filhos usam a rede.

use std::{fs, path::PathBuf};

use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
};

use crate::{
    ai::json::{self, Json},
    child,
    lang::{fill, tr},
    net, sha256,
    win::clean_line,
};

pub const ARG_CHECK: &str = "--procurar-versao";
pub const ARG_DOWNLOAD: &str = "--baixar-versao";
pub const ARG_AFTER: &str = "--atualizado";

const LATEST_API: &str = "https://api.github.com/repos/4TyllaL/notStayAlone/releases/latest";
/// Único lugar de onde um .exe novo pode vir.
const DOWNLOADS: &str = "https://github.com/4TyllaL/notStayAlone/releases/download/";
const ASSET: &str = "dontStayAlone.exe";
const MAX_API: usize = 512 * 1024;
const MAX_EXE: usize = 16 * 1024 * 1024;
const MAX_SIG: usize = 1024;
/// "O que mudou": no máximo tantos itens, cada um até tantos caracteres.
const MAX_NOTES: usize = 6;
const MAX_NOTE: usize = 140;

/// Chave pública das releases (Ed25519). A privada fica só com quem publica
/// (`cargo run --example assinar`): nem com acesso ao repositório dá para gerar um
/// `.sig` que este app aceite.
const RELEASE_KEY: [u8; 32] = [
    0x76, 0x12, 0x4a, 0xb7, 0x62, 0x81, 0x0f, 0xab, 0x4b, 0x18, 0xdf, 0x56, 0xe3, 0x98, 0xb0, 0x97,
    0x20, 0x6c, 0x9d, 0x20, 0x99, 0x00, 0xe5, 0x52, 0xf8, 0xe7, 0x21, 0x18, 0x0b, 0x81, 0x25, 0x2a,
];

/// Uma versão publicada: número, de onde baixar, o SHA-256 esperado e o que mudou.
#[derive(Clone, PartialEq, Debug)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub sha256: String,
    /// Itens de "o que mudou", um por linha, já sem Markdown (pode ser vazio).
    pub notes: String,
}

impl Release {
    /// Linha passada ao filho que baixa: "versão url sha256".
    fn to_line(&self) -> String {
        format!("{} {} {}", self.version, self.url, self.sha256)
    }

    fn from_line(line: &str) -> Option<Release> {
        let mut parts = line.split_whitespace();
        let (version, url, sha256) = (parts.next()?, parts.next()?, parts.next()?);
        let release = Release { version: version.into(), url: url.into(), sha256: sha256.into(), notes: String::new() };
        release.trusted().then_some(release)
    }

    /// Resposta do filho que procura: a linha e, abaixo, o que mudou.
    fn to_reply(&self) -> String {
        format!("{}\n{}", self.to_line(), self.notes)
    }

    pub fn from_reply(reply: &str) -> Option<Release> {
        let mut lines = reply.lines();
        let mut release = Release::from_line(lines.next()?)?;
        let notes: Vec<String> =
            lines.map(|l| clean_line(l, MAX_NOTE + 1)).filter(|l| !l.is_empty()).take(MAX_NOTES).collect();
        release.notes = notes.join("\n");
        Some(release)
    }

    /// Endereço do repositório oficial e hash no formato certo.
    fn trusted(&self) -> bool {
        parse_version(&self.version).is_some()
            && self.url.starts_with(DOWNLOADS)
            && self.url.ends_with(&format!("/{ASSET}"))
            && !self.url[DOWNLOADS.len()..].contains("..")
            && self.sha256.len() == 64
            && self.sha256.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let mut parts = v.trim_start_matches('v').split('.').map(|p| p.parse::<u32>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

/// `candidate` é mais nova que `installed`? (nunca volta para uma versão antiga)
pub fn is_newer(candidate: &str, installed: &str) -> bool {
    matches!((parse_version(candidate), parse_version(installed)), (Some(a), Some(b)) if a > b)
}

/// A frase assinada: amarra o arquivo (SHA-256) à versão, então uma release antiga
/// assinada não passa por uma versão nova. Igual à de examples/assinar.rs.
fn signed_message(version: &str, sha256: &str) -> String {
    format!("!StayAlone {version} sha256:{sha256}")
}

/// `sig` (hexadecimal) é a assinatura de `key` para esta versão e este SHA-256?
fn signature_ok(key: &[u8; 32], version: &str, sha256: &str, sig: &[u8]) -> bool {
    let Some(sig) = std::str::from_utf8(sig).ok().map(str::trim).filter(|s| s.len() == 128 && s.bytes().all(|b| b.is_ascii_hexdigit())) else { return false };
    let Some(bytes) = (0..128).step_by(2).map(|i| u8::from_str_radix(&sig[i..i + 2], 16).ok()).collect::<Option<Vec<u8>>>()
    else {
        return false;
    };
    match (ed25519_compact::PublicKey::from_slice(key), ed25519_compact::Signature::from_slice(&bytes)) {
        (Ok(key), Ok(sig)) => key.verify(signed_message(version, sha256).as_bytes(), &sig).is_ok(),
        _ => false,
    }
}

/// Tira o Markdown de um item: negrito/itálico, crases e links (fica o texto).
fn plain(item: &str) -> String {
    let mut out = String::new();
    let mut rest = item;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        match after.find("](").and_then(|mid| after[mid..].find(')').map(|end| (mid, mid + end))) {
            Some((mid, end)) => {
                out.push_str(&rest[..start]);
                out.push_str(&after[..mid]);
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out.replace(['*', '`'], "")
}

/// "O que mudou": os itens (`- ...`) da primeira seção das notas que tem itens, só a
/// primeira frase de cada um.
fn summary(body: &str) -> String {
    let mut items = Vec::new();
    for line in body.lines().map(str::trim) {
        if line.starts_with('#') && !items.is_empty() {
            break;
        }
        let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) else { continue };
        let text = plain(item);
        let text = text.split_once(". ").map_or(text.as_str(), |(first, _)| first).trim_end_matches('.');
        let mut text = clean_line(text, MAX_NOTE);
        if text.chars().count() == MAX_NOTE {
            text.push('…');
        }
        if !text.is_empty() && items.len() < MAX_NOTES {
            items.push(text);
        }
    }
    items.join("\n")
}

/// O texto da pergunta antes de instalar: o que foi conferido e o que mudou.
pub fn confirmation(release: &Release) -> String {
    let short = format!("{}…{}", &release.sha256[..8], &release.sha256[56..]);
    let mut text = format!(
        "!StayAlone {} → {}\n\n✓ {}\n✓ {}\n✓ {}\n✓ {}",
        current(),
        release.version,
        tr("Baixado das releases oficiais (github.com/4TyllaL/notStayAlone)"),
        fill(tr("SHA-256 confere com o da release ({})"), &[&short]),
        tr("Assinatura do !StayAlone válida (Ed25519)"),
        tr("Versão mais nova que a instalada"),
    );
    if !release.notes.is_empty() {
        text.push_str(&format!("\n\n{}\n", tr("O que mudou:")));
        for note in release.notes.lines() {
            text.push_str(&format!("• {note}\n"));
        }
    } else {
        text.push('\n');
    }
    text.push_str(&format!("\n{}", tr("Instalar agora? O mascote fecha e volta em seguida, já na versão nova.")));
    text
}

/// Lê a resposta da API de releases do GitHub.
fn parse_release(api: &Json) -> Option<Release> {
    if api.get("draft") == Some(&Json::Bool(true)) || api.get("prerelease") == Some(&Json::Bool(true)) {
        return None;
    }
    let version = api.get("tag_name")?.as_str()?.trim_start_matches('v').to_string();
    let asset = api.get("assets")?.as_array().iter().find(|a| a.get("name").and_then(Json::as_str) == Some(ASSET))?;
    let url = asset.get("browser_download_url")?.as_str()?.to_string();
    let sha256 = asset.get("digest")?.as_str()?.strip_prefix("sha256:")?.to_ascii_lowercase();
    let notes = api.get("body").and_then(Json::as_str).map(summary).unwrap_or_default();
    let release = Release { version, url, sha256, notes };
    release.trusted().then_some(release)
}

// --- processos filhos --------------------------------------------------------------

/// Modo `--procurar-versao`: escreve a última release (ou nada) e devolve o código de saída.
pub fn serve_check() -> i32 {
    let found = net::get(LATEST_API, "application/vnd.github+json", MAX_API).and_then(|(status, body)| match status {
        200 => {
            let text = String::from_utf8(body).map_err(|_| tr("resposta inválida").to_string())?;
            Ok(parse_release(&json::parse(&text)?))
        }
        404 => Ok(None), // ainda sem releases
        other => Err(fill(tr("GitHub respondeu HTTP {}"), &[&other])),
    });
    report(found.map(|release| release.map(|r| r.to_reply()).unwrap_or_default()))
}

/// Modo `--baixar-versao versão url sha256`: baixa, confere e escreve o caminho do arquivo.
pub fn serve_download(line: &str) -> i32 {
    report(download(line).map(|path| path.display().to_string()))
}

fn report(result: Result<String, String>) -> i32 {
    use std::io::Write;
    match result {
        Ok(text) => {
            let _ = std::io::stdout().write_all(text.as_bytes());
            0
        }
        Err(e) => {
            let _ = std::io::stderr().write_all(e.as_bytes());
            1
        }
    }
}

fn download(line: &str) -> Result<PathBuf, String> {
    let release = Release::from_line(line).ok_or(tr("endereço de atualização recusado"))?;
    let (status, bytes) = net::get(&release.url, "application/octet-stream", MAX_EXE)?;
    if status != 200 {
        return Err(fill(tr("o download falhou (HTTP {})"), &[&status]));
    }
    if sha256::hex(&bytes) != release.sha256 {
        return Err(tr("o arquivo baixado não confere com o da release (SHA-256)").into());
    }
    if !bytes.starts_with(b"MZ") {
        return Err(tr("o arquivo baixado não é um programa do Windows").into());
    }
    let (status, sig) = net::get(&format!("{}.sig", release.url), "application/octet-stream", MAX_SIG)?;
    if status != 200 {
        return Err(tr("a versão nova não veio com a assinatura do !StayAlone").into());
    }
    if !signature_ok(&RELEASE_KEY, &release.version, &release.sha256, &sig) {
        return Err(tr("a assinatura da versão nova não confere; a atualização foi recusada").into());
    }
    let dir = folder().ok_or(tr("sem pasta de dados"))?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (part, done) = (dir.join(format!("{ASSET}.part")), dir.join(ASSET));
    fs::write(&part, &bytes).map_err(|e| e.to_string())?;
    fs::rename(&part, &done).map_err(|e| e.to_string())?;
    Ok(done)
}

/// `%APPDATA%\StayAlone\update`
fn folder() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("StayAlone").join("update"))
}

// --- no app --------------------------------------------------------------------------

/// Procura versão nova num filho; a resposta chega à janela `to` com `msg`.
pub fn check(to: windows_sys::Win32::Foundation::HWND, msg: u32) {
    child::spawn(to, msg, || child::run(child::this_app(&[ARG_CHECK]), "", 8192), |reply| reply);
}

/// Baixa a versão `release` num filho; a resposta (caminho do arquivo) chega com `msg`.
pub fn fetch(to: windows_sys::Win32::Foundation::HWND, msg: u32, release: &Release) {
    let line = release.to_line();
    child::spawn(to, msg, move || child::run(child::this_app(&[ARG_DOWNLOAD, &line]), "", 4096), |reply| reply);
}

/// Você disse "agora não": apaga o que foi baixado (só no lugar esperado).
pub fn discard(downloaded: &str) {
    if folder().map(|d| d.join(ASSET)).as_deref() == Some(std::path::Path::new(downloaded)) {
        let _ = fs::remove_file(downloaded);
    }
}

/// Troca o .exe em uso pelo baixado e abre o novo. Depois disso o app deve fechar.
pub fn install(downloaded: &str) -> Result<(), String> {
    let downloaded = PathBuf::from(downloaded);
    let expected = folder().map(|d| d.join(ASSET));
    if expected.as_ref() != Some(&downloaded) {
        return Err(tr("arquivo de atualização fora do lugar esperado").into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let old = exe.with_extension("old.exe");
    let _ = fs::remove_file(&old);
    // O Windows deixa renomear um .exe em uso, mas não sobrescrevê-lo.
    fs::rename(&exe, &old).map_err(|e| fill(tr("não consegui trocar o programa nesta pasta ({})"), &[&e]))?;
    if let Err(e) = fs::copy(&downloaded, &exe) {
        let _ = fs::rename(&old, &exe); // desfaz
        return Err(fill(tr("não consegui instalar a versão nova ({})"), &[&e]));
    }
    let _ = fs::remove_file(&downloaded);
    let pid = std::process::id().to_string();
    std::process::Command::new(&exe).args([ARG_AFTER, &pid]).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// No .exe novo (`--atualizado <pid>`): espera o antigo fechar e apaga o `.old`.
pub fn finish(pid: &str) {
    if let Ok(pid) = pid.parse::<u32>() {
        unsafe {
            let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if !process.is_null() {
                // Se o antigo não fechar em 15 s, segue assim mesmo (o mutex decide quem fica).
                WaitForSingleObject(process, 15_000);
                CloseHandle(process);
            }
        }
    }
    for _ in 0..20 {
        if cleanup() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

/// Apaga o `.old` deixado por uma atualização (em toda abertura). `true` = não sobrou nada.
pub fn cleanup() -> bool {
    let Ok(exe) = std::env::current_exe() else { return true };
    let old = exe.with_extension("old.exe");
    !old.exists() || fs::remove_file(&old).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "55cfd144c76b8e9d8d13b3a5d99a34fd39a90b0cfcd34ed5697c5b86116c6ea9";

    fn api(tag: &str, url: &str) -> Json {
        let text = format!(
            r#"{{"tag_name":"{tag}","draft":false,"prerelease":false,"body":"Intro.\n\n## What changed\n\n- **Water goal** — set glasses. Log them.\n- See [the README](https://x/y) and `--ajuda`.\n\n## Download\n\n- ignored","assets":[{{"name":"dontStayAlone.exe","browser_download_url":"{url}","digest":"sha256:{SHA}"}}]}}"#
        );
        json::parse(&text).unwrap()
    }

    #[test]
    fn versions_only_go_forward() {
        assert!(is_newer("1.0.2", "1.0.1") && is_newer("v1.1.0", "1.0.9") && is_newer("2.0.0", "1.99.99"));
        assert!(!is_newer("1.0.1", "1.0.1") && !is_newer("1.0.0", "1.0.1") && !is_newer("lixo", "1.0.0"));
    }

    #[test]
    fn reads_the_github_release() {
        let url = "https://github.com/4TyllaL/notStayAlone/releases/download/v1.2.0/dontStayAlone.exe";
        let release = parse_release(&api("v1.2.0", url)).unwrap();
        let notes = "Water goal \u{2014} set glasses\nSee the README and --ajuda".to_string();
        assert_eq!(release, Release { version: "1.2.0".into(), url: url.into(), sha256: SHA.into(), notes });
        assert_eq!(Release::from_reply(&release.to_reply()), Some(release.clone()));
        // O filho que baixa recebe só a linha.
        assert_eq!(Release::from_line(&release.to_line()).map(|r| r.url), Some(release.url.clone()));
        let text = confirmation(&release);
        assert!(text.contains("→ 1.2.0") && text.contains("55cfd144…116c6ea9") && text.contains("• Water goal"));
    }

    #[test]
    fn notes_are_short_and_plain() {
        let body = format!("- {}\n- \u{202e}oi\u{0}\n- [x](y\n- 3\n- 4\n- 5\n- 6\n- 7", "a".repeat(500));
        let notes = summary(&body);
        let lines: Vec<&str> = notes.lines().collect();
        assert_eq!(lines.len(), MAX_NOTES);
        assert_eq!(lines[0].chars().count(), MAX_NOTE + 1);
        assert_eq!(lines[1], "oi");
        assert_eq!(lines[2], "[x](y");
        // Uma resposta com linhas demais é aparada no app também.
        let line = format!("1.2.0 https://github.com/4TyllaL/notStayAlone/releases/download/v1.2.0/dontStayAlone.exe {SHA}");
        let reply = format!("{line}\n{}", "b\n".repeat(50));
        assert_eq!(Release::from_reply(&reply).unwrap().notes.lines().count(), MAX_NOTES);
    }

    #[test]
    fn only_signed_releases_are_accepted() {
        use ed25519_compact::{KeyPair, Seed};
        let keys = KeyPair::from_seed(Seed::generate());
        let key: [u8; 32] = keys.pk.as_ref().try_into().unwrap();
        let sign = |version: &str, sha: &str| {
            let sig = keys.sk.sign(signed_message(version, sha).as_bytes(), None);
            sig.as_ref().iter().map(|b| format!("{b:02x}")).collect::<String>() + "
"
        };
        let good = sign("1.3.0", SHA);
        assert!(signature_ok(&key, "1.3.0", SHA, good.as_bytes()));
        // Outra versão, outro arquivo, outra chave ou lixo: recusado.
        assert!(!signature_ok(&key, "9.0.0", SHA, good.as_bytes()));
        assert!(!signature_ok(&key, "1.3.0", &SHA.replace('5', "6"), good.as_bytes()));
        assert!(!signature_ok(&RELEASE_KEY, "1.3.0", SHA, good.as_bytes()));
        assert!(!signature_ok(&key, "1.3.0", SHA, b"naohex"));
        assert!(!signature_ok(&key, "1.3.0", SHA, format!("a{}b", "é".repeat(63)).as_bytes()));
        assert!(!signature_ok(&key, "1.3.0", SHA, b""));
    }

    /// Antes de publicar: `cargo test --release -- --ignored release_is_signed` confere o
    /// .exe de target\release e o .sig dele com a chave embutida, como o app fará.
    #[test]
    #[ignore]
    fn release_is_signed() {
        let exe = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(r"target\release\dontStayAlone.exe");
        let bytes = fs::read(&exe).unwrap();
        let sig = fs::read(exe.with_extension("exe.sig")).expect("falta o .sig: cargo run --example assinar");
        assert!(signature_ok(&RELEASE_KEY, current(), &sha256::hex(&bytes), &sig), "assinatura não confere");
    }

    #[test]
    fn downloads_only_come_from_this_repository() {
        for url in [
            "https://github.com/outra-pessoa/notStayAlone/releases/download/v9.0.0/dontStayAlone.exe",
            "https://evil.com/4TyllaL/notStayAlone/releases/download/v9.0.0/dontStayAlone.exe",
            "https://github.com/4TyllaL/notStayAlone/releases/download/../../x/dontStayAlone.exe",
            "https://github.com/4TyllaL/notStayAlone/releases/download/v9.0.0/virus.exe",
        ] {
            assert!(parse_release(&api("v9.0.0", url)).is_none(), "aceitou {url}");
        }
        assert!(Release::from_line("1.2.0 https://github.com/4TyllaL/notStayAlone/releases/download/v1.2.0/dontStayAlone.exe naohex").is_none());
    }
}
