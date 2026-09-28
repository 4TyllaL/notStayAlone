//! Plugin de conversa do !StayAlone.
//!
//! Protocolo (qualquer programa pode ser um plugin):
//!   stdin : {"system": "...", "messages": [{"role": "user"|"assistant", "content": "..."}], "max_tokens"?: n}
//!   stdout: a resposta, em texto puro (UTF-8)
//!   erro  : mensagem curta no stderr e cÃ³digo de saÃ­da != 0
//!
//! Este plugin fala com qualquer API no padrÃ£o OpenAI (`/chat/completions`).
//! O provedor fica em %APPDATA%\StayAlone\chat.ini (Gemini por padrÃ£o); o plugin
//! sÃ³ lÃª arquivos, nunca escreve.

mod http;
mod json;

use std::{io::Read, path::PathBuf, ptr::null_mut};

use windows_sys::Win32::{
    Foundation::ERROR_SUCCESS,
    Security::Credentials::{CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC},
    System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ},
};

use json::{quote, Json};

/// Limites do que entra: pedido do app (12 mensagens curtas + instruções), chat.ini
/// e tokens. O teto de tokens cobre o desenho da IA (4096); a conversa usa 1024,
/// porque modelos que "pensam" gastam tokens raciocinando antes de responder.
const MAX_REQUEST: u64 = 64 * 1024;
const MAX_INI: u64 = 16 * 1024;
const MAX_TOKENS: u32 = 4096;

struct Config {
    api_base: String,
    model: String,
    api_key_env: String,
    reasoning_effort: String,
    max_tokens: u32,
}

impl Config {
    fn load() -> Config {
        let path = std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("StayAlone").join("chat.ini"));
        let text = path.and_then(|p| read_limited(std::fs::File::open(p).ok()?, MAX_INI)).unwrap_or_default();
        Self::parse(&text)
    }

    /// Sem chat.ini (ou sem uma linha), vale o padrÃ£o: Gemini.
    fn parse(text: &str) -> Config {
        let mut c = Config {
            api_base: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
            model: "gemini-3.8-flash".into(),
            api_key_env: "GEMINI_API_KEY".into(),
            reasoning_effort: "low".into(),
            max_tokens: 1024,
        };
        for line in text.lines().map(str::trim).filter(|l| !l.starts_with('#')) {
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim().to_string();
            match key.trim() {
                "api_base" => c.api_base = value.trim_end_matches('/').to_string(),
                "model" => c.model = value,
                "api_key_env" => c.api_key_env = value,
                "reasoning_effort" => c.reasoning_effort = value,
                "max_tokens" => c.max_tokens = value.parse().unwrap_or(c.max_tokens),
                _ => {}
            }
        }
        c
    }
}

fn read_limited(source: impl Read, max: u64) -> Option<String> {
    let mut bytes = Vec::new();
    source.take(max + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= max).then_some(())?;
    String::from_utf8(bytes).ok()
}

