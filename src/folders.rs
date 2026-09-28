//! Pastas que um plugin pode pedir no `plugin.ini` (`ler = ...`, `gravar = ...`).
//!
//! O pedido vira um caminho real (atalhos como `Documentos\Notas` seguem a pasta de
//! verdade do Windows, inclusive no OneDrive; junções e `..` são resolvidos) e só passa
//! se não for amplo nem sensível demais: raiz de disco, a sua pasta de usuário inteira (ou
//! algo acima dela), `AppData`, pastas ocultas como `.ssh`, Windows, Arquivos de Programas
//! e ProgramData ficam de fora, assim como pastas de rede.

use std::path::PathBuf;

use windows_sys::{
    core::{GUID, PWSTR},
    Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{
            SHGetKnownFolderPath, FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music, FOLDERID_Pictures,
            FOLDERID_Videos,
        },
    },
};

use crate::lang::{fill, tr};

/// Um plugin pede no máximo isso de pastas.
pub const MAX_FOLDERS: usize = 5;

/// Uma pasta liberada: o caminho real e se pode gravar.
#[derive(Clone, PartialEq, Debug)]
pub struct Access {
    pub path: String,
    pub write: bool,
}

impl Access {
    /// "Só lê: C:\..." / "Lê e grava: C:\..."
    pub fn label(&self) -> String {
        fill(if self.write { tr("Lê e grava: {}") } else { tr("Só lê: {}") }, &[&self.path])
    }
}

/// Os atalhos aceitos no começo do caminho (em português e em inglês).
const ALIASES: [(&str, GUID); 12] = [
    ("documentos", FOLDERID_Documents),
    ("documents", FOLDERID_Documents),
    ("downloads", FOLDERID_Downloads),
    ("área de trabalho", FOLDERID_Desktop),
    ("area de trabalho", FOLDERID_Desktop),
    ("desktop", FOLDERID_Desktop),
    ("imagens", FOLDERID_Pictures),
    ("pictures", FOLDERID_Pictures),
    ("músicas", FOLDERID_Music),
    ("music", FOLDERID_Music),
    ("vídeos", FOLDERID_Videos),
    ("videos", FOLDERID_Videos),
];

fn known_folder(id: &GUID) -> Option<String> {
    unsafe {
        let mut path: PWSTR = std::ptr::null_mut();
        let ok = SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut path) >= 0;
        let text = ok.then(|| {
            let len = (0..).take_while(|&i| *path.add(i) != 0).count();
            String::from_utf16_lossy(std::slice::from_raw_parts(path, len))
        });
        CoTaskMemFree(path as _);
        text
    }
}

/// Transforma o que está no `plugin.ini` no caminho real da pasta, se ela puder ser liberada.
pub fn resolve(raw: &str) -> Result<String, String> {
    let raw = raw.trim().trim_end_matches(['\\', '/']);
    let (first, rest) = raw.split_once(['\\', '/']).unwrap_or((raw, ""));
    let base = if let Some((_, id)) = ALIASES.iter().find(|(alias, _)| first.to_lowercase() == *alias) {
        known_folder(id).ok_or_else(|| fill(tr("não achei a pasta {}"), &[&first]))?
    } else if raw.len() >= 3 && raw.as_bytes()[0].is_ascii_alphabetic() && &raw[1..3] == ":\\" {
        String::new()
    } else {
        return Err(fill(tr("\"{}\": use um caminho completo (C:\\...) ou comece por Documentos, Downloads, Área de Trabalho, Imagens, Músicas ou Vídeos"), &[&raw]));
    };
    let path = if base.is_empty() { PathBuf::from(raw) } else { PathBuf::from(base).join(rest) };
    let real = std::fs::canonicalize(&path).map_err(|_| fill(tr("a pasta {} não existe"), &[&path.display()]))?;
    if !real.is_dir() {
        return Err(fill(tr("{} não é uma pasta"), &[&path.display()]));
    }
    let real = real.to_string_lossy().to_string();
    let Some(real) = real.strip_prefix(r"\\?\").filter(|r| !r.starts_with("UNC\\")) else {
        return Err(tr("pastas de rede não podem ser liberadas").into());
    };
    allowed(real, &Sensitive::current()).map_err(|why| format!("{real}: {why}"))?;
    Ok(real.to_string())
}

