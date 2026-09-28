//! Sprites em ASCII: cada frame é uma grade 16×16 de caracteres, onde `.` é
//! transparente e as demais letras são cores definidas na paleta.

pub const SPRITE: usize = 16;
pub const PIXELS: usize = SPRITE * SPRITE;

pub const PROPS: &str = include_str!("../assets/props.txt");

/// Limites para arquivos de mods: nomes vão para menus, `about` vai para a IA.
const MAX_NAME: usize = 32;
const MAX_ABOUT: usize = 200;
const MAX_FRAMES: usize = 64;

/// Nome de mascote limpo (sem caracteres de controle, tamanho de menu).
pub fn clean_name(name: &str) -> String {
    crate::win::clean_line(name, MAX_NAME)
}

/// Folha de sprites genérica: frames com nome, em qualquer quantidade.
pub struct Sheet {
    pub name: Option<String>,
    /// Quem ele é, em uma frase (vira a personalidade na conversa).
    pub about: Option<String>,
    /// "a" para mascotes femininas (falas no feminino); "o" por padrão.
    pub article: Option<String>,
    /// Cor BGRA (alpha pré-multiplicado) indexada pelo caractere ASCII; 0 = transparente.
    colors: [u32; 128],
    names: Vec<String>,
    frames: Vec<[u8; PIXELS]>,
}

