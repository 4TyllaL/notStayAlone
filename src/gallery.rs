//! Galeria da comunidade: mascotes e plugins publicados na pasta `gallery/` do
//! repositório do app, com um `index.json` que diz o SHA-256 de cada arquivo.
//!
//! Tudo que usa a rede roda num processo filho (`--galeria`, `--instalar-galeria`).
//! Os arquivos só vêm deste repositório, são conferidos um a um e só são gravados
//! depois que todos conferem. Plugins chegam desligados (ligar pede confirmação).

use std::{fs, path::PathBuf};

use crate::{
    ai::json::{self, Json},
    child::{self, Reply},
    net, pack, plugins, sha256,
    win::clean_line,
};

pub const ARG_LIST: &str = "--galeria";
pub const ARG_INSTALL: &str = "--instalar-galeria";

/// Único lugar de onde vêm os arquivos da galeria.
const BASE: &str = "https://raw.githubusercontent.com/4TyllaL/notStayAlone/main/gallery/";
const MAX_INDEX: usize = 256 * 1024;
const MAX_FILE: usize = 256 * 1024;
const MAX_ITEMS: usize = 200;
const MAX_FILES: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Mascot,
    Plugin,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Mascot => "Mascote",
            Kind::Plugin => "Plugin",
        }
    }

    fn folder(self) -> &'static str {
        match self {
            Kind::Mascot => "mascots",
            Kind::Plugin => "plugins",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Kind::Mascot => "mascote",
            Kind::Plugin => "plugin",
        }
    }
}

/// Um item da galeria.
#[derive(Clone, PartialEq, Debug)]
pub struct Item {
    pub kind: Kind,
    pub id: String,
    pub name: String,
    pub author: String,
    pub about: String,
    /// (caminho dentro de `gallery/`, SHA-256)
    files: Vec<(String, String)>,
}

