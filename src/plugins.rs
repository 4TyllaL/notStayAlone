//! Plugins: programas que dão novos poderes ao mascote. Cada um fica numa pasta
//! em %APPDATA%\StayAlone\plugins\<id>\ com um `plugin.ini`:
//!
//!   name  = Curiosidades
//!   about = Conta uma curiosidade de vez em quando
//!   kind  = avisos             (ou: conversa)
//!   run   = curiosidades.ps1   (um .exe ou .ps1 dentro da própria pasta)
//!   every = 90                 (avisos: minutos entre uma vez e outra)
//!   internet = sim             (opcional: pede acesso à internet)
//!   ler    = Documentos\Notas   (opcional, repetível: pasta que ele só lê)
//!   gravar = Downloads          (opcional, repetível: pasta que ele lê e grava)
//!
//! - **conversa**: responde quando você conversa com o mascote (o nativo usa a IA).
//! - **avisos**: roda de tempos em tempos; o que escrever no stdout, o mascote fala.
//!
//! Segurança: plugin novo chega desligado. Ao ligar, você confirma e o app guarda a
//! impressão digital (SHA-256) do programa, se ele pode usar a internet e as pastas que
//! você liberou; se o arquivo mudar (ou ele passar a pedir mais), ele não roda até você
//! confirmar de novo. Cada execução roda num AppContainer (`sandbox`): só lê a própria
//! pasta, tem uma pasta de dados só dele e não alcança seus arquivos (fora as pastas
//! liberadas, ver `folders`) nem a rede sem permissão.

use std::{
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use windows_sys::Win32::{Foundation::HWND, System::SystemInformation::GetSystemDirectoryW};

use crate::{
    ai::{self, json::quote},
    child::{self, Program, Reply},
    config::read_file,
    folders::{self, Access, MAX_FOLDERS},
    lang::{self, tr},
    sandbox, sha256,
    win::clean_line,
};

/// O plugin de conversa com IA que vem com o app: o próprio .exe, no modo `--ia`.
pub const NATIVE_ID: &str = "nativo";
const MANIFEST: &str = "plugin.ini";
const MAX_PLUGINS: usize = 50;
/// Programas maiores que isso não são aceitos (a impressão digital lê o arquivo todo).
const MAX_PROGRAM: u64 = 64 * 1024 * 1024;
/// O que um plugin escrever além disso é ignorado (o balão mostra bem menos).
const MAX_OUTPUT: u64 = 16 * 1024;
/// Intervalo dos avisos, em minutos.
pub const MIN_EVERY: u32 = 5;
pub const MAX_EVERY: u32 = 24 * 60;
/// Um aviso fala no máximo isso.
pub const MAX_NOTICE: usize = 200;
/// Resposta de um plugin que mudou depois de aprovado.
pub const CHANGED: &str = "o arquivo do plugin mudou e ele foi pausado. Ligue de novo em Configurações → Plugins se confiar na nova versão.";

const README: &str = include_str!("../assets/plugins/LEIA-ME.txt");
/// Plugin de exemplo, copiado (desligado) para a pasta na primeira vez.
const EXAMPLE: (&str, [(&str, &str); 2]) = (
    "curiosidades",
    [
        ("plugin.ini", include_str!("../assets/plugins/curiosidades/plugin.ini")),
        ("curiosidades.ps1", include_str!("../assets/plugins/curiosidades/curiosidades.ps1")),
    ],
);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Responde à conversa (só um fica ligado por vez).
    Chat,
    /// Fala de tempos em tempos.
    Notice,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Chat => tr("Conversa"),
            Kind::Notice => tr("Avisos"),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub about: String,
    pub kind: Kind,
    /// O programa (.exe ou .ps1), sempre dentro da pasta do plugin.
    pub program: PathBuf,
    /// Minutos entre execuções (avisos).
    pub every: u32,
    /// O `plugin.ini` pede acesso à internet.
    pub internet: bool,
    /// Pastas pedidas, como estão no `plugin.ini` (texto, gravar?).
    pub folders: Vec<(String, bool)>,
}

/// Plugin ligado: o id, a impressão digital aprovada (vazia no nativo), se você
/// liberou a internet para ele e as pastas liberadas.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Enabled {
    pub id: String,
    pub fingerprint: String,
    pub internet: bool,
    pub folders: Vec<Access>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Off,
    On,
    /// Ligado, mas o arquivo mudou desde que você aprovou.
    Changed,
    /// O programa não está mais lá.
    Missing,
}

