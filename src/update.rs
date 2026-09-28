//! Atualização automática pelas releases do GitHub.
//!
//! 1. Uma vez por dia (se ligado), um processo filho `--procurar-versao` pergunta
//!    à API do GitHub qual é a última release deste repositório.
//! 2. Se houver versão mais nova, o painel mostra "Atualizar". Ao clicar, outro
//!    filho `--baixar-versao` baixa o .exe **só** do endereço de releases deste
//!    repositório e confere o SHA-256 informado pelo GitHub.
//! 3. O app renomeia o .exe em uso para `.old`, põe o novo no lugar e abre o novo
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
    child, net, sha256,
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

/// Uma versão publicada: número, de onde baixar e o SHA-256 esperado.
#[derive(Clone, PartialEq, Debug)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub sha256: String,
}

impl Release {
    /// Linha trocada entre o filho e o app: "versão url sha256".
    fn to_line(&self) -> String {
        format!("{} {} {}", self.version, self.url, self.sha256)
    }

    pub fn from_line(line: &str) -> Option<Release> {
        let mut parts = line.split_whitespace();
        let (version, url, sha256) = (parts.next()?, parts.next()?, parts.next()?);
        let release = Release { version: version.into(), url: url.into(), sha256: sha256.into() };
        release.trusted().then_some(release)
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

/// Lê a resposta da API de releases do GitHub.
fn parse_release(api: &Json) -> Option<Release> {
    if api.get("draft") == Some(&Json::Bool(true)) || api.get("prerelease") == Some(&Json::Bool(true)) {
        return None;
    }
    let version = api.get("tag_name")?.as_str()?.trim_start_matches('v').to_string();
    let asset = api.get("assets")?.as_array().iter().find(|a| a.get("name").and_then(Json::as_str) == Some(ASSET))?;
    let url = asset.get("browser_download_url")?.as_str()?.to_string();
    let sha256 = asset.get("digest")?.as_str()?.strip_prefix("sha256:")?.to_ascii_lowercase();
    let release = Release { version, url, sha256 };
    release.trusted().then_some(release)
}

// --- processos filhos --------------------------------------------------------------

/// Modo `--procurar-versao`: escreve a última release (ou nada) e devolve o código de saída.
pub fn serve_check() -> i32 {
    let found = net::get(LATEST_API, "application/vnd.github+json", MAX_API).and_then(|(status, body)| match status {
        200 => {
            let text = String::from_utf8(body).map_err(|_| "resposta inválida".to_string())?;
            Ok(parse_release(&json::parse(&text)?))
        }
        404 => Ok(None), // ainda sem releases
        other => Err(format!("GitHub respondeu HTTP {other}")),
    });
    report(found.map(|release| release.map(|r| r.to_line()).unwrap_or_default()))
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
    let release = Release::from_line(line).ok_or("endereço de atualização recusado")?;
    let (status, bytes) = net::get(&release.url, "application/octet-stream", MAX_EXE)?;
    if status != 200 {
        return Err(format!("o download falhou (HTTP {status})"));
    }
    if sha256::hex(&bytes) != release.sha256 {
        return Err("o arquivo baixado não confere com o da release (SHA-256)".into());
    }
    if !bytes.starts_with(b"MZ") {
        return Err("o arquivo baixado não é um programa do Windows".into());
    }
    let dir = folder().ok_or("sem pasta de dados")?;
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
    child::spawn(to, msg, || child::run(child::this_app(&[ARG_CHECK]), "", 4096), |reply| reply);
}

/// Baixa a versão `release` num filho; a resposta (caminho do arquivo) chega com `msg`.
pub fn fetch(to: windows_sys::Win32::Foundation::HWND, msg: u32, release: &Release) {
    let line = release.to_line();
    child::spawn(to, msg, move || child::run(child::this_app(&[ARG_DOWNLOAD, &line]), "", 4096), |reply| reply);
}

/// Troca o .exe em uso pelo baixado e abre o novo. Depois disso o app deve fechar.
pub fn install(downloaded: &str) -> Result<(), String> {
    let downloaded = PathBuf::from(downloaded);
    let expected = folder().map(|d| d.join(ASSET));
    if expected.as_ref() != Some(&downloaded) {
        return Err("arquivo de atualização fora do lugar esperado".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let old = exe.with_extension("old.exe");
    let _ = fs::remove_file(&old);
    // O Windows deixa renomear um .exe em uso, mas não sobrescrevê-lo.
    fs::rename(&exe, &old).map_err(|e| format!("não consegui trocar o programa nesta pasta ({e})"))?;
    if let Err(e) = fs::copy(&downloaded, &exe) {
        let _ = fs::rename(&old, &exe); // desfaz
        return Err(format!("não consegui instalar a versão nova ({e})"));
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
            r#"{{"tag_name":"{tag}","draft":false,"prerelease":false,"assets":[{{"name":"dontStayAlone.exe","browser_download_url":"{url}","digest":"sha256:{SHA}"}}]}}"#
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
        assert_eq!(release, Release { version: "1.2.0".into(), url: url.into(), sha256: SHA.into() });
        assert_eq!(Release::from_line(&release.to_line()), Some(release));
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
