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
use crate::lang::tr;
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
            Size::Tiny => tr("Extra pequeno"),
            Size::Small => tr("Pequeno"),
            Size::Medium => tr("Médio"),
            Size::Large => tr("Grande"),
        }
    }

    /// Rótulo curto, para o seletor do painel.
    pub fn short(self) -> &'static str {
        match self {
            Size::Tiny => "Mini",
            Size::Small => tr("P"),
            Size::Medium => "M",
            Size::Large => tr("G"),
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
    /// Procurar versões novas no GitHub uma vez por dia.
    pub updates: bool,
    /// Seu aniversário (dia, mês), para o mascote comemorar.
    pub birthday: Option<(u32, u32)>,
    /// Ficar quieto enquanto um programa de reunião está em primeiro plano
    /// (olha só o nome do programa, nunca o conteúdo).
    pub quiet_in_meetings: bool,
    /// Segundo mascote na tela (id do pacote; vazio = nenhum).
    pub buddy: String,
    /// O mascote lembra de coisas que você contou na conversa.
    pub memory: bool,
    pub theme: Theme,
    pub language: Language,
    /// Ctrl+Alt+M abre a conversa (atalho só dessa combinação, sem ler o teclado).
    pub chat_hotkey: bool,
    /// Acessórios de época (gorro no Natal, chapéu de bruxa no Halloween...).
    pub accessories: bool,
}

/// Claro, escuro ou o mesmo do Windows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Auto,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Auto, Theme::Light, Theme::Dark];

    pub fn key(self) -> &'static str {
        match self {
            Theme::Auto => "auto",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

/// Idioma da interface: o do Windows, português ou inglês.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Language {
    Auto,
    Portuguese,
    English,
}

impl Language {
    pub const ALL: [Language; 3] = [Language::Auto, Language::Portuguese, Language::English];

    pub fn key(self) -> &'static str {
        match self {
            Language::Auto => "auto",
            Language::Portuguese => "pt",
            Language::English => "en",
        }
    }
}

/// "25/12" → (25, 12), só datas que existem.
pub fn parse_birthday(text: &str) -> Option<(u32, u32)> {
    let (day, month) = text.trim().split_once('/')?;
    let (day, month) = (day.trim().parse::<u32>().ok()?, month.trim().parse::<u32>().ok()?);
    let days = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    (1..=12).contains(&month).then_some(())?;
    (1..=days[month as usize - 1]).contains(&day).then_some((day, month))
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

/// Já existe um config.ini? (senão, é a primeira vez: o app mostra as boas-vindas)
pub fn exists() -> bool {
    dir().is_some_and(|d| d.join("config.ini").is_file())
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
            updates: true,
            birthday: None,
            quiet_in_meetings: false,
            buddy: String::new(),
            memory: true,
            theme: Theme::Auto,
            language: Language::Auto,
            chat_hotkey: true,
            accessories: true,
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
                "updates" => config.updates = value != "off",
                "birthday" => config.birthday = parse_birthday(value),
                "quiet_in_meetings" => config.quiet_in_meetings = value == "on",
                "buddy" => config.buddy = value.to_lowercase(),
                "memory" => config.memory = value != "off",
                "theme" => config.theme = Theme::ALL.into_iter().find(|t| t.key() == value).unwrap_or(Theme::Auto),
                "language" => config.language = Language::ALL.into_iter().find(|l| l.key() == value).unwrap_or(Language::Auto),
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
                "water_goal" => config.companion.water_goal = number.unwrap_or(8).min(30),
                "focus_minutes" => config.companion.focus_minutes = number.unwrap_or(25).clamp(1, 180),
                "break_minutes" => config.companion.break_minutes = number.unwrap_or(5).clamp(1, 60),
                "focus_quiet" => config.companion.focus_holds_reminders = value != "off",
                "chat_hotkey" => config.chat_hotkey = value != "off",
                "accessories" => config.accessories = value != "off",
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
        let on_off = |on: bool| if on { "on" } else { "off" };
        let birthday = self.birthday.map_or(String::new(), |(d, m)| format!("{d:02}/{m:02}"));
        text += &format!(
            "# procurar versões novas no GitHub uma vez por dia: on | off\n\
             updates={}\n\
             # seu aniversário (dia/mês), para o mascote comemorar\n\
             birthday={birthday}\n\
             # ficar quieto com programa de reunião aberto (Teams, Zoom...): on | off\n\
             quiet_in_meetings={}\n\
             # segundo mascote na tela (vazio = nenhum)\n\
             buddy={}\n\
             # o mascote lembra do que você contou na conversa: on | off\n\
             memory={}\n\
             # tema: auto | light | dark    idioma: auto | pt | en\n\
             theme={}\n\
             language={}\n\
             # meta de copos d'água por dia (0 = sem meta)\n\
             water_goal={}\n\
             # foco (pomodoro): minutos de foco e de pausa; focus_quiet = lembretes esperam a pausa\n\
             focus_minutes={}\n\
             break_minutes={}\n\
             focus_quiet={}\n\
             # Ctrl+Alt+M abre a conversa: on | off\n\
             chat_hotkey={}\n\
             # acessórios de época no mascote (Natal, Halloween, aniversário...): on | off\n\
             accessories={}\n",
            on_off(self.updates),
            on_off(self.quiet_in_meetings),
            self.buddy,
            on_off(self.memory),
            self.theme.key(),
            self.language.key(),
            self.companion.water_goal,
            self.companion.focus_minutes,
            self.companion.break_minutes,
            on_off(self.companion.focus_holds_reminders),
            on_off(self.chat_hotkey),
            on_off(self.accessories)
        );
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
    fn new_options_round_trip() {
        let mut c = Config::parse("");
        assert!(c.updates && c.memory && !c.quiet_in_meetings && c.birthday.is_none());
        c.updates = false;
        c.birthday = Some((7, 3));
        c.quiet_in_meetings = true;
        c.buddy = "lance".into();
        c.memory = false;
        c.theme = Theme::Dark;
        c.language = Language::English;
        let back = Config::parse(&c.to_text());
        assert!(!back.updates && !back.memory && back.quiet_in_meetings);
        assert_eq!((back.birthday, back.buddy.as_str(), back.theme, back.language), (Some((7, 3)), "lance", Theme::Dark, Language::English));
    }

    #[test]
    fn birthdays_must_exist() {
        assert_eq!(parse_birthday("29/2"), Some((29, 2)));
        assert_eq!(parse_birthday(" 5 / 12 "), Some((5, 12)));
        assert!(parse_birthday("31/4").is_none() && parse_birthday("0/1").is_none() && parse_birthday("1/13").is_none());
        assert!(parse_birthday("amanhã").is_none());
    }

    #[test]
    fn old_config_without_new_keys_still_loads() {
        let c = Config::parse("size=small\nwater=off\nwater_minutes=30\n");
        assert!(c.size == Size::Small && c.mascot == crate::pack::DEFAULT);
        assert_eq!((c.companion.reminders[0].on, c.companion.reminders[0].minutes), (false, 30));
        assert_eq!(c.companion.reminders.len(), 3);
    }
}
