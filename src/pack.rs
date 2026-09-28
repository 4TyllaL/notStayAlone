//! Pacotes de mascote: os embutidos no .exe e os mods em pastas.
//!
//! Um pacote é uma pasta com `mascot.txt` (sprites) e, opcionalmente,
//! `phrases.txt` (falas que substituem as padrão só nos tópicos presentes).

use std::{fs, path::PathBuf};

use crate::{config::read_file, sprite::Art};

/// Falas no feminino, aplicadas a mascotes com `article a`.
pub const FEMININE: &str = include_str!("../assets/phrases_feminine.txt");

/// (id, sprites, falas próprias)
pub const EMBEDDED: [(&str, &str, &str); 4] = [
    (
        "calcifer",
        include_str!("../assets/mascots/calcifer/mascot.txt"),
        include_str!("../assets/mascots/calcifer/phrases.txt"),
    ),
    (
        "lance",
        include_str!("../assets/mascots/lance/mascot.txt"),
        include_str!("../assets/mascots/lance/phrases.txt"),
    ),
    (
        "zeze",
        include_str!("../assets/mascots/zeze/mascot.txt"),
        include_str!("../assets/mascots/zeze/phrases.txt"),
    ),
    (
        "jujubs",
        include_str!("../assets/mascots/jujubs/mascot.txt"),
        include_str!("../assets/mascots/jujubs/phrases.txt"),
    ),
];

pub const DEFAULT: &str = "calcifer";
/// Quantos mascotes cabem no menu (os ids do menu reservam 100 posições).
pub const MAX_PACKS: usize = 99;

pub struct PackInfo {
    pub id: String,
    pub name: String,
    /// `None` = embutido.
    dir: Option<PathBuf>,
}

pub struct Pack {
    pub id: String,
    pub art: Art,
    pub phrases: Option<String>,
}

/// `%APPDATA%\StayAlone\mascots`
pub fn user_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("StayAlone").join("mascots"))
}

fn search_dirs() -> Vec<PathBuf> {
    let beside_exe = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join("mascots")));
    beside_exe.into_iter().chain(user_dir()).collect()
}

/// Lê só a linha `name` — listar não precisa carregar os sprites.
fn read_name(src: &str) -> Option<String> {
    src.lines().find_map(|l| l.trim().strip_prefix("name ").map(crate::sprite::clean_name))
}

/// Embutidos primeiro; um mod com o mesmo id de um embutido o substitui.
pub fn list() -> Vec<PackInfo> {
    let mut packs: Vec<PackInfo> = EMBEDDED
        .iter()
        .map(|(id, src, _)| PackInfo { id: id.to_string(), name: read_name(src).unwrap_or(id.to_string()), dir: None })
        .collect();
    for root in search_dirs() {
        let Ok(entries) = fs::read_dir(&root) else { continue };
        let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.join("mascot.txt").is_file()).collect();
        dirs.sort();
        for dir in dirs {
            let Some(id) = dir.file_name().map(|n| n.to_string_lossy().to_lowercase()) else { continue };
            let head = read_file(&dir.join("mascot.txt")).unwrap_or_default();
            let info = PackInfo { name: read_name(&head).unwrap_or(id.clone()), id, dir: Some(dir) };
            if let Some(existing) = packs.iter_mut().find(|p| p.id == info.id) {
                *existing = info;
            } else if packs.len() < MAX_PACKS {
                packs.push(info);
            }
        }
    }
    packs
}

pub fn load(info: &PackInfo) -> Result<Pack, String> {
    let (sprites, phrases) = match &info.dir {
        None => {
            let (_, sprites, phrases) = EMBEDDED.iter().find(|(id, ..)| *id == info.id).ok_or("mascote não encontrado")?;
            (sprites.to_string(), Some(phrases.to_string()))
        }
        Some(dir) => {
            let sprites = read_file(&dir.join("mascot.txt")).ok_or("mascot.txt não abriu (ou passa de 256 KB)")?;
            (sprites, read_file(&dir.join("phrases.txt")))
        }
    };
    let art = Art::parse(&sprites).map_err(|e| format!("mascot.txt: {e}"))?;
    Ok(Pack { id: info.id.clone(), art, phrases })
}

/// Cria a pasta de mods (com instruções) e devolve o caminho.
pub fn prepare_user_dir() -> Option<PathBuf> {
    let dir = user_dir()?;
    fs::create_dir_all(&dir).ok()?;
    let readme = dir.join("LEIA-ME.txt");
    if !readme.exists() {
        let mut text = String::from(
            "Mascotes do !StayAlone\r\n\
             ======================\r\n\r\n\
             Cada pasta aqui dentro vira um mascote no menu \"Mascote\".\r\n\
             Dentro da pasta:\r\n\
             \x20 mascot.txt   os sprites (obrigatório)\r\n\
             \x20 phrases.txt  falas próprias (opcional; só os [tópicos] que quiser mudar)\r\n\r\n\
             Para começar, copie o mascot.txt de um dos mascotes abaixo para uma pasta nova,\r\n\
             troque a linha \"name\" e as cores/desenhos. Uma pasta com o mesmo nome de um\r\n\
             mascote embutido (calcifer, lance, zeze, jujubs) o substitui.\r\n\r\n\
             Mascotes embutidos, para servir de modelo:\r\n",
        );
        for (id, sprites, _) in EMBEDDED {
            let _ = fs::write(dir.join(format!("_modelo_{id}.txt")), sprites);
            text += &format!("  _modelo_{id}.txt\r\n");
        }
        let _ = fs::write(readme, text);
    }
    Some(dir)
}