impl Plugin {
    pub fn native() -> Plugin {
        let exe = std::env::current_exe().unwrap_or_default();
        Plugin {
            id: NATIVE_ID.into(),
            name: tr("Conversa com IA (nativo)").into(),
            about: tr("Responde com a IA escolhida na aba Conversa (Gemini, OpenAI, Ollama...).").into(),
            kind: Kind::Chat,
            program: exe,
            every: 0,
            internet: true,
            folders: Vec::new(),
        }
    }

    pub fn is_native(&self) -> bool {
        self.id == NATIVE_ID
    }

    /// SHA-256 do programa (`None` se não der para ler).
    pub fn fingerprint(&self) -> Option<String> {
        let file = fs::File::open(&self.program).ok()?;
        let mut bytes = Vec::new();
        file.take(MAX_PROGRAM + 1).read_to_end(&mut bytes).ok()?;
        (bytes.len() as u64 <= MAX_PROGRAM).then(|| sha256::hex(&bytes))
    }

    /// O que você aprovou para este plugin (`None` = nativo ou ainda não ligado).
    pub fn approved(&self, enabled: &[Enabled]) -> Option<Enabled> {
        enabled.iter().find(|e| e.id == self.id && !self.is_native()).cloned()
    }

    /// As pastas pedidas, já como caminhos reais (erro se alguma não puder ser liberada).
    pub fn requested_folders(&self) -> Result<Vec<Access>, String> {
        self.folders.iter().map(|(raw, write)| folders::resolve(raw).map(|path| Access { path, write: *write })).collect()
    }

    /// O arquivo é o aprovado e não pede mais do que foi aprovado.
    fn matches(&self, approval: &Enabled) -> bool {
        let folders_ok = self.requested_folders().is_ok_and(|asked| {
            asked.iter().all(|a| approval.folders.iter().any(|ok| ok.path.eq_ignore_ascii_case(&a.path) && (ok.write || !a.write)))
        });
        (!self.internet || approval.internet) && folders_ok && self.fingerprint().as_deref() == Some(approval.fingerprint.as_str())
    }

    pub fn status(&self, enabled: &[Enabled]) -> Status {
        if !self.program.is_file() {
            return Status::Missing;
        }
        match enabled.iter().find(|e| e.id == self.id) {
            None => Status::Off,
            Some(_) if self.is_native() => Status::On,
            Some(e) if self.matches(e) => Status::On,
            Some(_) => Status::Changed,
        }
    }

    /// Como rodar: o nativo é o próprio app (fora do sandbox: precisa da rede e do
    /// Gerenciador de Credenciais); os outros, no AppContainer, o .exe direto ou o .ps1
    /// pelo PowerShell do Windows (System32). Internet só se `internet` (já aprovada).
    fn command(&self, internet: bool) -> Program {
        if self.is_native() {
            return child::this_app(&[ai::ARG]).into();
        }
        let script = self.program.extension().is_some_and(|e| e.eq_ignore_ascii_case("ps1"));
        let (program, args) = if script {
            let flags = ["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"];
            let mut args: Vec<OsString> = flags.iter().map(OsString::from).collect();
            args.push(self.program.clone().into());
            (system_dir().join(r"WindowsPowerShell\v1.0\powershell.exe"), args)
        } else {
            (self.program.clone(), Vec::new())
        };
        let folder = self.program.parent().map(Path::to_path_buf).unwrap_or_default();
        Program::Sandboxed(sandbox::Spec { id: self.id.clone(), program, args, folder, internet })
    }
}

fn system_dir() -> PathBuf {
    let mut buf = [0u16; 260];
    let n = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    PathBuf::from(String::from_utf16_lossy(&buf[..n.min(buf.len())]))
}

/// `%APPDATA%\StayAlone\plugins`
pub fn dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("StayAlone").join("plugins"))
}

