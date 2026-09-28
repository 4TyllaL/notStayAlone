//! Conversa com IA (o plugin de conversa nativo). Roda num processo separado:
//! o app abre a si mesmo como `dontStayAlone.exe --ia`, manda a conversa no stdin e
//! lê a resposta no stdout. Assim o processo do mascote nunca toca em rede, e
//! qualquer falha aqui não derruba o mascote.
//!
//! Protocolo (o mesmo dos plugins de conversa de terceiros):
//!   stdin : {"system": "...", "messages": [{"role": "user"|"assistant", "content": "..."}], "max_tokens"?: n}
//!   stdout: a resposta, em texto puro (UTF-8)
//!   erro  : mensagem curta no stderr e código de saída != 0
//!
//! Fala com qualquer API no padrão OpenAI (`/chat/completions`); o provedor vem
//! do chat.ini (Gemini por padrão) e a chave do `secret`.


pub mod json;

use std::io::{Read, Write};

use json::{quote, Json};

use crate::{
    config::ChatSettings,
    lang::{fill, tr},
    secret,
};

/// Argumento que abre o app no modo "conversa com IA".
pub const ARG: &str = "--ia";
/// Tamanho máximo do pedido vindo do app (12 mensagens curtas + instruções).
const MAX_REQUEST: u64 = 64 * 1024;
/// Teto de tokens: cobre o desenho da IA (4096); a conversa usa 1024, porque
/// modelos que "pensam" gastam tokens raciocinando antes de responder.
const MAX_TOKENS: u32 = 4096;

/// Modo `--ia`: atende um pedido e devolve o código de saída do processo.
pub fn serve() -> i32 {
    let reply = read_request().and_then(|request| answer(&request));
    let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
    match reply {
        Ok(text) => {
            let _ = out.write_all(text.as_bytes());
            0
        }
        Err(e) => {
            let _ = err.write_all(e.as_bytes());
            1
        }
    }
}

fn read_request() -> Result<Json, String> {
    let mut bytes = Vec::new();
    std::io::stdin().take(MAX_REQUEST + 1).read_to_end(&mut bytes).map_err(|e| fill(tr("não li a conversa: {}"), &[&e]))?;
    if bytes.len() as u64 > MAX_REQUEST {
        return Err(tr("a conversa veio grande demais.").into());
    }
    let text = String::from_utf8(bytes).map_err(|_| tr("a conversa não veio em UTF-8.").to_string())?;
    // Alguns programas mandam o BOM do UTF-8 no começo.
    json::parse(text.trim_start_matches('\u{feff}'))
}

fn answer(request: &Json) -> Result<String, String> {
    let config = ChatSettings::load();
    if config.api_base.is_empty() || config.model.is_empty() {
        return Err(tr("chat.ini sem api_base ou model.").into());
    }
    let key = secret::read(&config.key_env)?;
    let url = format!("{}/chat/completions", config.api_base);
    let (status, text) = crate::net::post(&url, key.as_deref(), &body(&config, request))?;
    let reply = json::parse(&text).map_err(|_| fill(tr("resposta inesperada do servidor (HTTP {})"), &[&status]))?;
    // Alguns provedores devolvem o erro dentro de uma lista.
    let reply = match &reply {
        Json::Arr(items) if !items.is_empty() => &items[0],
        other => other,
    };
    if status != 200 {
        let message = reply.get("error").and_then(|e| e.get("message")).and_then(Json::as_str);
        let bad_key = message.is_some_and(|m| m.to_lowercase().contains("api key"));
        return Err(match status {
            _ if bad_key => tr("a chave da API foi recusada — confira em Configurações → Conversa.").into(),
            401 | 403 => tr("a chave da API foi recusada — confira em Configurações → Conversa.").into(),
            429 => tr("muitas mensagens seguidas (limite da API). Tenta daqui a pouco!").into(),
            _ => format!("HTTP {status}: {}", message.unwrap_or(tr("erro no servidor"))),
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
        return Err(tr("o modelo não respondeu nada (talvez max_tokens baixo demais).").into());
    }
    Ok(content.to_string())
}

/// Monta o corpo no formato OpenAI: o "system" vira a primeira mensagem.
fn body(config: &ChatSettings, request: &Json) -> String {
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
    // O app pode pedir mais espaço (ex.: para desenhar um mascote); vale o maior, até o teto.
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
    if !config.effort.is_empty() {
        body += &format!(",\"reasoning_effort\":{}", quote(&config.effort));
    }
    body + "}"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gemini() -> ChatSettings {
        ChatSettings::provider(0)
    }

    #[test]
    fn builds_openai_style_body() {
        let req = json::parse(r#"{"system":"Seja o Lance","messages":[{"role":"user","content":"oi \"você\""}]}"#).unwrap();
        let body = json::parse(&body(&gemini(), &req)).unwrap();
        let msgs = body.get("messages").unwrap();
        assert_eq!(msgs.at(0).unwrap().get("role").and_then(Json::as_str), Some("system"));
        assert_eq!(msgs.at(1).unwrap().get("content").and_then(Json::as_str), Some("oi \"você\""));
        assert_eq!(body.get("reasoning_effort").and_then(Json::as_str), Some("low"));
    }

    #[test]
    fn empty_effort_is_not_sent() {
        let local = ChatSettings::provider(3);
        let body = json::parse(&body(&local, &Json::Obj(vec![]))).unwrap();
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn token_requests_are_capped() {
        let req = json::parse(r#"{"messages":[],"max_tokens":1e12}"#).unwrap();
        let body = json::parse(&body(&gemini(), &req)).unwrap();
        assert_eq!(body.get("max_tokens"), Some(&Json::Num(MAX_TOKENS as f64)));
    }
}