fn valid_id(id: &str) -> bool {
    (1..=40).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// `<mascots|plugins>/<id>/<arquivo>`, sem subpastas, `..` nem arquivos ocultos.
fn valid_path(path: &str, kind: Kind, id: &str) -> bool {
    let mut parts = path.split('/');
    let (Some(folder), Some(item), Some(file), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    folder == kind.folder()
        && item == id
        && (1..=60).contains(&file.len())
        && !file.starts_with('.')
        && file.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// Lê o `index.json`, descartando o que não passar nas regras.
fn parse_index(text: &str) -> Result<Vec<Item>, String> {
    let root = json::parse(text)?;
    let items = root.get("items").map(Json::as_array).unwrap_or(&[]);
    let mut out = Vec::new();
    for entry in items.iter().take(MAX_ITEMS) {
        let text_of = |key: &str, max: usize| entry.get(key).and_then(Json::as_str).map(|s| clean_line(s, max)).unwrap_or_default();
        let kind = match entry.get("kind").and_then(Json::as_str) {
            Some("mascote") => Kind::Mascot,
            Some("plugin") => Kind::Plugin,
            _ => continue,
        };
        let id = text_of("id", 40);
        if !valid_id(&id) {
            continue;
        }
        let files: Vec<(String, String)> = entry
            .get("files")
            .map(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|f| {
                let path = f.get("path")?.as_str()?.to_string();
                let sha = f.get("sha256")?.as_str()?.to_ascii_lowercase();
                (valid_path(&path, kind, &id) && sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit())).then_some((path, sha))
            })
            .collect();
        if files.is_empty() || files.len() > MAX_FILES {
            continue;
        }
        out.push(Item { kind, name: text_of("name", 40), author: text_of("author", 40), about: text_of("about", 200), id, files });
    }
    Ok(out)
}

fn fetch_index() -> Result<Vec<Item>, String> {
    let (status, body) = net::get(&format!("{BASE}index.json"), "application/json", MAX_INDEX)?;
    if status != 200 {
        return Err(format!("a galeria respondeu HTTP {status}"));
    }
    parse_index(&String::from_utf8(body).map_err(|_| "índice inválido".to_string())?)
}

// --- processos filhos --------------------------------------------------------------

/// Uma linha por item: tipo, id, nome, autor e descrição, separados por tabulação.
fn to_lines(items: &[Item]) -> String {
    items
        .iter()
        .map(|i| [i.kind.key(), &i.id, &i.name, &i.author, &i.about].join("\t"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Modo `--galeria`: escreve a lista de itens.
pub fn serve_list() -> i32 {
    report(fetch_index().map(|items| to_lines(&items)))
}

/// Modo `--instalar-galeria <id>`: baixa, confere e instala um item.
pub fn serve_install(id: &str) -> i32 {
    report(install(id))
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

fn install(id: &str) -> Result<String, String> {
    let item = fetch_index()?.into_iter().find(|i| i.id == id).ok_or("esse item não está mais na galeria")?;
    // Baixa e confere tudo antes de gravar qualquer coisa.
    let mut downloaded = Vec::new();
    for (path, sha) in &item.files {
        let (status, bytes) = net::get(&format!("{BASE}{path}"), "application/octet-stream", MAX_FILE)?;
        if status != 200 {
            return Err(format!("não consegui baixar {path} (HTTP {status})"));
        }
        if sha256::hex(&bytes) != *sha {
            return Err(format!("{path} não confere com o índice (SHA-256)"));
        }
        let name = path.rsplit('/').next().unwrap_or_default().to_string();
        downloaded.push((name, bytes));
    }
    let root: PathBuf = match item.kind {
        Kind::Mascot => pack::user_dir(),
        Kind::Plugin => plugins::dir(),
    }
    .ok_or("sem pasta de dados")?;
    let folder = root.join(&item.id);
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    for (name, bytes) in downloaded {
        fs::write(folder.join(name), bytes).map_err(|e| e.to_string())?;
    }
    Ok(format!("{}\t{}", item.kind.key(), item.id))
}

// --- no app --------------------------------------------------------------------------

/// Um item como a janela de configurações mostra (vindo do filho).
#[derive(Clone, PartialEq, Debug)]
pub struct Entry {
    pub kind: Kind,
    pub id: String,
    pub name: String,
    pub author: String,
    pub about: String,
}

/// Lê a saída de `--galeria`.
pub fn parse_lines(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let kind = match parts.next()? {
                "mascote" => Kind::Mascot,
                "plugin" => Kind::Plugin,
                _ => return None,
            };
            let id = parts.next()?.to_string();
            valid_id(&id).then_some(())?;
            let (name, author, about) = (parts.next()?, parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            Some(Entry { kind, id, name: clean_line(name, 40), author: clean_line(author, 40), about: clean_line(about, 200) })
        })
        .collect()
}

/// Busca a lista num filho; a resposta chega à janela `to` com `msg`.
pub fn list(to: windows_sys::Win32::Foundation::HWND, msg: u32) {
    child::spawn(to, msg, || child::run(child::this_app(&[ARG_LIST]), "", 64 * 1024), |reply: Reply| reply);
}

/// Instala `id` num filho; a resposta ("tipo\tid") chega com `msg`.
pub fn install_in_child(to: windows_sys::Win32::Foundation::HWND, msg: u32, id: &str) {
    let id = id.to_string();
    child::spawn(to, msg, move || child::run(child::this_app(&[ARG_INSTALL, &id]), "", 4096), |reply: Reply| reply);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "3df5fa859ff86fe38edce2c2b997ee24ac11cb7c1d13ac981cfe4684198ed1bc";

    #[test]
    fn reads_a_valid_index() {
        let index = format!(
            r#"{{"items":[{{"kind":"mascote","id":"calcifer-hd","name":"Calcifer HD","author":"4TyllaL","about":"gatinho em alta","files":[{{"path":"mascots/calcifer-hd/mascot.txt","sha256":"{SHA}"}}]}}]}}"#
        );
        let items = parse_index(&index).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!((items[0].kind, items[0].id.as_str()), (Kind::Mascot, "calcifer-hd"));
        let entries = parse_lines(&to_lines(&items));
        assert_eq!(entries[0].name, "Calcifer HD");
    }

    #[test]
    fn paths_cannot_escape_the_item_folder() {
        for path in [
            "mascots/outro/mascot.txt",
            "mascots/x/../../evil.exe",
            "plugins/x/run.ps1",
            "mascots/x/sub/mascot.txt",
            "mascots/x/.hidden",
            "mascots/x/a b.txt",
            "/etc/passwd",
        ] {
            assert!(!valid_path(path, Kind::Mascot, "x"), "aceitou {path}");
        }
        assert!(valid_path("mascots/x/mascot.txt", Kind::Mascot, "x"));
        assert!(valid_path("plugins/x/frase.ps1", Kind::Plugin, "x"));
    }

    #[test]
    fn bad_entries_are_dropped() {
        let index = format!(
            r#"{{"items":[
                {{"kind":"virus","id":"a","files":[{{"path":"mascots/a/m.txt","sha256":"{SHA}"}}]}},
                {{"kind":"mascote","id":"Maiúsculo","files":[{{"path":"mascots/Maiúsculo/m.txt","sha256":"{SHA}"}}]}},
                {{"kind":"mascote","id":"b","files":[{{"path":"mascots/b/m.txt","sha256":"curto"}}]}},
                {{"kind":"plugin","id":"c","files":[]}}
            ]}}"#
        );
        assert!(parse_index(&index).unwrap().is_empty());
    }
}
