//! Plugins: programas que dão novos poderes ao mascote. Cada um fica numa pasta
//! em %APPDATA%\StayAlone\plugins\<id>\ com um `plugin.ini`:
//!
//!   name  = Curiosidades
//!   about = Conta uma curiosidade de vez em quando
//!   kind  = avisos             (ou: conversa)
//!   run   = curiosidades.ps1   (um .exe ou .ps1 dentro da própria pasta)
//!   every = 90                 (avisos: minutos entre uma vez e outra)
//!
//! - **conversa**: responde quando você conversa com o mascote (o nativo usa a IA).
//! - **avisos**: roda de tempos em tempos; o que escrever no stdout, o mascote fala.
//!
//! Segurança: plugin novo chega desligado. Ao ligar, você confirma e o app guarda a
//! impressão digital (SHA-256) do programa; se o arquivo mudar, ele não roda até
//! você confirmar de novo. Cada execução tem tempo e tamanho de saída limitados.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

use windows_sys::Win32::{Foundation::HWND, System::SystemInformation::GetSystemDirectoryW};

use crate::{
    ai::{self, json::quote},
    child::{self, Reply},
    config::read_file,
    lang::{self, tr},
    sha256,
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
}

/// Plugin ligado: o id e a impressão digital aprovada (vazia no nativo).
#[derive(Clone, PartialEq, Debug)]
pub struct Enabled {
    pub id: String,
    pub fingerprint: String,
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

    /// A impressão digital que este plugin precisa ter para rodar (`None` = nativo, não confere).
    pub fn approved(&self, enabled: &[Enabled]) -> Option<String> {
        enabled.iter().find(|e| e.id == self.id && !self.is_native()).map(|e| e.fingerprint.clone())
    }

    pub fn status(&self, enabled: &[Enabled]) -> Status {
        if !self.program.is_file() {
            return Status::Missing;
        }
        match enabled.iter().find(|e| e.id == self.id) {
            None => Status::Off,
            Some(_) if self.is_native() => Status::On,
            Some(e) if self.fingerprint().as_deref() == Some(e.fingerprint.as_str()) => Status::On,
            Some(_) => Status::Changed,
        }
    }

    /// Como rodar: o .exe direto, ou o .ps1 pelo PowerShell do Windows (System32).
    fn command(&self) -> Command {
        let script = self.program.extension().is_some_and(|e| e.eq_ignore_ascii_case("ps1"));
        let mut command = if script {
            let mut c = Command::new(system_dir().join(r"WindowsPowerShell\v1.0\powershell.exe"));
            c.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(&self.program);
            c
        } else if self.is_native() {
            child::this_app(&[ai::ARG])
        } else {
            Command::new(&self.program)
        };
        if let Some(folder) = self.program.parent() {
            command.current_dir(folder);
        }
        command
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
    Ok(Plugin { id: id.into(), name: if name.is_empty() { id.into() } else { name }, about, kind, program: folder.join(run), every })
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

/// O que um plugin de avisos recebe no stdin.
pub fn notice_input(mascot: &str, female: bool, hour: u32) -> String {
    let idioma = if lang::is_english() { "en" } else { "pt" };
    format!("{{\"evento\":\"aviso\",\"mascote\":{},\"feminino\":{female},\"hora\":{hour},\"idioma\":\"{idioma}\"}}", quote(mascot))
}

/// Roda o plugin numa thread e entrega `wrap(resposta)` à janela `to` com `msg` (pelo `mailbox`).
/// `approved` = impressão digital exigida (conferida antes de rodar).
pub fn request<T: Send + 'static>(
    to: HWND,
    msg: u32,
    plugin: &Plugin,
    approved: Option<String>,
    input: String,
    wrap: impl FnOnce(Reply) -> T + Send + 'static,
) {
    let plugin = plugin.clone();
    let job = move || {
        let intact = approved.is_none_or(|expected| plugin.fingerprint().as_deref() == Some(expected.as_str()));
        if intact {
            child::run(plugin.command(), &input, MAX_OUTPUT)
        } else {
            Err(CHANGED.into())
        }
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
        assert_eq!((p.name.as_str(), p.kind, p.every), ("Clima", Kind::Notice, MIN_EVERY));
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

    #[test]
    fn changed_files_are_detected() {
        let dir = std::env::temp_dir().join("stayalone-plugin-test");
        fs::create_dir_all(&dir).unwrap();
        let program = dir.join("p.exe");
        fs::write(&program, "versão 1").unwrap();
        let plugin = Plugin { program: program.clone(), ..parse("teste", &dir, "kind = avisos\nrun = p.exe").unwrap() };
        let enabled = vec![Enabled { id: "teste".into(), fingerprint: plugin.fingerprint().unwrap() }];
        assert_eq!(plugin.status(&enabled), Status::On);
        assert_eq!(plugin.status(&[]), Status::Off);
        fs::write(&program, "versão 2").unwrap();
        assert_eq!(plugin.status(&enabled), Status::Changed);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn notice_input_is_json() {
        assert_eq!(notice_input("Zé \"Z\"", false, 9), r#"{"evento":"aviso","mascote":"Zé \"Z\"","feminino":false,"hora":9,"idioma":"pt"}"#);
    }
}
