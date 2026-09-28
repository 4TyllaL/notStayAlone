//! Configurações em %APPDATA%\StayAlone\config.ini e o "Iniciar com o Windows".

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    ptr::null_mut,
};

use windows_sys::Win32::{Foundation::ERROR_SUCCESS, System::Registry::*};

use crate::companion::{Reminder, ReminderKind, Settings};
use crate::plugins::{Enabled, NATIVE_ID};
use crate::win::{clean_line, w};

/// Nenhum arquivo de texto do app (configurações, mods, falas) passa disso:
/// um arquivo gigante ou corrompido não consegue esgotar a memória.
pub const MAX_FILE: u64 = 256 * 1024;
/// Lembretes seus, no máximo — e o tamanho do texto de cada um.
pub const MAX_REMINDERS: usize = 50;
pub const MAX_REMINDER_TEXT: usize = 120;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Tiny,
    Small,
    Medium,
    Large,
}

impl Size {
    pub const ALL: [Size; 4] = [Size::Tiny, Size::Small, Size::Medium, Size::Large];

    /// Pixels de tela por pixel de sprite em 100% de escala (96 DPI).
    pub fn factor(self) -> i32 {
        match self {
            Size::Tiny => 2,
            Size::Small => 3,
            Size::Medium => 4,
            Size::Large => 6,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Size::Tiny => "Extra pequeno",
            Size::Small => "Pequeno",
            Size::Medium => "Médio",
            Size::Large => "Grande",
        }
    }

    /// Rótulo curto, para o seletor do painel.
    pub fn short(self) -> &'static str {
        match self {
            Size::Tiny => "Mini",
            Size::Small => "P",
            Size::Medium => "M",
            Size::Large => "G",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Size::Tiny => "tiny",
            Size::Small => "small",
            Size::Medium => "medium",
            Size::Large => "large",
        }
    }
}

#[derive(Clone)]
pub struct Config {
    pub size: Size,
    /// Pixels de sprite por passo (1 a 4).
    pub speed: u32,
    /// Pasta/id do mascote escolhido.
    pub mascot: String,
    /// Plugins ligados (com a impressão digital que você aprovou).
    pub plugins: Vec<Enabled>,
    pub companion: Settings,
}

/// Chaves dos lembretes embutidos no config.ini, na ordem de `ReminderKind::BUILT_IN`.
const REMINDER_KEYS: [&str; 3] = ["water", "stretch", "eyes"];

fn dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("StayAlone"))
}

fn write(name: &str, text: &str) {
    let Some(dir) = dir() else { return };
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(dir.join(name), text);
}

fn read(name: &str) -> Option<String> {
    read_file(&dir()?.join(name))
}