fn main() {
    match run() {
        Ok(reply) => print!("{reply}"),
        Err(e) => {
            eprint!("{e}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<String, String> {
    let input = read_limited(std::io::stdin(), MAX_REQUEST).ok_or("nÃ£o li a conversa (grande demais ou invÃ¡lida).")?;
    // Alguns programas mandam o BOM do UTF-8 no comeÃ§o.
    let request = json::parse(input.trim_start_matches('\u{feff}'))?;
    let config = Config::load();
    if config.api_base.is_empty() || config.model.is_empty() {
        return Err("chat.ini sem api_base ou model.".into());
    }
    let key = api_key(&config.api_key_env)?;

    let (status, text) = http::post(&format!("{}/chat/completions", config.api_base), key.as_deref(), &body(&config, &request))?;
    let reply = json::parse(&text).map_err(|_| format!("resposta inesperada do servidor (HTTP {status})"))?;
    // Alguns provedores devolvem o erro dentro de uma lista.
    let reply = match &reply {
        Json::Arr(items) if !items.is_empty() => &items[0],
        other => other,
    };
    if status != 200 {
        let message = reply.get("error").and_then(|e| e.get("message")).and_then(Json::as_str);
        let bad_key = message.is_some_and(|m| m.to_lowercase().contains("api key"));
        return Err(match status {
            _ if bad_key => "a chave da API foi recusada â€” confira em ConfiguraÃ§Ãµes â†’ Conversa.".into(),
            401 | 403 => "a chave da API foi recusada â€” confira em ConfiguraÃ§Ãµes â†’ Conversa.".into(),
            429 => "muitas mensagens seguidas (limite da API). Tenta daqui a pouco!".into(),
            _ => format!("HTTP {status}: {}", message.unwrap_or("erro no servidor")),
        });
    }
    let content = reply
        .get("choices")
        .and_then(|c| c.at(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(Json::as_str)
        .map(str::trim)
        .unwrap_or("");
    if content.is_empty() {
        return Err("o modelo nÃ£o respondeu nada (talvez max_tokens baixo demais).".into());
    }
    Ok(content.to_string())
}

/// Monta o corpo no formato OpenAI: o "system" vira a primeira mensagem.
fn body(config: &Config, request: &Json) -> String {
    let mut messages = Vec::new();
    if let Some(system) = request.get("system").and_then(Json::as_str) {
        messages.push(format!("{{\"role\":\"system\",\"content\":{}}}", quote(system)));
    }
    for m in request.get("messages").map(Json::as_array).unwrap_or(&[]) {
        let role = match m.get("role").and_then(Json::as_str) {
            Some("assistant") => "assistant",
            _ => "user",
        };
        let content = m.get("content").and_then(Json::as_str).unwrap_or("");
        messages.push(format!("{{\"role\":\"{role}\",\"content\":{}}}", quote(content)));
    }
    // O app pode pedir mais espaÃ§o (ex.: para desenhar um mascote); vale o maior, atÃ© o teto.
    let asked = match request.get("max_tokens") {
        Some(Json::Num(n)) if n.is_finite() && *n > 0.0 => n.min(MAX_TOKENS as f64) as u32,
        _ => 0,
    };
    let mut body = format!(
        "{{\"model\":{},\"max_tokens\":{},\"messages\":[{}]",
        quote(&config.model),
        config.max_tokens.max(asked).min(MAX_TOKENS),
        messages.join(",")
    );
    if !config.reasoning_effort.is_empty() {
        body += &format!(",\"reasoning_effort\":{}", quote(&config.reasoning_effort));
    }
    body + "}"
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// A chave, nesta ordem: Gerenciador de Credenciais (colada nas ConfiguraÃ§Ãµes),
/// variÃ¡vel de ambiente, ou variÃ¡vel do usuÃ¡rio criada depois (ex.: `setx`).
fn api_key(name: &str) -> Result<Option<String>, String> {
    if name.is_empty() {
        return Ok(None); // provedor local, sem chave
    }
    if !(name.len() <= 64 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')) {
        return Err("api_key_env invÃ¡lido no chat.ini (use letras, nÃºmeros e _).".into());
    }
    let key = vault(name)
        .or_else(|| std::env::var(name).ok())
        .or_else(|| user_env(name))
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .ok_or_else(|| "falta a chave da API: cole em ConfiguraÃ§Ãµes â†’ Conversa.".to_string())?;
    // Uma chave com quebra de linha poderia injetar cabeÃ§alhos HTTP.
    if !key.chars().all(|c| c.is_ascii_graphic()) {
        return Err("a chave da API tem caracteres invÃ¡lidos â€” cole de novo em ConfiguraÃ§Ãµes â†’ Conversa.".into());
    }
    Ok(Some(key))
}

/// Credencial "StayAlone:<nome>" do Gerenciador de Credenciais do Windows.
fn vault(name: &str) -> Option<String> {
    unsafe {
        let mut cred: *mut CREDENTIALW = null_mut();
        if CredReadW(wide(&format!("StayAlone:{name}")).as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) == 0 {
            return None;
        }
        let c = &*cred;
        let key = decode_blob(std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize));
        CredFree(cred.cast());
        key
    }
}

/// O app grava em UTF-8; o `cmdkey` do Windows grava em UTF-16. Aceita os dois.
fn decode_blob(blob: &[u8]) -> Option<String> {
    let utf16 = blob.len().is_multiple_of(2) && !blob.is_empty() && blob.iter().skip(1).step_by(2).all(|&b| b == 0);
    if utf16 {
        let units: Vec<u16> = blob.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16(&units).ok()
    } else {
        String::from_utf8(blob.to_vec()).ok()
    }
}

fn user_env(name: &str) -> Option<String> {
    let (key, value) = (wide("Environment"), wide(name));
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1);
    let value = String::from_utf16_lossy(&buf[..len]);
    buf.fill(0);
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_gemini() {
        let c = Config::parse("");
        assert!(c.api_base.starts_with("https://generativelanguage.googleapis.com/"));
        assert_eq!(c.api_key_env, "GEMINI_API_KEY");
        assert!(!c.model.is_empty());
    }

    #[test]
    fn builds_openai_style_body() {
        let c = Config::parse("");
        let req = json::parse(r#"{"system":"Seja o Lance","messages":[{"role":"user","content":"oi \"vocÃª\""}]}"#).unwrap();
        let body = json::parse(&body(&c, &req)).unwrap();
        let msgs = body.get("messages").unwrap();
        assert_eq!(msgs.at(0).unwrap().get("role").and_then(Json::as_str), Some("system"));
        assert_eq!(msgs.at(1).unwrap().get("content").and_then(Json::as_str), Some("oi \"vocÃª\""));
        assert_eq!(body.get("reasoning_effort").and_then(Json::as_str), Some("low"));
    }

    #[test]
    fn empty_effort_is_not_sent() {
        let c = Config::parse("api_base=http://localhost:11434/v1\nmodel=llama\nreasoning_effort=\n");
        let body = json::parse(&body(&c, &Json::Obj(vec![]))).unwrap();
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn token_requests_are_capped() {
        let c = Config::parse("");
        let req = json::parse(r#"{"messages":[],"max_tokens":1e12}"#).unwrap();
        let body = json::parse(&body(&c, &req)).unwrap();
        assert_eq!(body.get("max_tokens"), Some(&Json::Num(MAX_TOKENS as f64)));
    }

    #[test]
    fn bad_key_names_are_refused() {
        assert!(api_key("PATH;X").is_err());
        assert_eq!(api_key(""), Ok(None));
    }

    #[test]
    fn keys_saved_by_the_app_or_by_cmdkey_are_read() {
        assert_eq!(decode_blob(b"abc-123").as_deref(), Some("abc-123"));
        assert_eq!(decode_blob(b"a\0b\0c\0").as_deref(), Some("abc"));
    }

    #[test]
    fn oversized_input_is_refused() {
        assert!(read_limited(&b"12345"[..], 4).is_none());
        assert_eq!(read_limited(&b"1234"[..], 4).as_deref(), Some("1234"));
    }
}