/// O nativo primeiro, depois as pastas com `plugin.ini` válido.
pub fn list() -> Vec<Plugin> {
    let mut plugins = vec![Plugin::native()];
    let Some(root) = dir() else { return plugins };
    if !root.exists() {
        prepare_dir();
    }
    let Ok(entries) = fs::read_dir(&root) else { return plugins };
    let mut folders: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.join(MANIFEST).is_file()).collect();
    folders.sort();
    for folder in folders.into_iter().take(MAX_PLUGINS) {
        let Some(id) = folder.file_name().map(|n| n.to_string_lossy().to_lowercase()) else { continue };
        let Some(text) = read_file(&folder.join(MANIFEST)) else { continue };
        if let Ok(plugin) = parse(&id, &folder, &text) {
            plugins.push(plugin);
        }
    }
    plugins
}

fn parse(id: &str, folder: &Path, text: &str) -> Result<Plugin, String> {
    // O id vai para o config.ini: só letras minúsculas, números, - e _.
    let id_ok = id != NATIVE_ID && !id.is_empty() && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    if !id_ok {
        return Err(format!("nome de pasta inválido: {id}"));
    }
    let (mut name, mut about, mut kind, mut run, mut every) = (String::new(), String::new(), None, String::new(), 60);
    let mut internet = false;
    let mut asked: Vec<(String, bool)> = Vec::new();
    // Sem o BOM do UTF-8 (PowerShell 5 e o Bloco de Notas antigo gravam), a 1ª chave se perderia.
    let text = text.trim_start_matches('\u{feff}');
    for line in text.lines().map(str::trim).filter(|l| !l.starts_with('#')) {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        match key.trim() {
            "name" => name = clean_line(value, 40),
            "about" => about = clean_line(value, 200),
            "kind" => {
                kind = match value {
                    "conversa" => Some(Kind::Chat),
                    "avisos" => Some(Kind::Notice),
                    _ => None,
                }
            }
            "run" => run = value.to_string(),
            "every" => every = value.parse::<u32>().unwrap_or(every).clamp(MIN_EVERY, MAX_EVERY),
            "internet" => internet = matches!(value.to_lowercase().as_str(), "sim" | "yes" | "true" | "1"),
            "ler" | "read" | "gravar" | "write" if !value.is_empty() => {
                let write = matches!(key.trim(), "gravar" | "write");
                match asked.iter_mut().find(|(raw, _)| raw.eq_ignore_ascii_case(value)) {
                    Some(existing) => existing.1 |= write,
                    None => asked.push((value.to_string(), write)),
                }
            }
            _ => {}
        }
    }
    let kind = kind.ok_or("kind precisa ser conversa ou avisos")?;
    // Só um arquivo da própria pasta: nada de caminhos, "..", nem outros tipos.
    let extension_ok = |e: &str| run.len() > e.len() && run.to_ascii_lowercase().ends_with(e);
    let file_ok = !run.contains(['/', '\\', ':']) && !run.starts_with('.') && (extension_ok(".exe") || extension_ok(".ps1"));
    if !file_ok {
        return Err("run precisa ser um .exe ou .ps1 dentro da pasta do plugin".into());
    }
    if asked.len() > MAX_FOLDERS {
        return Err(format!("no máximo {MAX_FOLDERS} pastas (ler/gravar)"));
    }
    let name = if name.is_empty() { id.into() } else { name };
    Ok(Plugin { id: id.into(), name, about, kind, program: folder.join(run), every, internet, folders: asked })
}

/// Cria a pasta de plugins (com o LEIA-ME e o exemplo, desligado) e devolve o caminho.
pub fn prepare_dir() -> Option<PathBuf> {
    let root = dir()?;
    fs::create_dir_all(&root).ok()?;
    let readme = root.join("LEIA-ME.txt");
    if !readme.exists() {
        let _ = fs::write(readme, crlf(README));
        let (id, files) = EXAMPLE;
        let folder = root.join(id);
        if fs::create_dir_all(&folder).is_ok() {
            for (name, text) in files {
                let _ = fs::write(folder.join(name), crlf(text));
            }
        }
    }
    Some(root)
}

/// Texto com fim de linha do Windows (seja qual for o do repositório).
fn crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