/// Lê um arquivo de texto de até `MAX_FILE` bytes (maior que isso é ignorado).
pub fn read_file(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_FILE {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// Afeto e estatísticas do dia (formato em `Companion::save_string`).
pub fn read_state() -> Option<String> {
    read("state.ini")
}

pub fn write_state(text: &str) {
    write("state.ini", text);
}

impl Config {
    pub fn load() -> Config {
        Config::parse(&read("config.ini").unwrap_or_default())
    }

    pub fn save(&self) {
        write("config.ini", &self.to_text());
    }

    fn parse(text: &str) -> Config {
        let mut config = Config {
            size: Size::Medium,
            speed: 1,
            mascot: crate::pack::DEFAULT.to_string(),
            plugins: Vec::new(),
            companion: Settings::default(),
        };
        // Config de antes dos plugins: vale o padrão (só a conversa nativa ligada).
        let mut plugins_saved = false;
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            let number = value.parse::<u32>().ok();
            match key {
                "size" => {
                    if let Some(size) = Size::ALL.into_iter().find(|s| s.key() == value) {
                        config.size = size;
                    }
                }
                "speed" => config.speed = number.unwrap_or(1).clamp(1, 4),
                "mascot" if !value.is_empty() => config.mascot = value.to_lowercase(),
                "plugins" => plugins_saved = true,
                // plugin=id|impressão digital (o nativo não tem)
                "plugin" => {
                    let (id, fingerprint) = value.split_once('|').unwrap_or((value, ""));
                    let fingerprint_ok = fingerprint.len() == 64 && fingerprint.bytes().all(|b| b.is_ascii_hexdigit());
                    if (id == NATIVE_ID || fingerprint_ok) && !config.plugins.iter().any(|e| e.id == id) {
                        config.plugins.push(Enabled { id: id.to_string(), fingerprint: fingerprint.to_lowercase() });
                    }
                }
                "away_minutes" => config.companion.away_minutes = number.unwrap_or(5).clamp(1, 120),
                // reminder=on|30|Conferir o e-mail
                "reminder" => {
                    let mut parts = value.splitn(3, '|');
                    if let (Some(on), Some(Ok(minutes)), Some(text)) =
                        (parts.next(), parts.next().map(|m| m.trim().parse::<u32>()), parts.next())
                    {
                        let text = clean_line(text, MAX_REMINDER_TEXT);
                        let custom = config.companion.reminders.len() - ReminderKind::BUILT_IN.len();
                        if !text.is_empty() && custom < MAX_REMINDERS {
                            config.companion.reminders.push(Reminder {
                                on: on.trim() == "on",
                                minutes: minutes.clamp(1, 24 * 60),
                                kind: ReminderKind::Custom(text),
                            });
                        }
                    }
                }
                _ => {
                    for (i, name) in REMINDER_KEYS.iter().enumerate() {
                        let r = &mut config.companion.reminders[i];
                        if key == *name {
                            r.on = value == "on";
                        } else if key.strip_suffix("_minutes") == Some(name) {
                            r.minutes = number.unwrap_or(r.minutes).clamp(1, 24 * 60);
                        }
                    }
                }
            }
        }
        if !plugins_saved {
            config.plugins = vec![Enabled { id: NATIVE_ID.into(), fingerprint: String::new() }];
        }
        config
    }

    fn to_text(&self) -> String {
        let mut text = format!(
            "# !StayAlone\n\
             # mascot: calcifer | lance | zeze | jujubs | nome da pasta de um mod\n\
             mascot={}\n\
             # size: tiny | small | medium | large\n\
             size={}\n\
             # speed: 1 a 4 (velocidade da caminhada)\n\
             speed={}\n\
             # minutos parado para ele entender que você saiu\n\
             away_minutes={}\n\
             # lembretes: on | off, e o intervalo em minutos de uso\n",
            self.mascot,
            self.size.key(),
            self.speed,
            self.companion.away_minutes
        );
        let on = |r: &Reminder| if r.on { "on" } else { "off" };
        for (name, r) in REMINDER_KEYS.iter().zip(&self.companion.reminders) {
            text += &format!("{name}={}\n{name}_minutes={}\n", on(r), r.minutes);
        }
        text += "# lembretes seus: reminder=on|minutos|texto\n";
        for r in &self.companion.reminders {
            if let ReminderKind::Custom(label) = &r.kind {
                text += &format!("reminder={}|{}|{}\n", on(r), r.minutes, clean_line(label, MAX_REMINDER_TEXT));
            }
        }
        text += "# plugins ligados (Configurações → Plugins): plugin=pasta|impressão digital SHA-256\nplugins=\n";
        for e in &self.plugins {
            text += &if e.fingerprint.is_empty() { format!("plugin={}\n", e.id) } else { format!("plugin={}|{}\n", e.id, e.fingerprint) };
        }
        text
    }
}


// --- conversa (chat.ini, lido pelo plugin padrão) -------------------------------

/// Serviços prontos: (nome, api_base, modelo, nome da chave, reasoning_effort, onde pegar a chave).
pub const PROVIDERS: [(&str, &str, &str, &str, &str, &str); 4] = [
    (
        "Gemini (Google) — recomendado",
        "https://generativelanguage.googleapis.com/v1beta/openai",
        "gemini-3.8-flash",
        "GEMINI_API_KEY",
        "low",
        "https://aistudio.google.com/apikey",
    ),
    ("OpenAI", "https://api.openai.com/v1", "gpt-5-mini", "OPENAI_API_KEY", "low", "https://platform.openai.com/api-keys"),
    ("OpenRouter", "https://openrouter.ai/api/v1", "openrouter/auto", "OPENROUTER_API_KEY", "", "https://openrouter.ai/keys"),
    ("Ollama (roda no seu PC, sem chave)", "http://localhost:11434/v1", "llama3.2", "", "", "https://ollama.com"),
];

#[derive(Clone, PartialEq, Debug)]
pub struct ChatSettings {
    pub api_base: String,
    pub model: String,
    pub key_env: String,
    pub effort: String,
    pub max_tokens: u32,
}

impl ChatSettings {
    pub fn provider(i: usize) -> ChatSettings {
        let (_, base, model, env, effort, _) = PROVIDERS[i];
        ChatSettings { api_base: base.into(), model: model.into(), key_env: env.into(), effort: effort.into(), max_tokens: 1024 }
    }

    /// Qual dos serviços prontos é este (None = personalizado).
    pub fn provider_index(&self) -> Option<usize> {
        PROVIDERS.iter().position(|p| p.1 == self.api_base)
    }

    pub fn load() -> ChatSettings {
        ChatSettings::parse(&read("chat.ini").unwrap_or_default())
    }

    pub fn save(&self) {
        write("chat.ini", &self.to_text());
    }

    fn parse(text: &str) -> ChatSettings {
        let mut c = ChatSettings::provider(0);
        for line in text.lines().map(str::trim).filter(|l| !l.starts_with('#')) {
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim().to_string();
            match key.trim() {
                "api_base" => c.api_base = value.trim_end_matches('/').to_string(),
                "model" => c.model = value,
                "api_key_env" => c.key_env = value,
                "reasoning_effort" => c.effort = value,
                "max_tokens" => c.max_tokens = value.parse().unwrap_or(c.max_tokens),
                _ => {}
            }
        }
        c
    }

    fn to_text(&self) -> String {
        format!(
            "# Plugin de conversa do !StayAlone (editável também em Configurações → Conversa)\n\
             # Funciona com qualquer API compatível com OpenAI (/chat/completions).\n\
             # A chave NUNCA fica aqui: ela fica no Gerenciador de Credenciais do Windows\n\
             # (como \"StayAlone:<api_key_env>\") ou numa variável de ambiente com esse nome.\n\
             # Endereços http:// só são aceitos para serviços no seu PC (localhost).\n\
             api_base={}\n\
             model={}\n\
             api_key_env={}\n\
             # Quanto o modelo \"pensa\" antes de responder (vazio = não enviar).\n\
             reasoning_effort={}\n\
             max_tokens={}\n\
             \n\
             # Outros provedores (troque as três primeiras linhas):\n\
             #   Gemini:     api_base=https://generativelanguage.googleapis.com/v1beta/openai  api_key_env=GEMINI_API_KEY\n\
             #   OpenAI:     api_base=https://api.openai.com/v1         api_key_env=OPENAI_API_KEY\n\
             #   OpenRouter: api_base=https://openrouter.ai/api/v1      api_key_env=OPENROUTER_API_KEY\n\
             #   Ollama:     api_base=http://localhost:11434/v1         api_key_env=   (sem chave)\n",
            self.api_base, self.model, self.key_env, self.effort, self.max_tokens
        )
    }
}

/// Endereço aceito para a API: HTTPS sempre; HTTP só para serviços no próprio PC
/// (a chave nunca viaja sem criptografia pela rede). O plugin confere de novo.
pub fn endpoint_allowed(url: &str) -> bool {
    if let Some(rest) = url.strip_prefix("https://") {
        return !rest.is_empty();
    }
    let Some(rest) = url.strip_prefix("http://") else { return false };
    let authority = rest.split('/').next().unwrap_or("");
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if port.parse::<u16>().is_ok() => host,
        _ => authority,
    };
    matches!(host.to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "[::1]")
}