impl Sheet {
    pub fn parse(src: &str) -> Result<Sheet, String> {
        let mut sheet =
            Sheet { name: None, about: None, article: None, colors: [0; 128], names: Vec::new(), frames: Vec::new() };
        let mut lines = src
            .lines()
            .enumerate()
            .map(|(i, l)| (i + 1, l.trim()))
            .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'));

        while let Some((n, line)) = lines.next() {
            if let Some(name) = line.strip_prefix("name ") {
                sheet.name = Some(clean_name(name)).filter(|n| !n.is_empty());
                continue;
            }
            if let Some(about) = line.strip_prefix("about ") {
                sheet.about = Some(crate::win::clean_line(about, MAX_ABOUT)).filter(|a| !a.is_empty());
                continue;
            }
            if let Some(article) = line.strip_prefix("article ") {
                sheet.article = Some(article.trim().to_lowercase());
                continue;
            }
            let mut words = line.split_whitespace();
            match (words.next(), words.next(), words.next(), words.next()) {
                (Some("color"), Some(key), Some(hex), None) => {
                    let key = match key.as_bytes() {
                        [b] if b.is_ascii_graphic() && *b != b'.' && *b != b'#' => *b,
                        _ => return Err(format!("linha {n}: cor inválida '{key}'")),
                    };
                    let hex = hex.trim_start_matches('#');
                    let rgb = u32::from_str_radix(hex, 16)
                        .ok()
                        .filter(|_| hex.len() == 6)
                        .ok_or(format!("linha {n}: use RRGGBB, recebi '{hex}'"))?;
                    sheet.colors[key as usize] = 0xFF00_0000 | rgb;
                }
                (Some("frame"), Some(name), None, None) => {
                    if sheet.find(name).is_some() {
                        return Err(format!("linha {n}: frame '{name}' repetido"));
                    }
                    if sheet.frames.len() >= MAX_FRAMES {
                        return Err(format!("linha {n}: mais de {MAX_FRAMES} frames"));
                    }
                    let mut px = [0u8; PIXELS];
                    for row in 0..SPRITE {
                        let (rn, text) =
                            lines.next().ok_or(format!("frame '{name}': faltam linhas"))?;
                        if text.len() != SPRITE {
                            return Err(format!(
                                "linha {rn}: o frame '{name}' precisa de {SPRITE} colunas, tem {}",
                                text.chars().count()
                            ));
                        }
                        for (col, b) in text.bytes().enumerate() {
                            if b == b'.' {
                                continue;
                            }
                            if b >= 128 || sheet.colors[b as usize] == 0 {
                                return Err(format!("linha {rn}: cor '{}' não definida", b as char));
                            }
                            px[row * SPRITE + col] = b;
                        }
                    }
                    sheet.names.push(name.to_string());
                    sheet.frames.push(px);
                }
                _ => return Err(format!("linha {n}: não entendi '{line}'")),
            }
        }
        Ok(sheet)
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    /// Os caracteres de um frame (0 = transparente), para editar no criador de mascotes.
    pub fn frame_chars(&self, name: &str) -> Option<&[u8; PIXELS]> {
        self.find(name).map(|i| &self.frames[i])
    }

    /// Cor RGB (0xRRGGBB) de um caractere da paleta.
    pub fn rgb(&self, ch: u8) -> u32 {
        self.colors.get(ch as usize).map_or(0, |c| c & 0xFF_FFFF)
    }

    /// Desenha o frame ampliado `scale` vezes em `out` (largura SPRITE * scale).
    pub fn draw(&self, index: usize, flip: bool, scale: usize, out: &mut [u32]) {
        let px = &self.frames[index];
        let width = SPRITE * scale;
        for y in 0..SPRITE {
            for x in 0..SPRITE {
                let sx = if flip { SPRITE - 1 - x } else { x };
                let color = self.colors[px[y * SPRITE + sx] as usize];
                for dy in 0..scale {
                    let start = (y * scale + dy) * width + x * scale;
                    out[start..start + scale].fill(color);
                }
            }
        }
    }

    /// Redimensiona um frame para `size`×`size` (usado no ícone da bandeja).
    pub fn draw_fit(&self, index: usize, size: usize) -> Vec<u32> {
        let px = &self.frames[index];
        (0..size * size)
            .map(|i| {
                let (x, y) = (i % size * SPRITE / size, i / size * SPRITE / size);
                self.colors[px[y * SPRITE + x] as usize]
            })
            .collect()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Frame {
    Idle,
    Blink,
    Walk1,
    Walk2,
    Yawn,
    Sleep1,
    Sleep2,
    Held1,
    Held2,
    Fall,
    Land,
    Happy1,
    Happy2,
    Eat1,
    Eat2,
    Idle2,
    Look,
}

/// Nome de cada `Frame` no arquivo e, se for opcional, o frame que o substitui.
const FRAMES: [(&str, Option<Frame>); 17] = [
    ("idle", None),
    ("blink", None),
    ("walk1", None),
    ("walk2", None),
    ("yawn", None),
    ("sleep1", None),
    ("sleep2", None),
    ("held1", None),
    ("held2", None),
    ("fall", None),
    ("land", None),
    ("happy1", None),
    ("happy2", None),
    ("eat1", Some(Frame::Idle)),
    ("eat2", Some(Frame::Blink)),
    ("idle2", Some(Frame::Idle)),
    ("look", Some(Frame::Walk1)),
];

/// A arte de um mascote: a folha + o índice de cada `Frame` nela.
pub struct Art {
    pub name: String,
    pub sheet: Sheet,
    map: [usize; FRAMES.len()],
    /// Comida favorita (frame `food`), se o mascote tiver.
    pub food: Option<usize>,
}

impl Art {
    pub fn parse(src: &str) -> Result<Art, String> {
        let sheet = Sheet::parse(src)?;
        let mut map = [0; FRAMES.len()];
        for (i, (name, fallback)) in FRAMES.iter().enumerate() {
            map[i] = match (sheet.find(name), fallback) {
                (Some(index), _) => index,
                (None, Some(other)) => map[*other as usize],
                (None, None) => return Err(format!("falta o frame '{name}'")),
            };
        }
        Ok(Art {
            name: sheet.name.clone().unwrap_or_else(|| "Mascote".to_string()),
            food: sheet.find("food"),
            sheet,
            map,
        })
    }

    /// Mascote no feminino?
    pub fn female(&self) -> bool {
        self.sheet.article.as_deref() == Some("a")
    }

    /// "o" ou "a", para frases como "Diga algo para a Jujubs".
    pub fn article(&self) -> &'static str {
        if self.female() {
            "a"
        } else {
            "o"
        }
    }

    pub fn index(&self, frame: Frame) -> usize {
        self.map[frame as usize]
    }

    pub fn draw(&self, frame: Frame, flip: bool, scale: usize, out: &mut [u32]) {
        self.sheet.draw(self.index(frame), flip, scale, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::EMBEDDED;

    #[test]
    fn embedded_mascots_are_valid() {
        for (id, mascot, _) in EMBEDDED {
            let art = Art::parse(mascot).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(art.food.is_some(), "{id} sem comida");
            assert_ne!(art.index(Frame::Eat1), art.index(Frame::Idle), "{id} sem frame de comer");
        }
    }

    #[test]
    fn props_are_valid() {
        let props = Sheet::parse(PROPS).unwrap();
        for name in ["ball1", "ball2", "food"] {
            assert!(props.find(name).is_some(), "falta {name}");
        }
    }

    #[test]
    fn optional_frames_fall_back() {
        // Um mod antigo, só com os frames obrigatórios.
        let src = EMBEDDED[0].1;
        let mut kept = String::new();
        let mut skip = false;
        for line in src.lines() {
            if let Some(name) = line.strip_prefix("frame ") {
                skip = ["idle2", "look", "eat1", "eat2", "food"].contains(&name);
            }
            if !skip {
                kept += line;
                kept += "\n";
            }
        }
        let art = Art::parse(&kept).unwrap();
        assert_eq!(art.index(Frame::Eat1), art.index(Frame::Idle));
        assert_eq!(art.index(Frame::Idle2), art.index(Frame::Idle));
        assert_eq!(art.index(Frame::Look), art.index(Frame::Walk1));
        assert!(art.food.is_none());
    }

    #[test]
    fn reports_missing_frame() {
        let src = EMBEDDED[0].1.replacen("frame idle", "frame idle_x", 1);
        assert!(Art::parse(&src).err().unwrap().contains("'idle'"));
    }

    #[test]
    fn reports_wrong_width() {
        let src = EMBEDDED[0].1.replacen("..kkkkkkkkkkkk..", "..kkkkkkkkkkkk.", 1);
        assert!(Sheet::parse(&src).err().unwrap().contains("colunas"));
    }

    #[test]
    fn flip_mirrors_pixels() {
        let sheet = Sheet::parse(EMBEDDED[0].1).unwrap();
        let (mut a, mut b) = (vec![0; PIXELS], vec![0; PIXELS]);
        sheet.draw(2, false, 1, &mut a);
        sheet.draw(2, true, 1, &mut b);
        for y in 0..SPRITE {
            for x in 0..SPRITE {
                assert_eq!(a[y * SPRITE + x], b[y * SPRITE + SPRITE - 1 - x]);
            }
        }
    }
}
