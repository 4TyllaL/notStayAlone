//! Acessórios de época: um chapéu desenhado por cima do mascote em certas datas.
//! Tudo local (só a data do PC), sem arquivo novo: os chapéus são pixel art aqui.

/// Um chapéu, em pixels de um sprite 16×16 (num sprite 32×32 cada pixel vale 2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hat {
    /// Gorro de Natal (20 a 26 de dezembro).
    Santa,
    /// Chapéu de bruxa (25 a 31 de outubro).
    Witch,
    /// Chapéu de festa (seu aniversário e a virada do ano).
    Party,
    /// Chapéu de palha (festa junina, 15 a 30 de junho).
    Straw,
}

const SANTA: [&str; 5] = ["......ww", ".....rww", "...rrrr.", "..rrrrrr", ".wwwwwww"];
const WITCH: [&str; 5] = [".....k....", "....kpk...", "...kpppk..", "...kyyyk..", "kppppppppk"];
const PARTY: [&str; 5] = ["..yy..", "..pp..", ".bbbb.", ".pppp.", "bbbbbb"];
const STRAW: [&str; 5] = ["...kkkk...", "..kssssk..", "..krrrrk..", "kssssssssk", ".kkkkkkkk."];

fn color(key: u8) -> u32 {
    0xFF00_0000
        | match key {
            b'k' => 0x2b1e2f,
            b'r' => 0xd7263d,
            b'w' => 0xffffff,
            b'p' => 0x7b4fbf,
            b'y' => 0xffd23f,
            b'b' => 0x3fa7d6,
            _ => 0xe8c170, // palha
        }
}

impl Hat {
    /// O chapéu do dia (`month`, `day`), se houver.
    pub fn for_date(month: u32, day: u32, birthday: bool) -> Option<Hat> {
        match (month, day) {
            _ if birthday => Some(Hat::Party),
            (12, 31) | (1, 1) => Some(Hat::Party),
            (12, 20..=26) => Some(Hat::Santa),
            (10, 25..=31) => Some(Hat::Witch),
            (6, 15..=30) => Some(Hat::Straw),
            _ => None,
        }
    }

    fn rows(self) -> &'static [&'static str] {
        match self {
            Hat::Santa => &SANTA,
            Hat::Witch => &WITCH,
            Hat::Party => &PARTY,
            Hat::Straw => &STRAW,
        }
    }

    /// Desenha o chapéu em cima da cabeça, num frame já desenhado em `out`
    /// (`size`×`size` pixels de sprite, cada um com `scale`×`scale` na tela).
    pub fn draw(self, out: &mut [u32], size: usize, scale: usize, flip: bool) {
        let width = size * scale;
        let unit = (size / 16).max(1); // pixels de sprite por pixel do chapéu
        let opaque = |x: usize, y: usize| out[y * scale * width + x * scale] >> 24 != 0;
        // O topo da cabeça: primeira linha com as 4 colunas do meio preenchidas
        // (ignora orelhas, espinhos e laços, que ficam nas pontas).
        let mid = size / 2;
        let Some(head) = (0..size).find(|&y| (mid - 2 * unit..mid + 2 * unit).all(|x| opaque(x, y))) else { return };
        let rows = self.rows();
        let (w, h) = (rows[0].len(), rows.len());
        // A aba encosta no topo da cabeça, cobrindo o contorno.
        let bottom = (head / unit) as i32 + 1;
        let left = (16 - w as i32) / 2;
        for (ry, row) in rows.iter().enumerate() {
            let hy = bottom - (h - 1 - ry) as i32;
            if hy < 0 {
                continue;
            }
            for (rx, key) in row.bytes().enumerate() {
                if key == b'.' {
                    continue;
                }
                let hx = if flip { left + (w - 1 - rx) as i32 } else { left + rx as i32 };
                let c = color(key);
                // Cada pixel do chapéu vira `unit`×`unit` pixels de sprite, cada um `scale`×`scale`.
                let (x0, y0) = (hx as usize * unit * scale, hy as usize * unit * scale);
                let side = unit * scale;
                if x0 + side > width || y0 + side > width {
                    continue;
                }
                for y in y0..y0 + side {
                    out[y * width + x0..y * width + x0 + side].fill(c);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprite::{Art, Frame};

    #[test]
    fn hats_follow_the_calendar() {
        assert_eq!(Hat::for_date(12, 24, false), Some(Hat::Santa));
        assert_eq!(Hat::for_date(10, 31, false), Some(Hat::Witch));
        assert_eq!(Hat::for_date(6, 24, false), Some(Hat::Straw));
        assert_eq!(Hat::for_date(1, 1, false), Some(Hat::Party));
        assert_eq!(Hat::for_date(3, 10, true), Some(Hat::Party));
        assert_eq!(Hat::for_date(3, 10, false), None);
    }

    #[test]
    fn every_hat_row_has_the_same_width() {
        for hat in [Hat::Santa, Hat::Witch, Hat::Party, Hat::Straw] {
            let rows = hat.rows();
            assert!(rows.iter().all(|r| r.len() == rows[0].len() && r.len() % 2 == 0), "{hat:?}");
        }
    }

    /// Em todo mascote embutido e em qualquer pose, o chapéu cabe e aparece.
    #[test]
    fn hats_sit_on_every_mascot() {
        for (id, src, ..) in crate::pack::EMBEDDED {
            let art = Art::parse(src).unwrap();
            let size = art.size();
            for frame in [Frame::Idle, Frame::Walk1, Frame::Sleep1, Frame::Happy1, Frame::Held1] {
                for flip in [false, true] {
                    let scale = 3;
                    let mut out = vec![0; size * size * scale * scale];
                    art.draw(frame, flip, scale, &mut out);
                    let before = out.clone();
                    Hat::Witch.draw(&mut out, size, scale, flip);
                    assert_ne!(before, out, "{id} {frame:?}: o chapéu não apareceu");
                }
            }
        }
    }

    #[test]
    fn big_mascots_get_a_hat_twice_as_big() {
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/gallery/mascots/calcifer-hd/mascot.txt")).unwrap();
        let art = Art::parse(&src).unwrap();
        assert_eq!(art.size(), 32);
        let mut out = vec![0; 32 * 32];
        art.draw(Frame::Idle, false, 1, &mut out);
        let before = out.clone();
        Hat::Party.draw(&mut out, 32, 1, false);
        let changed = out.iter().zip(&before).filter(|(a, b)| a != b).count();
        assert!(changed >= 4 * 10, "só {changed} pixels mudaram");
    }
}
