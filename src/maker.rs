//! Criador de mascotes: um único desenho 16×16 vira um mascote completo.
//! As outras poses (piscar, dormir, andar, cair, feliz...) são geradas
//! automaticamente a partir dele. Lógica pura, sem Win32.

use crate::{
    sprite::{clean_name, Sheet, PIXELS, SPRITE},
    win::clean_line,
};

/// Cores além do transparente.
pub const MAX_COLORS: usize = 15;

/// Paleta inicial de um desenho em branco (índice 0 = transparente).
/// O índice 2 é a cor dos olhos: pintar os olhos com ela faz o mascote piscar.
pub const BLANK_PALETTE: [u32; 15] = [
    0, 0x2b1e2f, 0x111111, 0xffffff, 0xfff3d9, 0xffd23f, 0xf4a259, 0xef476f, 0xf59bb0, 0x9b5de5,
    0x4cc9f0, 0x7ccf6a, 0x3a7d44, 0x8d5a3b, 0x9aa0a6,
];
pub const BLANK_EYES: u8 = 2;

const Z_PATTERN: [&str; 4] = ["zzzz", "..z.", ".z..", "zzzz"];
const HEART_PATTERN: [&str; 4] = ["hh.hh", "hhhhh", ".hhh.", "..h.."];
/// Valores internos para os enfeites (fora da paleta).
const Z: u8 = 254;
const HEART: u8 = 255;

type Grid = [u8; PIXELS];

/// Poses que dá para desenhar. Só a parada é obrigatória: as outras, se ficarem
/// em branco, são criadas a partir dela.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pose {
    Idle,
    Sleep,
    Eat,
    Happy,
    Walk,
}

impl Pose {
    pub const ALL: [Pose; 5] = [Pose::Idle, Pose::Sleep, Pose::Eat, Pose::Happy, Pose::Walk];