/// Dá e tira o acesso às pastas conforme o que mudou nos plugins ligados (numa thread:
/// numa pasta grande o Windows demora para aplicar a permissão a cada arquivo).
pub fn sync_folders(old: &[Enabled], new: &[Enabled]) -> Option<std::thread::JoinHandle<()>> {
    let pairs = |list: &[Enabled]| -> Vec<(String, Access)> {
        list.iter().flat_map(|e| e.folders.iter().map(|a| (e.id.clone(), a.clone()))).collect()
    };
    let (old, new) = (pairs(old), pairs(new));
    let same_folder = |(id, a): &(String, Access), (id2, b): &(String, Access)| id == id2 && a.path.eq_ignore_ascii_case(&b.path);
    let revoke: Vec<_> = old.iter().filter(|o| !new.iter().any(|n| same_folder(o, n))).cloned().collect();
    let grant: Vec<_> = new.iter().filter(|n| !old.contains(n)).cloned().collect();
    if revoke.is_empty() && grant.is_empty() {
        return None;
    }
    Some(std::thread::spawn(move || {
        for (id, a) in revoke {
            let _ = sandbox::revoke_folder(&id, Path::new(&a.path));
        }
        for (id, a) in grant {
            let _ = sandbox::allow_folder(&id, Path::new(&a.path), a.write);
        }
    }))
}

/// O que um plugin de avisos recebe no stdin.
pub fn notice_input(mascot: &str, female: bool, hour: u32) -> String {
    let idioma = if lang::is_english() { "en" } else { "pt" };
    format!("{{\"evento\":\"aviso\",\"mascote\":{},\"feminino\":{female},\"hora\":{hour},\"idioma\":\"{idioma}\"}}", quote(mascot))
}

