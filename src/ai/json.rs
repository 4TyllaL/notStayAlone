//! JSON mínimo: o suficiente para montar a requisição e ler a resposta.
//! A resposta vem da internet, então o parser se protege: aninhamento limitado
//! (uma resposta "[[[[..." não estoura a pilha).

/// Níveis de `{`/`[` aceitos um dentro do outro (respostas reais usam uns 6).
const MAX_DEPTH: usize = 64;

#[derive(Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn at(&self, index: usize) -> Option<&Json> {
        match self {
            Json::Arr(items) => items.get(index),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> &[Json] {
        match self {
            Json::Arr(items) => items,
            _ => &[],
        }
    }
}

/// `s` como string JSON (com aspas).
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn parse(src: &str) -> Result<Json, String> {
    let mut p = Parser { s: src.as_bytes(), i: 0, depth: 0 };
    let value = p.value()?;
    p.ws();
    if p.i != p.s.len() {
        return Err(p.err("texto sobrando depois do JSON"));
    }
    Ok(value)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    depth: usize,
}

impl Parser<'_> {
    fn err(&self, what: &str) -> String {
        format!("JSON inválido na posição {}: {what}", self.i)
    }

    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn expect(&mut self, b: u8) -> Result<(), String> {
        self.ws();
        if self.peek() == Some(b) {
            self.i += 1;
            Ok(())
        } else {
            Err(self.err(&format!("esperava '{}'", b as char)))
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.s[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(value)
        } else {
            Err(self.err("valor desconhecido"))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        if self.depth >= MAX_DEPTH {
            return Err(self.err("aninhamento demais"));
        }
        self.depth += 1;
        let value = self.value_inner();
        self.depth -= 1;
        value
    }

    fn value_inner(&mut self) -> Result<Json, String> {
        self.ws();
        match self.peek().ok_or_else(|| self.err("fim inesperado"))? {
            b'{' => {
                self.i += 1;
                let mut fields = Vec::new();
                self.ws();
                if self.peek() == Some(b'}') {
                    self.i += 1;
                    return Ok(Json::Obj(fields));
                }
                loop {
                    self.ws();
                    let key = self.string()?;
                    self.expect(b':')?;
                    fields.push((key, self.value()?));
                    self.ws();
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(Json::Obj(fields));
                        }
                        _ => return Err(self.err("esperava ',' ou '}'")),
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.peek() == Some(b']') {
                    self.i += 1;
                    return Ok(Json::Arr(items));
                }
                loop {
                    items.push(self.value()?);
                    self.ws();
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(Json::Arr(items));
                        }
                        _ => return Err(self.err("esperava ',' ou ']'")),
                    }
                }
            }
            b'"' => Ok(Json::Str(self.string()?)),
            b't' => self.literal("true", Json::Bool(true)),
            b'f' => self.literal("false", Json::Bool(false)),
            b'n' => self.literal("null", Json::Null),
            _ => {
                let start = self.i;
                while self.peek().is_some_and(|b| b.is_ascii_digit() || b"+-.eE".contains(&b)) {
                    self.i += 1;
                }
                std::str::from_utf8(&self.s[start..self.i])
                    .ok()
                    .and_then(|n| n.parse().ok())
                    .map(Json::Num)
                    .ok_or_else(|| self.err("número inválido"))
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self.s.get(self.i..self.i + 4).ok_or_else(|| self.err("\\u incompleto"))?;
        let n = std::str::from_utf8(digits).ok().and_then(|d| u32::from_str_radix(d, 16).ok());
        self.i += 4;
        n.ok_or_else(|| self.err("\\u inválido"))
    }

    fn string(&mut self) -> Result<String, String> {
        if self.peek() != Some(b'"') {
            return Err(self.err("esperava uma string"));
        }
        self.i += 1;
        let mut out = Vec::new();
        loop {
            let b = self.peek().ok_or_else(|| self.err("string sem fim"))?;
            self.i += 1;
            match b {
                b'"' => return String::from_utf8(out).map_err(|_| self.err("UTF-8 inválido")),
                b'\\' => {
                    let e = self.peek().ok_or_else(|| self.err("escape sem fim"))?;
                    self.i += 1;
                    let c = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let mut code = self.hex4()?;
                            // Par substituto (emojis etc.).
                            if (0xD800..0xDC00).contains(&code) && self.s[self.i..].starts_with(b"\\u") {
                                self.i += 2;
                                let low = self.hex4()?;
                                code = 0x10000 + ((code - 0xD800) << 10) + (low.wrapping_sub(0xDC00) & 0x3FF);
                            }
                            char::from_u32(code).unwrap_or('\u{FFFD}')
                        }
                        _ => return Err(self.err("escape desconhecido")),
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                b => out.push(b),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_openai_style_reply() {
        let body = r#"{"id":"x","choices":[{"index":0,"message":{"role":"assistant","content":"Olá! \"Oi\"\nTchau 😀"},"finish_reason":"stop"}],"usage":{"total_tokens":12}}"#;
        let v = parse(body).unwrap();
        let text = v.get("choices").and_then(|c| c.at(0)).and_then(|c| c.get("message")).and_then(|m| m.get("content"));
        assert_eq!(text.and_then(Json::as_str), Some("Olá! \"Oi\"\nTchau 😀"));
    }

    #[test]
    fn quote_round_trips() {
        let s = "aspas \" barra \\ quebra\n acentuação ♥";
        assert_eq!(parse(&quote(s)).unwrap(), Json::Str(s.to_string()));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("{\"a\":}").is_err());
        assert!(parse("[1,2").is_err());
    }

    #[test]
    fn deep_nesting_is_an_error_not_a_crash() {
        let deep = "[".repeat(100_000) + &"]".repeat(100_000);
        assert!(parse(&deep).is_err());
        let ok = "[".repeat(10) + &"]".repeat(10);
        assert!(parse(&ok).is_ok());
    }
}
