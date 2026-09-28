//! Galeria da comunidade: **só mascotes**, publicados na pasta `gallery/mascots/`
//! do repositório do app, com um `index.json` que diz o SHA-256 de cada arquivo.
//!
//! Mascote é só desenho e falas em texto: nada ali roda no seu PC. Por isso a galeria
//! aceita apenas `mascot.txt`, `phrases.txt` e `phrases_en.txt` — plugins (programas)
//! ficam de fora de propósito.
//!
//! Tudo que usa a rede roda num processo filho (`--galeria`, `--instalar-galeria`).
//! Os arquivos só vêm deste repositório, são conferidos um a um e só são gravados
//! depois que todos conferem.

use std::fs;

use crate::{
    ai::json::{self, Json},
    child::{self, Reply},
    lang::{fill, tr},
    net, pack, sha256,
    win::clean_line,
};

pub const ARG_LIST: &str = "--galeria";
pub const ARG_INSTALL: &str = "--instalar-galeria";

/// Único lugar de onde vêm os arquivos da galeria.
const BASE: &str = "https://raw.githubusercontent.com/4TyllaL/notStayAlone/main/gallery/";
const MAX_INDEX: usize = 256 * 1024;
const MAX_FILE: usize = 256 * 1024;
const MAX_ITEMS: usize = 200;
/// Os únicos arquivos que um mascote da galeria pode ter.
const FILES: [&str; 3] = ["mascot.txt", "phrases.txt", "phrases_en.txt"];

/// Um mascote da galeria.
#[derive(Clone, PartialEq, Debug)]
pub struct Item {
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

/// Exatamente `mascots/<id>/<um dos FILES>`.
fn valid_path(path: &str, id: &str) -> bool {
    let mut parts = path.split('/');
    let (Some(folder), Some(item), Some(file), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    folder == "mascots" && item == id && FILES.contains(&file)
}

/// Lê o `index.json`, descartando o que não passar nas regras (inclusive o que não
/// for mascote).
fn parse_index(text: &str) -> Result<Vec<Item>, String> {
    let root = json::parse(text)?;
    let items = root.get("items").map(Json::as_array).unwrap_or(&[]);
    let mut out = Vec::new();
    for entry in items.iter().take(MAX_ITEMS) {
        let text_of = |key: &str, max: usize| entry.get(key).and_then(Json::as_str).map(|s| clean_line(s, max)).unwrap_or_default();
        if entry.get("kind").and_then(Json::as_str) != Some("mascote") {
            continue;
        }
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
                (valid_path(&path, &id) && sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit())).then_some((path, sha))
            })
            .collect();
        // Um arquivo fora das regras invalida o item inteiro (nada de instalar pela metade).
        let listed = entry.get("files").map_or(0, |f| f.as_array().len());
        let has_sprites = files.iter().any(|(p, _)| p.ends_with("/mascot.txt"));
        if !has_sprites || files.len() != listed || files.len() > FILES.len() {
            continue;
        }
        out.push(Item { name: text_of("name", 40), author: text_of("author", 40), about: text_of("about", 200), id, files });
    }
    Ok(out)
}

fn fetch_index() -> Result<Vec<Item>, String> {
    let (status, body) = net::get(&format!("{BASE}index.json"), "application/json", MAX_INDEX)?;
    if status != 200 {
        return Err(fill(tr("a galeria respondeu HTTP {}"), &[&status]));
    }
    parse_index(&String::from_utf8(body).map_err(|_| tr("índice inválido").to_string())?)
}

// --- processos filhos --------------------------------------------------------------

/// Uma linha por item: id, nome, autor e descrição, separados por tabulação.
fn to_lines(items: &[Item]) -> String {
    items.iter().map(|i| [i.id.as_str(), &i.name, &i.author, &i.about].join("\t")).collect::<Vec<_>>().join("\n")
}

/// Modo `--galeria`: escreve a lista de mascotes.
pub fn serve_list() -> i32 {
    report(fetch_index().map(|items| to_lines(&items)))
}