/// Roda o plugin numa thread e entrega `wrap(resposta)` à janela `to` com `msg` (pelo `mailbox`).
/// `approved` = o que você aprovou (conferido antes de rodar); sem aprovação (o botão
/// Testar de um plugin desligado), roda no sandbox sem internet.
pub fn request<T: Send + 'static>(
    to: HWND,
    msg: u32,
    plugin: &Plugin,
    approved: Option<Enabled>,
    input: String,
    wrap: impl FnOnce(Reply) -> T + Send + 'static,
) {
    let plugin = plugin.clone();
    let job = move || match approved {
        Some(approval) if !plugin.matches(&approval) => Err(CHANGED.into()),
        approval => child::run(plugin.command(approval.is_some_and(|a| a.internet)), &input, MAX_OUTPUT),
    };
    child::spawn(to, msg, job, wrap);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> PathBuf {
        PathBuf::from(r"C:\plugins\clima")
    }

    #[test]
    fn reads_a_manifest() {
        let p = parse("clima", &folder(), "name = Clima\nkind = avisos\nrun = clima.exe\nevery = 1\n").unwrap();
        assert_eq!((p.name.as_str(), p.kind, p.every, p.internet), ("Clima", Kind::Notice, MIN_EVERY, false));
        assert!(parse("clima", &folder(), "kind = avisos\nrun = clima.exe\ninternet = sim\n").unwrap().internet);
        assert_eq!(parse("clima", &folder(), "\u{feff}name = Clima\nkind = avisos\nrun = clima.exe").unwrap().name, "Clima");
        let notes = parse("notas", &folder(), "kind = avisos\nrun = n.exe\nler = Documentos\\Notas\ngravar = Downloads\nler = downloads\n").unwrap();
        assert_eq!(notes.folders, vec![("Documentos\\Notas".to_string(), false), ("Downloads".to_string(), true)]);
        let many: String = (0..=MAX_FOLDERS).map(|i| format!("ler = D:\\p{i}\n")).collect();
        assert!(parse("notas", &folder(), &format!("kind = avisos\nrun = n.exe\n{many}")).is_err());
        assert_eq!(p.program, folder().join("clima.exe"));
    }

    #[test]
    fn programs_must_live_inside_the_plugin_folder() {
        for run in [r"..\evil.exe", r"C:\Windows\x.exe", "sub/x.exe", "x.bat", "x.cmd", ".exe", "x.exe.txt"] {
            let manifest = format!("kind = avisos\nrun = {run}\n");
            assert!(parse("clima", &folder(), &manifest).is_err(), "aceitou {run}");
        }
        assert!(parse("clima", &folder(), "kind = conversa\nrun = bot.PS1\n").is_ok());
    }

    #[test]
    fn ids_and_kinds_are_checked() {
        assert!(parse("Clima|x", &folder(), "kind = avisos\nrun = a.exe").is_err());
        assert!(parse(NATIVE_ID, &folder(), "kind = avisos\nrun = a.exe").is_err());
        assert!(parse("clima", &folder(), "kind = hackear\nrun = a.exe").is_err());
    }

    #[test]
    fn example_plugin_is_valid() {
        let (id, files) = EXAMPLE;
        let p = parse(id, &folder(), files[0].1).unwrap();
        assert_eq!(p.kind, Kind::Notice);
        assert!(p.program.ends_with(files[1].0));
    }

    /// O exemplo roda de verdade, como o app roda: PowerShell no sandbox e no job, JSON no stdin.
    #[test]
    fn example_plugin_runs_confined() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join(r"assets\plugins\curiosidades");
        let p = parse(EXAMPLE.0, &folder, EXAMPLE.1[0].1).unwrap();
        let said = child::run(p.command(false), &notice_input("Calcifer", false, 15), MAX_OUTPUT).unwrap();
        assert!(!said.is_empty() && said.len() < MAX_NOTICE * 4, "{said}");
    }

    #[test]
    fn changed_files_are_detected() {
        let dir = std::env::temp_dir().join("stayalone-plugin-test");
        fs::create_dir_all(&dir).unwrap();
        let program = dir.join("p.exe");
        fs::write(&program, "versão 1").unwrap();
        let plugin = Plugin { program: program.clone(), ..parse("teste", &dir, "kind = avisos\nrun = p.exe").unwrap() };
        let enabled = vec![Enabled { id: "teste".into(), fingerprint: plugin.fingerprint().unwrap(), ..Default::default() }];
        assert_eq!(plugin.status(&enabled), Status::On);
        assert_eq!(plugin.status(&[]), Status::Off);
        // Passar a pedir internet depois de aprovado também exige aprovar de novo.
        let online = Plugin { internet: true, ..plugin.clone() };
        assert_eq!(online.status(&enabled), Status::Changed);
        let enabled_online = vec![Enabled { internet: true, ..enabled[0].clone() }];
        assert_eq!(online.status(&enabled_online), Status::On);
        // Pastas: pedir uma pasta nova, ou gravar onde só podia ler, também.
        let documents = folders::resolve("Documentos").unwrap();
        let reader = Plugin { folders: vec![("Documentos".into(), false)], ..plugin.clone() };
        let writer = Plugin { folders: vec![("Documentos".into(), true)], ..plugin.clone() };
        assert_eq!(reader.status(&enabled), Status::Changed);
        let read_ok = vec![Enabled { folders: vec![Access { path: documents.clone(), write: false }], ..enabled[0].clone() }];
        assert_eq!(reader.status(&read_ok), Status::On);
        assert_eq!(writer.status(&read_ok), Status::Changed);
        let write_ok = vec![Enabled { folders: vec![Access { path: documents, write: true }], ..enabled[0].clone() }];
        assert_eq!(writer.status(&write_ok), Status::On);
        assert_eq!(reader.status(&write_ok), Status::On);
        fs::write(&program, "versão 2").unwrap();
        assert_eq!(plugin.status(&enabled), Status::Changed);
        let _ = fs::remove_dir_all(dir);
    }

    /// Ligar dá acesso à pasta aprovada; desligar tira (como o app faz ao salvar).
    #[test]
    fn turning_off_closes_the_folders() {
        let folder = std::env::temp_dir().join("stayalone-plugin-pastas");
        fs::create_dir_all(&folder).unwrap();
        let path = fs::canonicalize(&folder).unwrap().to_string_lossy().trim_start_matches(r"\\?\").to_string();
        let on = vec![Enabled { id: "teste-pastas".into(), folders: vec![Access { path: path.clone(), write: true }], ..Default::default() }];
        let has_ace = || {
            let out = std::process::Command::new("icacls").arg(&path).output().unwrap();
            String::from_utf8_lossy(&out.stdout).contains("S-1-15-2-")
        };
        sync_folders(&[], &on).unwrap().join().unwrap();
        assert!(has_ace(), "ligar não liberou a pasta");
        assert!(sync_folders(&on, &on).is_none(), "nada mudou, nada a fazer");
        sync_folders(&on, &[]).unwrap().join().unwrap();
        assert!(!has_ace(), "desligar não tirou o acesso");
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn notice_input_is_json() {
        assert_eq!(notice_input("Zé \"Z\"", false, 9), r#"{"evento":"aviso","mascote":"Zé \"Z\"","feminino":false,"hora":9,"idioma":"pt"}"#);
    }
}