    pub fn label(self) -> &'static str {
        match self {
            Pose::Idle => "Parado (a base)",
            Pose::Sleep => "Dormindo",
            Pose::Eat => "Comendo",
            Pose::Happy => "Feliz",
            Pose::Walk => "Andando",
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Drawing {
    pub name: String,
    pub about: String,
    pub female: bool,
    /// Cores 0xRRGGBB; a posição 0 é o transparente (valor ignorado).
    pub palette: Vec<u32>,
    /// A pose parada (a base de tudo).
    pub px: Grid,
    /// Poses extras desenhadas (dormindo, comendo, feliz, andando); `None` = automática.
    pub extra: [Option<Grid>; 4],
    /// Cor dedicada aos olhos, se houver.
    pub eyes: Option<u8>,
}

impl Drawing {
    pub fn blank() -> Drawing {
        Drawing {
            name: String::new(),
            about: String::new(),
            female: false,
            palette: BLANK_PALETTE.to_vec(),
            px: [0; PIXELS],
            extra: [None; 4],
            eyes: Some(BLANK_EYES),
        }
    }

    /// A grade de uma pose (`None` = pose extra ainda não desenhada).
    pub fn grid(&self, pose: Pose) -> Option<&Grid> {
        match pose {
            Pose::Idle => Some(&self.px),
            other => self.extra[other as usize - 1].as_ref(),
        }
    }

    /// A grade de uma pose para editar (uma pose extra começa vazia).
    pub fn grid_mut(&mut self, pose: Pose) -> &mut Grid {
        match pose {
            Pose::Idle => &mut self.px,
            other => self.extra[other as usize - 1].get_or_insert([0; PIXELS]),
        }
    }

    /// Apaga uma pose (a extra volta a ser automática).
    pub fn clear(&mut self, pose: Pose) {
        match pose {
            Pose::Idle => self.px = [0; PIXELS],
            other => self.extra[other as usize - 1] = None,
        }
    }

    /// A pose extra desenhada, se tiver algo nela.
    fn drawn(&self, pose: Pose) -> Option<Grid> {
        self.grid(pose).filter(|g| g.iter().any(|&p| p != 0)).map(settle)
    }

    /// Começa a partir do frame `idle` de um mascote existente (um 32×32 é reduzido).
    pub fn from_sheet(sheet: &Sheet, female: bool) -> Option<Drawing> {
        let chars = shrink(sheet.frame_chars("idle")?, sheet.size);
        let mut palette = vec![0u32];
        let mut map = [0u8; 128];
        let mut px = [0u8; PIXELS];
        for (i, &c) in chars.iter().enumerate() {
            if c == 0 {
                continue;
            }
            if map[c as usize] == 0 {
                if palette.len() <= MAX_COLORS {
                    palette.push(sheet.rgb(c));
                    map[c as usize] = (palette.len() - 1) as u8;
                } else {
                    map[c as usize] = 1;
                }
            }
            px[i] = map[c as usize];
        }
        Some(Drawing {
            name: sheet.name.clone().unwrap_or_default(),
            about: sheet.about.clone().unwrap_or_default(),
            female,
            palette,
            px,
            extra: [None; 4],
            eyes: (map[b'e' as usize] != 0).then_some(map[b'e' as usize]),
        })
    }

    /// Lê o desenho que a IA mandou (formato do mascot.txt, com tolerância).
    pub fn from_ai(text: &str) -> Result<Drawing, String> {
        let mut d = Drawing { palette: vec![0], eyes: None, ..Drawing::blank() };
        let mut keys: Vec<u8> = Vec::new();
        let mut rows: Vec<&str> = Vec::new();
        for raw in text.lines() {
            let line = raw.trim().trim_matches('`').trim();
            if let Some(v) = line.strip_prefix("name ") {
                d.name = clean_name(v);
            } else if let Some(v) = line.strip_prefix("about ") {
                d.about = clean_line(v, 120);
            } else if let Some(v) = line.strip_prefix("article ") {
                d.female = v.trim() == "a";
            } else if let Some(v) = line.strip_prefix("color ") {
                let mut parts = v.split_whitespace();
                let (Some(key), Some(hex)) = (parts.next(), parts.next()) else { continue };
                let hex = hex.trim_start_matches('#');
                let (Some(&k), Ok(rgb)) = (key.as_bytes().first(), u32::from_str_radix(hex, 16)) else { continue };
                if key.len() == 1 && k.is_ascii_alphabetic() && hex.len() == 6 && !keys.contains(&k) && keys.len() < MAX_COLORS {
                    keys.push(k);
                    d.palette.push(rgb);
                }
            } else if (10..=20).contains(&line.len())
                && line.bytes().all(|b| b == b'.' || b.is_ascii_alphabetic())
                && rows.len() < SPRITE
            {
                rows.push(line);
            }
        }
        if rows.len() < 6 || keys.is_empty() {
            return Err("a IA não mandou um desenho válido. Tente de novo ou descreva de outro jeito.".into());
        }
        let fallback = keys.iter().position(|&k| k == b'k').map_or(1, |i| i as u8 + 1);
        let top = SPRITE - rows.len(); // desenho curto: encosta embaixo
        for (r, row) in rows.iter().enumerate() {
            for (x, b) in row.bytes().take(SPRITE).enumerate() {
                if b != b'.' {
                    let index = keys.iter().position(|&k| k == b).map_or(fallback, |i| i as u8 + 1);
                    d.px[(top + r) * SPRITE + x] = index;
                }
            }
        }
        d.eyes = keys.iter().position(|&k| k == b'e').map(|i| i as u8 + 1);
        if d.name.is_empty() {
            d.name = "Mascote da IA".into();
        }
        Ok(d)
    }

    pub fn is_empty(&self) -> bool {
        self.px.iter().all(|&p| p == 0)
    }

    pub fn mirror(&mut self, pose: Pose) {
        for row in self.grid_mut(pose).chunks_mut(SPRITE) {
            row.reverse();
        }
    }

    /// Nome da pasta: minúsculas, sem acento, só letras/números/hífen.
    pub fn id(&self) -> String {
        let mut id = String::new();
        for c in self.name.to_lowercase().chars() {
            let c = match c {
                'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
                'é' | 'è' | 'ê' | 'ë' => 'e',
                'í' | 'ì' | 'î' | 'ï' => 'i',
                'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
                'ú' | 'ù' | 'û' | 'ü' => 'u',
                'ç' => 'c',
                c if c.is_ascii_alphanumeric() => c,
                _ => '-',
            };
            if c != '-' || !id.ends_with('-') {
                id.push(c);
            }
        }
        let id = id.trim_matches('-').to_string();
        if id.is_empty() {
            "meu-mascote".into()
        } else {
            id
        }
    }

    /// Gera o mascot.txt completo, com todas as poses.
    pub fn to_mascot_txt(&self) -> String {
        let base = settle(&self.px);
        let eyes = eye_boxes(&base, self.eyes, &self.palette);
        let closed = close_eyes(&base, &eyes, false);
        let blink = close_eyes(&base, &eyes, true);

        // Poses desenhadas à mão substituem as geradas.
        let asleep = self.drawn(Pose::Sleep).unwrap_or(closed);
        let happy = self.drawn(Pose::Happy).unwrap_or(base);
        let walking = self.drawn(Pose::Walk).unwrap_or_else(|| shift(&base, -1));

        let mut sleep1 = shift(&asleep, 1);
        overlay(&mut sleep1, &Z_PATTERN, 12, 0, Z);
        let mut sleep2 = shift(&asleep, 1);
        overlay(&mut sleep2, &Z_PATTERN, 11, 1, Z);
        let (top, _, left, right) = bounds(&happy);
        let mut happy1 = happy;
        overlay(&mut happy1, &HEART_PATTERN, ((left + right) / 2).saturating_sub(2), top.saturating_sub(4), HEART);

        let mut frames: Vec<(&str, Grid)> = vec![
            ("idle", base),
            ("blink", blink),
            ("walk1", base),
            ("walk2", walking),
            ("yawn", closed),
            ("sleep1", sleep1),
            ("sleep2", sleep2),
            ("held1", base),
            ("held2", shift(&base, -1)),
            ("fall", base),
            ("land", squash(&closed)),
            ("happy1", happy1),
            ("happy2", shift(&happy, -2)),
        ];
        if let Some(eating) = self.drawn(Pose::Eat) {
            // Mastigando: a pose sobe e desce.
            frames.push(("eat1", eating));
            frames.push(("eat2", shift(&eating, 1)));
        }

        // Letra de cada cor: olhos = 'e'; 'z' e 'h' ficam para os enfeites.
        let letters: Vec<u8> = b"kabcdfgijlmnopqrstuvwxy".to_vec();
        let mut key = vec![b'.'; 256];
        let mut next = letters.iter().filter(|&&l| l != b'e');
        for (i, k) in key.iter_mut().enumerate().take(self.palette.len()).skip(1) {
            *k = if Some(i as u8) == self.eyes { b'e' } else { *next.next().unwrap_or(&b'k') };
        }
        key[Z as usize] = b'z';
        key[HEART as usize] = b'h';

        let mut out = String::from("# Mascote criado no !StayAlone (Configurações → Criar mascote)\n");
        out += &format!("name {}\n", one_line(&self.name, "Meu mascote"));
        out += &format!("article {}\n", if self.female { "a" } else { "o" });
        out += &format!("about {}\n\n", one_line(&self.about, "um mascote fofinho feito à mão"));
        for (i, rgb) in self.palette.iter().enumerate().skip(1) {
            out += &format!("color {} {:06x}\n", key[i] as char, rgb & 0xFF_FFFF);
        }
        out += "color z 9fc5ff\ncolor h ef476f\n";
        for (name, grid) in frames {
            out += &format!("\nframe {name}\n");
            for row in grid.chunks(SPRITE) {
                out.extend(row.iter().map(|&p| key[p as usize] as char));
                out.push('\n');
            }
        }
        out
    }
}

/// Uma grade de `size`×`size` reduzida para 16×16 (em cada bloco, a cor mais comum).
fn shrink(chars: &[u8], size: usize) -> Vec<u8> {
    if size == SPRITE {
        return chars.to_vec();
    }
    let k = size / SPRITE;
    (0..PIXELS)
        .map(|i| {
            let (x, y) = (i % SPRITE * k, i / SPRITE * k);
            let mut count = [0u16; 128];
            for dy in 0..k {
                for dx in 0..k {
                    let c = chars[(y + dy) * size + x + dx];
                    count[c as usize & 127] += 1;
                }
            }
            // Transparente só se o bloco for quase todo transparente.
            let best = (1..128).max_by_key(|&c| count[c]).filter(|&c| count[c] * 4 >= (k * k) as u16).unwrap_or(0);
            best as u8
        })
        .collect()
}

fn one_line(s: &str, fallback: &str) -> String {
    let s = clean_line(s, 200);
    if s.is_empty() {
        fallback.into()
    } else {
        s
    }
}

/// (primeira linha, última linha, primeira coluna, última coluna) com desenho.
fn bounds(g: &Grid) -> (usize, usize, usize, usize) {
    let used = |x: usize, y: usize| g[y * SPRITE + x] != 0;
    let rows: Vec<usize> = (0..SPRITE).filter(|&y| (0..SPRITE).any(|x| used(x, y))).collect();
    let cols: Vec<usize> = (0..SPRITE).filter(|&x| (0..SPRITE).any(|y| used(x, y))).collect();
    match (rows.first(), rows.last(), cols.first(), cols.last()) {
        (Some(&t), Some(&b), Some(&l), Some(&r)) => (t, b, l, r),
        _ => (0, SPRITE - 1, 0, SPRITE - 1),
    }
}

/// Encosta o desenho na última linha (os pés no chão).
fn settle(g: &Grid) -> Grid {
    let (_, bottom, _, _) = bounds(g);
    shift(g, (SPRITE - 1 - bottom) as i32)
}

/// Move o desenho `dy` linhas (positivo = para baixo); o que sai some.
fn shift(g: &Grid, dy: i32) -> Grid {
    let mut out = [0u8; PIXELS];
    for y in 0..SPRITE as i32 {
        let src = y - dy;
        if (0..SPRITE as i32).contains(&src) {
            let (d, s) = (y as usize * SPRITE, src as usize * SPRITE);
            out[d..d + SPRITE].copy_from_slice(&g[s..s + SPRITE]);
        }
    }
    out
}

/// Achata: tira duas linhas do meio do corpo (a "aterrissagem").
fn squash(g: &Grid) -> Grid {
    let (top, bottom, _, _) = bounds(g);
    if bottom - top < 6 {
        return *g;
    }
    let cut = top + (bottom - top) * 55 / 100;
    let mut rows: Vec<&[u8]> = g.chunks(SPRITE).collect();
    rows.drain(cut..cut + 2);
    let mut out = [0u8; PIXELS];
    for (i, row) in rows.iter().enumerate() {
        let y = i + 2;
        out[y * SPRITE..(y + 1) * SPRITE].copy_from_slice(row);
    }
    out
}

fn overlay(g: &mut Grid, pattern: &[&str], x0: usize, y0: usize, value: u8) {
    for (dy, line) in pattern.iter().enumerate() {
        for (dx, c) in line.bytes().enumerate() {
            let (x, y) = (x0 + dx, y0 + dy);
            if c != b'.' && x < SPRITE && y < SPRITE && g[y * SPRITE + x] == 0 {
                g[y * SPRITE + x] = value;
            }
        }
    }
}

fn luminance(rgb: u32) -> u32 {
    let (r, g, b) = ((rgb >> 16) & 255, (rgb >> 8) & 255, rgb & 255);
    r * 3 + g * 6 + b
}

/// Caixas (x0, y0, x1, y1) de cada olho. Usa a cor "Olhos" se foi usada; senão,
/// deduz: pixels da cor mais escura cercados de desenho, na parte de cima.
fn eye_boxes(g: &Grid, eyes: Option<u8>, palette: &[u32]) -> Vec<(usize, usize, usize, usize)> {
    let dedicated = eyes.filter(|e| g.contains(e));
    let darkest = (1..palette.len())
        .filter(|&i| g.contains(&(i as u8)))
        .min_by_key(|&i| luminance(palette[i]))
        .unwrap_or(1) as u8;
    let is_eye = |i: usize| -> bool {
        let (x, y) = (i % SPRITE, i / SPRITE);
        match dedicated {
            Some(e) => g[i] == e,
            None => {
                g[i] == darkest
                    && y <= 11
                    && x > 0
                    && y > 0
                    && x < SPRITE - 1
                    && y < SPRITE - 1
                    && [i - 1, i + 1, i - SPRITE, i + SPRITE].iter().all(|&n| g[n] != 0)
            }
        }
    };
    let mut seen = [false; PIXELS];
    let mut boxes = Vec::new();
    for start in 0..PIXELS {
        if seen[start] || !is_eye(start) {
            continue;
        }
        let (mut stack, mut size) = (vec![start], 0);
        let mut b = (SPRITE, SPRITE, 0, 0);
        seen[start] = true;
        while let Some(i) = stack.pop() {
            size += 1;
            let (x, y) = (i % SPRITE, i / SPRITE);
            b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
            let neighbors = [
                (x > 0).then(|| i - 1),
                (x + 1 < SPRITE).then(|| i + 1),
                (y > 0).then(|| i - SPRITE),
                (y + 1 < SPRITE).then(|| i + SPRITE),
            ];
            for n in neighbors.into_iter().flatten() {
                if !seen[n] && is_eye(n) {
                    seen[n] = true;
                    stack.push(n);
                }
            }
        }
        // Manchas grandes não são olhos (só vale para a dedução).
        if dedicated.is_some() || size <= 6 {
            boxes.push(b);
        }
    }
    boxes
}

/// Fecha os olhos: a parte de cima vira "pele" e fica só a linha de baixo.
/// `blink`: olhos de uma linha só somem por um instante.
fn close_eyes(g: &Grid, boxes: &[(usize, usize, usize, usize)], blink: bool) -> Grid {
    let mut out = *g;
    for &(x0, y0, x1, y1) in boxes {
        let skin = skin_around(g, (x0, y0, x1, y1));
        let eye = g[y1 * SPRITE + x0].max(g[y1 * SPRITE + x1]);
        if y1 == y0 {
            if blink {
                (x0..=x1).for_each(|x| out[y0 * SPRITE + x] = skin);
            }
            continue;
        }
        for y in y0..y1 {
            for x in x0..=x1 {
                if g[y * SPRITE + x] != 0 {
                    out[y * SPRITE + x] = skin;
                }
            }
        }
        for x in x0..=x1 {
            if g[y1 * SPRITE + x] != 0 {
                out[y1 * SPRITE + x] = eye;
            }
        }
    }
    out
}

/// Cor mais comum em volta de uma caixa (a "pele" em volta do olho).
fn skin_around(g: &Grid, (x0, y0, x1, y1): (usize, usize, usize, usize)) -> u8 {
    let mut count = [0u32; 256];
    for y in y0.saturating_sub(1)..=(y1 + 1).min(SPRITE - 1) {
        for x in x0.saturating_sub(1)..=(x1 + 1).min(SPRITE - 1) {
            let inside = (x0..=x1).contains(&x) && (y0..=y1).contains(&y);
            let p = g[y * SPRITE + x];
            if !inside && p != 0 {
                count[p as usize] += 1;
            }
        }
    }
    (1..256).max_by_key(|&i| count[i]).filter(|&i| count[i] > 0).unwrap_or(1) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprite::{Art, Frame};

    /// Um blob simples com olhos 2×2 na cor dedicada.
    fn blob() -> Drawing {
        let mut d = Drawing::blank();
        d.name = "Bolinha".into();
        for y in 6..14 {
            for x in 3..13 {
                d.px[y * SPRITE + x] = if y == 6 || y == 13 || x == 3 || x == 12 { 1 } else { 11 };
            }
        }
        for (x, y) in [(5, 8), (6, 8), (5, 9), (6, 9), (9, 8), (10, 8), (9, 9), (10, 9)] {
            d.px[y * SPRITE + x] = BLANK_EYES;
        }
        d
    }

    fn frame(art: &Art, f: Frame) -> Vec<u32> {
        let mut out = vec![0; PIXELS];
        art.draw(f, false, 1, &mut out);
        out
    }

    #[test]
    fn drawing_becomes_a_full_mascot_that_blinks_and_stands_on_the_floor() {
        let txt = blob().to_mascot_txt();
        let art = Art::parse(&txt).unwrap_or_else(|e| panic!("{e}\n{txt}"));
        assert_eq!(art.name, "Bolinha");
        assert_ne!(frame(&art, Frame::Idle), frame(&art, Frame::Blink));
        // Encostado no chão: a última linha tem desenho.
        assert!(frame(&art, Frame::Idle)[15 * SPRITE..].iter().any(|&c| c != 0));
    }

    #[test]
    fn existing_mascots_can_be_used_as_templates() {
        for (id, src, _) in crate::pack::EMBEDDED {
            let sheet = Sheet::parse(src).unwrap();
            let d = Drawing::from_sheet(&sheet, false).unwrap();
            let art = Art::parse(&d.to_mascot_txt()).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_ne!(frame(&art, Frame::Idle), frame(&art, Frame::Blink), "{id} não pisca");
        }
    }

    #[test]
    fn reads_what_the_ai_sends() {
        let reply = "```\nname Polvi\narticle a\nabout uma polva roxa curiosa\ncolor k 2b1e2f\ncolor p 9b5de5\ncolor e 111111\nframe idle\n\
            ................\n................\n................\n................\n......kkkk......\n.....kppppk.....\n\
            ....kppppppk....\n....kpeppepk....\n....kpeppepk....\n....kppppppk....\n.....kppppk.....\n....kpkppkpk....\n\
            ...kp.kp.kpk....\n...k..k..k.k....\n................\n................\n```";
        let d = Drawing::from_ai(reply).unwrap();
        assert_eq!((d.name.as_str(), d.female), ("Polvi", true));
        assert_eq!(d.eyes, Some(3));
        let art = Art::parse(&d.to_mascot_txt()).unwrap();
        assert!(art.female());
        assert_ne!(frame(&art, Frame::Idle), frame(&art, Frame::Blink));
    }

    #[test]
    fn rejects_replies_without_a_drawing() {
        assert!(Drawing::from_ai("Desculpe, não consigo desenhar.").is_err());
    }

    #[test]
    fn ids_are_folder_safe() {
        let mut d = Drawing::blank();
        d.name = "  Zé Pequeno!! ".into();
        assert_eq!(d.id(), "ze-pequeno");
        d.name = "???".into();
        assert_eq!(d.id(), "meu-mascote");
    }

    #[test]
    fn mirror_twice_is_identity() {
        let mut d = blob();
        d.px[6 * SPRITE + 3] = 5;
        let before = d.px;
        d.mirror(Pose::Idle);
        assert_ne!(d.px, before);
        d.mirror(Pose::Idle);
        assert_eq!(d.px, before);
    }

    #[test]
    fn drawn_poses_replace_the_generated_ones() {
        let mut d = blob();
        let auto = Art::parse(&d.to_mascot_txt()).unwrap();
        assert_eq!(auto.index(Frame::Eat1), auto.index(Frame::Idle)); // sem pose de comer: usa a parada
        let mut eating = d.px;
        eating[15 * SPRITE + 7] = 5; // uma migalha no chão
        *d.grid_mut(Pose::Eat) = eating;
        *d.grid_mut(Pose::Sleep) = d.px;
        let art = Art::parse(&d.to_mascot_txt()).unwrap();
        assert_ne!(art.index(Frame::Eat1), art.index(Frame::Idle));
        assert_ne!(frame(&art, Frame::Eat1), frame(&art, Frame::Idle));
        d.clear(Pose::Eat);
        assert!(d.grid(Pose::Eat).is_none());
    }

    #[test]
    fn big_templates_are_shrunk_to_the_editor() {
        let mut big = vec![0u8; 32 * 32];
        for y in 8..32 {
            for x in 8..24 {
                big[y * 32 + x] = b'k';
            }
        }
        let small = shrink(&big, 32);
        assert_eq!(small.len(), PIXELS);
        assert_eq!((small[4 * SPRITE + 4], small[0]), (b'k', 0));
    }
}
