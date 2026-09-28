//! Peças de interface compartilhadas pelas janelas comuns (Configurações e a
//! primeira abertura): botões arredondados desenhados à mão.

use windows_sys::Win32::{
    Foundation::RECT,
    Graphics::Gdi::{DT_CENTER, DT_SINGLELINE, DT_VCENTER, HFONT},
    UI::Controls::{DRAWITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_NOFOCUSRECT, ODS_SELECTED},
};

use crate::{
    gfx::Canvas,
    theme::{self, argb},
    win::text_of,
};

/// Como um botão aparece.
pub struct ButtonStyle {
    /// A ação principal da área (laranja); os outros são brancos com borda.
    pub primary: bool,
    /// Cor atrás do botão (aparece nos cantos arredondados).
    pub background: u32,
    pub font: HFONT,
    pub bold: HFONT,
    pub dpi: u32,
}

/// Desenha um botão `BS_OWNERDRAW` (resposta ao `WM_DRAWITEM`).
pub unsafe fn draw_button(di: &DRAWITEMSTRUCT, style: &ButtonStyle) {
    let s = |v: i32| v * style.dpi as i32 / 96;
    let r = di.rcItem;
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let pressed = di.itemState & ODS_SELECTED != 0;
    let disabled = di.itemState & ODS_DISABLED != 0;
    let focused = di.itemState & ODS_FOCUS != 0 && di.itemState & ODS_NOFOCUSRECT == 0;
    let radius = s(7);
    let mut c = Canvas::new(w.max(1), h.max(1));
    c.fill(0, 0, w, h, argb(style.background));
    let ink = if disabled {
        c.card((0, 0, w, h), radius, argb(theme::SOFT), argb(theme::BORDER));
        theme::DISABLED
    } else if style.primary {
        c.round_rect(0, 0, w, h, radius, argb(if pressed { theme::ACCENT_DARK } else { theme::ACCENT }));
        if focused {
            // Anel claro por dentro: mostra o foco do teclado sem sair do laranja.
            c.card((s(2), s(2), w - s(4), h - s(4)), radius - 2, argb(theme::ACCENT_DARK), argb(0xF7C9A3));
            c.round_rect(s(3), s(3), w - s(6), h - s(6), radius - 3, argb(if pressed { theme::ACCENT_DARK } else { theme::ACCENT }));
        }
        theme::CARD
    } else {
        let border = if focused { theme::ACCENT } else { theme::BORDER };
        c.card((0, 0, w, h), radius, argb(if pressed { theme::HOVER } else { theme::CARD }), argb(border));
        theme::TEXT
    };
    let font = if style.primary { style.bold } else { style.font };
    c.text(font, &text_of(di.hwndItem), RECT { left: 0, top: 0, right: w, bottom: h }, ink, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    c.blit(di.hDC, r.left, r.top);
}
