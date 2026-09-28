//! Cores, fontes e ícones da interface (Configurações e painel da bandeja).
//! Um só lugar para o visual: tons creme e laranja, combinando com os sprites.

use std::ptr::null_mut;

use windows_sys::Win32::Graphics::Gdi::*;

use crate::win::{ui_font, w};

/// 0xRRGGBB — convertidas com `colorref` para o GDI ou `argb` para os canvas.
pub const BG: u32 = 0xF6F2EC;
pub const SIDEBAR: u32 = 0xFBEEDF;
pub const CARD: u32 = 0xFFFFFF;
pub const BORDER: u32 = 0xE7DED3;
pub const TEXT: u32 = 0x2B1E2F;
pub const MUTED: u32 = 0x6F6874;
pub const ACCENT: u32 = 0xD96C1F;
pub const ACCENT_DARK: u32 = 0xB85714;
pub const ACCENT_SOFT: u32 = 0xFCE7D5;
/// Fundo de botões/blocos neutros e realce ao passar o mouse.
pub const SOFT: u32 = 0xF3EEE7;
pub const HOVER: u32 = 0xEAE3DA;
pub const HEART: u32 = 0xEF476F;
pub const DANGER: u32 = 0xC0392B;
pub const DISABLED: u32 = 0xB9B2AA;

/// 0xRRGGBB → COLORREF do GDI (0x00BBGGRR).
pub const fn colorref(c: u32) -> u32 {
    (c >> 16) & 0xFF | (c & 0xFF00) | (c & 0xFF) << 16
}

/// 0xRRGGBB → pixel opaco dos canvas (0xAARRGGBB).
pub const fn argb(c: u32) -> u32 {
    0xFF00_0000 | c
}

/// Ícones da fonte de símbolos do Windows (Segoe Fluent Icons / Segoe MDL2 Assets).
pub mod icon {
    pub const HOME: char = '\u{E80F}';
    pub const BELL: char = '\u{EA8F}';
    pub const CHAT: char = '\u{E8BD}';
    pub const PALETTE: char = '\u{E790}';
    pub const PUZZLE: char = '\u{EA86}';
    pub const SETTINGS: char = '\u{E713}';
    pub const FOOD: char = '\u{EC32}';
    pub const GAME: char = '\u{E7FC}';
    pub const TIMER: char = '\u{E916}';
    pub const MUTE: char = '\u{E74F}';
    pub const HIDE: char = '\u{ED1A}';
    pub const CALENDAR: char = '\u{E787}';
    pub const POWER: char = '\u{E7E8}';
    pub const CHEVRON: char = '\u{E76C}';
    pub const SEND: char = '\u{E724}';
    pub const ADD: char = '\u{E710}';
    pub const MORE: char = '\u{E712}';
    pub const HEART_FULL: char = '\u{EB52}';
    pub const HEART_EMPTY: char = '\u{EB51}';
}

/// Fonte de ícones: a do Windows 11 se existir, senão a do Windows 10.
pub unsafe fn icon_font(height: i32) -> HFONT {
    for face in ["Segoe Fluent Icons", "Segoe MDL2 Assets"] {
        let font = CreateFontW(
            -height,
            0,
            0,
            0,
            FW_NORMAL as _,
            0,
            0,
            0,
            DEFAULT_CHARSET as _,
            OUT_DEFAULT_PRECIS as _,
            CLIP_DEFAULT_PRECIS as _,
            ANTIALIASED_QUALITY as _,
            (DEFAULT_PITCH | FF_DONTCARE) as _,
            w(face).as_ptr(),
        );
        if has_face(font, face) {
            return font;
        }
        DeleteObject(font);
    }
    ui_font(height, FW_NORMAL)
}

/// O Windows trocou a fonte pedida por outra (ela não está instalada)?
unsafe fn has_face(font: HFONT, face: &str) -> bool {
    let dc = CreateCompatibleDC(null_mut());
    let old = SelectObject(dc, font);
    let mut name = [0u16; 64];
    let n = GetTextFaceW(dc, name.len() as i32, name.as_mut_ptr());
    SelectObject(dc, old);
    DeleteDC(dc);
    let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    n > 0 && String::from_utf16_lossy(&name[..len]).eq_ignore_ascii_case(face)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_conversions() {
        assert_eq!(colorref(0x112233), 0x332211);
        assert_eq!(argb(0x112233), 0xFF112233);
    }
}
