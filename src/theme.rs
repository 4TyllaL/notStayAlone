//! Cores, fontes e ícones da interface (Configurações e painel da bandeja).
//! Um só lugar para o visual: tons creme e laranja, combinando com os sprites.

use std::{
    ptr::null_mut,
    sync::atomic::{AtomicBool, Ordering},
};

use windows_sys::Win32::{
    Foundation::ERROR_SUCCESS,
    Graphics::Gdi::*,
    System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
};

use crate::{
    config::Theme,
    win::{ui_font, w},
};

/// Tema escuro ligado (vale para todas as janelas desenhadas pelo app).
static DARK: AtomicBool = AtomicBool::new(false);

pub fn set_dark(dark: bool) {
    DARK.store(dark, Ordering::Relaxed);
}

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// Escuro? `Auto` segue o Windows ("Modo dos aplicativos" nas Configurações).
pub fn resolve(theme: Theme) -> bool {
    match theme {
        Theme::Light => false,
        Theme::Dark => true,
        Theme::Auto => windows_apps_dark(),
    }
}

fn windows_apps_dark() -> bool {
    let mut value = 1u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize").as_ptr(),
            w("AppsUseLightTheme").as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    status == ERROR_SUCCESS && value == 0
}

/// Cada cor em 0xRRGGBB (claro, escuro) — convertida com `colorref` para o GDI
/// ou `argb` para os canvas. Os tons escuros mantêm o calor do creme e do laranja.
macro_rules! colors {
    ($($(#[$doc:meta])* $name:ident: $light:expr, $dark:expr;)*) => {
        $(
            $(#[$doc])*
            pub fn $name() -> u32 {
                if is_dark() { $dark } else { $light }
            }
        )*
    };
}

colors! {
    /// Fundo da janela (papel creme) e da barra lateral, um tom abaixo.
    bg: 0xF8F4EF, 0x1B181B;
    sidebar: 0xF1EBE3, 0x221E22;
    /// Superfície dos cartões e dos campos.
    card: 0xFFFFFF, 0x2A252A;
    /// Contorno fino dos cartões.
    border: 0xEAE2D8, 0x3A333A;
    /// Linha entre as linhas de um cartão (mais leve que a borda).
    divider: 0xF1EBE4, 0x332D33;
    /// Contorno dos campos e seletores (mais firme: mostra onde clicar).
    field_border: 0xD9CEC2, 0x4A424A;
    text: 0x2B1E2F, 0xF3ECE6;
    muted: 0x6B6470, 0xA89FA9;
    /// Laranja queimado: no claro, escuro o bastante para texto branco (4,5:1).
    accent: 0xC2560F, 0xE8823A;
    /// Laranja apertado / com o mouse em cima.
    accent_dark: 0xA4470B, 0xF29752;
    accent_soft: 0xFBE9DB, 0x45301F;
    /// Fundo de botões/blocos neutros.
    soft: 0xF4EFE9, 0x353036;
    /// Realce ao passar o mouse.
    hover: 0xEBE4DB, 0x403940;
    heart: 0xE8466C, 0xFF5C84;
    danger: 0xB93A2C, 0xFF7A6B;
    disabled: 0x9A918A, 0x7A727B;
    /// Fundo das ações perigosas com o mouse em cima.
    danger_soft: 0xFBE5E1, 0x4A2A2A;
    /// Trilho do interruptor desligado.
    switch_off: 0xC9C0B6, 0x5A525B;
    /// Anel de foco do teclado.
    focus_ring: 0xF2B98F, 0x9A5A2A;
    /// Texto e ícones sobre o laranja (escuro no tema escuro: o laranja é claro).
    on_accent: 0xFFFFFF, 0x26150A;
    /// Bolinha dos interruptores.
    knob: 0xFFFFFF, 0xF3ECE6;
}

/// Escala de tipos (px a 96 DPI) — os mesmos tamanhos em todas as janelas.
pub mod font {
    /// Título da página.
    pub const DISPLAY: i32 = 22;
    /// Título de cartão / nome do mascote.
    pub const TITLE: i32 = 16;
    /// Texto, rótulos e botões.
    pub const BODY: i32 = 14;
    /// Descrições, legendas e notas.
    pub const SMALL: i32 = 12;
    /// Ícones da fonte de símbolos.
    pub const ICON: i32 = 16;
}

/// Medidas compartilhadas (px a 96 DPI): grade de 4/8, raios e alturas de controles.
pub mod size {
    /// Raio dos cartões e do painel.
    pub const CARD_RADIUS: i32 = 12;
    /// Raio de botões, campos e itens da navegação (concêntrico com o cartão).
    pub const RADIUS: i32 = 8;
    /// Campos, seletores e botões dentro de uma linha.
    pub const CONTROL: i32 = 30;
    /// Botões de ação (rodapé de cartão, cabeçalho).
    pub const BUTTON: i32 = 32;
    /// Interruptor.
    pub const SWITCH_W: i32 = 40;
    pub const SWITCH_H: i32 = 20;
}

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
    pub const SHOP: char = '\u{E719}';
    /// "Diagnóstico" (linha de batimento): uso do PC.
    pub const GAUGE: char = '\u{E9D9}';
    pub const SETTINGS: char = '\u{E713}';
    pub const FOOD: char = '\u{EC32}';
    pub const GAME: char = '\u{E7FC}';
    pub const TIMER: char = '\u{E916}';
    pub const MUTE: char = '\u{E74F}';
    pub const HIDE: char = '\u{ED1A}';
    pub const WATER: char = '\u{EB42}';
    pub const INFO: char = '\u{E946}';
    pub const CALENDAR: char = '\u{E787}';
    pub const POWER: char = '\u{E7E8}';
    pub const CHEVRON: char = '\u{E76C}';
    pub const SEND: char = '\u{E724}';
    pub const DOWNLOAD: char = '\u{E896}';
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