// --- iniciar com o Windows ------------------------------------------------------

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "StayAlone";

pub fn autostart_enabled() -> bool {
    let mut len = 0u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w(RUN_KEY).as_ptr(),
            w(RUN_VALUE).as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            null_mut(),
            &mut len,
        )
    };
    status == ERROR_SUCCESS
}

pub fn set_autostart(on: bool) {
    unsafe {
        if on {
            let Ok(exe) = std::env::current_exe() else { return };
            // Entre aspas: um caminho com espaços não pode ser lido como outro programa.
            let data = w(&format!("\"{}\"", exe.display()));
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                w(RUN_KEY).as_ptr(),
                w(RUN_VALUE).as_ptr(),
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            );
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, w(RUN_KEY).as_ptr(), w(RUN_VALUE).as_ptr());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_custom_reminders() {
        let mut c = Config::parse("");
        c.size = Size::Tiny;
        c.mascot = "lance".into();
        c.companion.reminders[2].on = true;
        c.companion.reminders.push(Reminder {
            on: false,
            minutes: 30,
            kind: ReminderKind::Custom("Conferir | o e-mail".into()),
        });
        let back = Config::parse(&c.to_text());
        assert_eq!(back.companion, c.companion);
        assert!(back.size == Size::Tiny && back.mascot == "lance");
    }

    #[test]
    fn custom_reminders_are_cleaned_and_capped() {
        let many = "reminder=on|10|oi\u{7}\n".repeat(MAX_REMINDERS + 10);
        let c = Config::parse(&many);
        assert_eq!(c.companion.reminders.len(), ReminderKind::BUILT_IN.len() + MAX_REMINDERS);
        assert_eq!(c.companion.reminders[3].kind.label(), "oi");
    }

    #[test]
    fn chat_settings_round_trip_and_default_to_gemini() {
        let default = ChatSettings::parse("");
        assert_eq!(default.provider_index(), Some(0));
        assert_eq!(default.key_env, "GEMINI_API_KEY");
        let mut c = ChatSettings::provider(3);
        c.model = "qwen3".into();
        let back = ChatSettings::parse(&c.to_text());
        assert_eq!(back, c);
        assert_eq!(back.provider_index(), Some(3));
    }

    #[test]
    fn keys_never_travel_unencrypted_over_the_network() {
        assert!(PROVIDERS.iter().all(|p| endpoint_allowed(p.1)));
        assert!(endpoint_allowed("http://127.0.0.1/v1") && endpoint_allowed("http://[::1]:8080"));
        assert!(!endpoint_allowed("http://api.openai.com/v1"));
        assert!(!endpoint_allowed("http://localhost.evil.com/v1"));
        assert!(!endpoint_allowed("http://localhost@evil.com/v1"));
        assert!(!endpoint_allowed("ftp://x") && !endpoint_allowed("https://"));
    }

    #[test]
    fn plugins_round_trip_and_default_to_native_chat() {
        assert_eq!(Config::parse("").plugins, vec![Enabled { id: NATIVE_ID.into(), fingerprint: String::new() }]);
        let mut c = Config::parse("");
        c.plugins = vec![Enabled { id: "clima".into(), fingerprint: "ab".repeat(32) }];
        assert_eq!(Config::parse(&c.to_text()).plugins, c.plugins);
        c.plugins.clear(); // tudo desligado continua desligado
        assert!(Config::parse(&c.to_text()).plugins.is_empty());
        // Sem impressão digital válida, um plugin de terceiros não liga.
        assert!(Config::parse("plugins=\nplugin=clima|123\n").plugins.is_empty());
    }

    #[test]
    fn oversized_files_are_ignored() {
        let path = std::env::temp_dir().join("stayalone-big-test.txt");
        fs::write(&path, vec![b'a'; MAX_FILE as usize + 1]).unwrap();
        assert!(read_file(&path).is_none());
        fs::write(&path, "ok").unwrap();
        assert_eq!(read_file(&path).as_deref(), Some("ok"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn old_config_without_new_keys_still_loads() {
        let c = Config::parse("size=small\nwater=off\nwater_minutes=30\n");
        assert!(c.size == Size::Small && c.mascot == crate::pack::DEFAULT);
        assert_eq!((c.companion.reminders[0].on, c.companion.reminders[0].minutes), (false, 30));
        assert_eq!(c.companion.reminders.len(), 3);
    }
}