/// As pastas que nunca são liberadas, desta máquina.
struct Sensitive {
    profile: String,
    /// Nem elas, nem nada dentro delas, nem nada acima delas.
    blocked: Vec<String>,
}

impl Sensitive {
    fn current() -> Sensitive {
        let var = |k: &str| std::env::var(k).unwrap_or_default();
        let profile = var("USERPROFILE");
        let blocked = [var("SystemRoot"), var("ProgramFiles"), var("ProgramFiles(x86)"), var("ProgramData"), format!(r"{profile}\AppData")]
            .into_iter()
            .filter(|p| p.len() > 3)
            .collect();
        Sensitive { profile, blocked }
    }
}

/// `path` pode ser liberada? (Caminhos já resolvidos, sem `\\?\`.)
fn allowed(path: &str, s: &Sensitive) -> Result<(), &'static str> {
    let norm = |p: &str| format!("{}\\", p.trim_end_matches('\\').to_lowercase());
    let path = norm(path);
    // "C:\" e afins.
    if path.len() <= 3 {
        return Err(tr("é a raiz de um disco"));
    }
    let profile = norm(&s.profile);
    if !s.profile.is_empty() && profile.starts_with(&path) {
        return Err(tr("é a sua pasta de usuário inteira (ou fica acima dela)"));
    }
    for blocked in s.blocked.iter().map(|b| norm(b)) {
        if path.starts_with(&blocked) || blocked.starts_with(&path) {
            return Err(tr("é uma pasta do Windows, de programas ou de configurações de apps"));
        }
    }
    if let Some(inside) = path.strip_prefix(&profile).filter(|_| !s.profile.is_empty()) {
        if inside.starts_with('.') {
            return Err(tr("é uma pasta oculta de configurações (como .ssh)"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine() -> Sensitive {
        Sensitive {
            profile: r"C:\Users\ana".into(),
            blocked: vec![r"C:\Windows".into(), r"C:\Program Files".into(), r"C:\ProgramData".into(), r"C:\Users\ana\AppData".into()],
        }
    }

    #[test]
    fn broad_or_sensitive_folders_are_refused() {
        for path in [
            r"C:\",
            r"D:\",
            r"C:\Users",
            r"C:\Users\ana",
            r"C:\users\ANA\",
            r"C:\Users\ana\AppData",
            r"C:\Users\ana\AppData\Roaming\StayAlone",
            r"C:\Windows\System32",
            r"C:\Program Files\App",
            r"C:\ProgramData",
            r"C:\Users\ana\.ssh",
            r"C:\Users\ana\.aws\credentials",
        ] {
            assert!(allowed(path, &machine()).is_err(), "liberou {path}");
        }
    }

    #[test]
    fn ordinary_folders_are_allowed() {
        for path in [r"C:\Users\ana\Documents\Notas", r"C:\Users\ana\Downloads", r"D:\Fotos", r"C:\Users\ana\OneDrive\Documentos", r"C:\Users\anabel\Docs"] {
            assert_eq!(allowed(path, &machine()), Ok(()), "{path}");
        }
    }

    #[test]
    fn aliases_and_paths_resolve_on_this_pc() {
        let documents = resolve("Documentos").unwrap();
        assert_eq!(resolve("documents").unwrap(), documents);
        assert!(resolve("Documentos\\pasta-que-nao-existe-123").is_err());
        assert!(resolve("C:\\").is_err());
        assert!(resolve("notas").is_err(), "caminho relativo");
        // `..` não escapa das regras: vira o caminho real antes de conferir.
        assert!(resolve("Documentos\\..").is_err());
        assert!(resolve(&format!("{}\\AppData\\Roaming", std::env::var("USERPROFILE").unwrap())).is_err());
    }
}