/// Modo `--instalar-galeria <id>`: baixa, confere e instala um mascote.
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
    let item = fetch_index()?.into_iter().find(|i| i.id == id).ok_or(tr("esse item não está mais na galeria"))?;
    // Baixa e confere tudo antes de gravar qualquer coisa.
    let mut downloaded = Vec::new();
    for (path, sha) in &item.files {
        let (status, bytes) = net::get(&format!("{BASE}{path}"), "application/octet-stream", MAX_FILE)?;
        if status != 200 {
            return Err(fill(tr("não consegui baixar {} (HTTP {})"), &[&path, &status]));
        }
        if sha256::hex(&bytes) != *sha {
            return Err(fill(tr("{} não confere com o índice (SHA-256)"), &[&path]));
        }
        let name = path.rsplit('/').next().unwrap_or_default().to_string();
        downloaded.push((name, bytes));
    }
    let folder = pack::user_dir().ok_or(tr("sem pasta de dados"))?.join(&item.id);
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    for (name, bytes) in downloaded {
        fs::write(folder.join(name), bytes).map_err(|e| e.to_string())?;
    }
    Ok(item.id)
}

// --- no app --------------------------------------------------------------------------

/// Um mascote como a janela de configurações mostra (vindo do filho).
#[derive(Clone, PartialEq, Debug)]
pub struct Entry {
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
            let id = parts.next()?.to_string();
            valid_id(&id).then_some(())?;
            let (name, author, about) = (parts.next()?, parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            Some(Entry { id, name: clean_line(name, 40), author: clean_line(author, 40), about: clean_line(about, 200) })
        })
        .collect()
}

/// Busca a lista num filho; a resposta chega à janela `to` com `msg`.
pub fn list(to: windows_sys::Win32::Foundation::HWND, msg: u32) {
    child::spawn(to, msg, || child::run(child::this_app(&[ARG_LIST]), "", 64 * 1024), |reply: Reply| reply);
}

/// Instala `id` num filho; a resposta (o id) chega com `msg`.
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
        assert_eq!(items[0].id, "calcifer-hd");
        let entries = parse_lines(&to_lines(&items));
        assert_eq!(entries[0].name, "Calcifer HD");
    }

    #[test]
    fn only_mascot_files_are_accepted() {
        for path in [
            "mascots/outro/mascot.txt",
            "mascots/x/../../evil.exe",
            "plugins/x/run.ps1",
            "mascots/x/run.ps1",
            "mascots/x/programa.exe",
            "mascots/x/sub/mascot.txt",
            "mascots/x/.hidden",
            "/etc/passwd",
        ] {
            assert!(!valid_path(path, "x"), "aceitou {path}");
        }
        for file in FILES {
            assert!(valid_path(&format!("mascots/x/{file}"), "x"));
        }
    }

    #[test]
    fn plugins_and_bad_entries_are_dropped() {
        let index = format!(
            r#"{{"items":[
                {{"kind":"plugin","id":"p","files":[{{"path":"plugins/p/plugin.ini","sha256":"{SHA}"}}]}},
                {{"kind":"virus","id":"a","files":[{{"path":"mascots/a/mascot.txt","sha256":"{SHA}"}}]}},
                {{"kind":"mascote","id":"Maiúsculo","files":[{{"path":"mascots/Maiúsculo/mascot.txt","sha256":"{SHA}"}}]}},
                {{"kind":"mascote","id":"b","files":[{{"path":"mascots/b/mascot.txt","sha256":"curto"}}]}},
                {{"kind":"mascote","id":"c","files":[{{"path":"mascots/c/phrases.txt","sha256":"{SHA}"}}]}},
                {{"kind":"mascote","id":"d","files":[{{"path":"mascots/d/mascot.txt","sha256":"{SHA}"}},{{"path":"mascots/d/x.exe","sha256":"{SHA}"}}]}}
            ]}}"#
        );
        assert!(parse_index(&index).unwrap().is_empty());
    }

    /// A pasta `gallery/` do repositório: índice em dia e todo mascote válido.
    /// (Pega um pull request com mascote quebrado ou índice desatualizado.)
    #[test]
    fn repository_gallery_is_consistent() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("gallery");
        let items = parse_index(&fs::read_to_string(root.join("index.json")).unwrap()).unwrap();
        let folders: Vec<_> = fs::read_dir(root.join("mascots")).unwrap().flatten().collect();
        assert_eq!(items.len(), folders.len(), "rode python tools/gallery.py");
        assert!(!root.join("plugins").exists(), "a galeria é só de mascotes");
        for item in &items {
            for (path, sha) in &item.files {
                let bytes = fs::read(root.join(path)).unwrap();
                assert_eq!(&sha256::hex(&bytes), sha, "{path}: rode python tools/gallery.py");
                let text = String::from_utf8(bytes).unwrap();
                if path.ends_with("/mascot.txt") {
                    crate::sprite::Art::parse(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
                } else {
                    crate::phrases::Phrases::parse_partial(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
                }
            }
        }
    }
}
