//! Editor de pixels da aba "Criar mascote": a grade 16×16 e a paleta de cores.
//! Controles desenhados à mão com GDI; o desenho vive no `State` da janela.

use std::{
    mem::{size_of, zeroed},
    ptr::null,
};

use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{
        Controls::Dialogs::{ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW},
        Input::KeyboardAndMouse::{ReleaseCapture, SetCapture},
        WindowsAndMessaging::*,
    },
};

use super::{colorref, item, rgb, scale, state, State, IDC_CANVAS, IDC_PALETTE};
use crate::{
    maker::{Pose, MAX_COLORS},
    sprite::{PIXELS, SPRITE},
};

/// Tamanho de um pixel do editor e de uma cor da paleta (em pontos de 96 DPI).
pub(super) const CELL: i32 = 16;
pub(super) const SWATCH: i32 = 24;
pub(super) const SWATCH_GAP: i32 = 4;

pub(super) unsafe fn redraw_editor(hwnd: HWND) {
    InvalidateRect(item(hwnd, IDC_CANVAS), null(), 0);
    InvalidateRect(item(hwnd, IDC_PALETTE), null(), 0);
}

/// Pinta um retângulo com o pincel do próprio DC (não cria objetos GDI).
pub(super) unsafe fn fill(dc: HDC, rect: RECT, color: COLORREF) {
    SetDCBrushColor(dc, color);
    FillRect(dc, &rect, GetStockObject(DC_BRUSH) as HBRUSH);
}

/// Estado da janela de configurações a partir de um controle filho.
unsafe fn parent_state<'a>(child: HWND) -> Option<(&'a mut State, HWND)> {
    let parent = GetParent(child);
    state(parent).map(|s| (s, parent))
}

fn mouse(lp: LPARAM) -> (i32, i32) {
    ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32)
}

/// `a` com `amount` de opacidade sobre `b` (cores 0xRRGGBB).
fn mix(a: u32, b: u32, amount: f32) -> u32 {
    let channel = |shift: u32| {
        let (x, y) = (((a >> shift) & 0xFF) as f32, ((b >> shift) & 0xFF) as f32);
        ((x * amount + y * (1.0 - amount)).round() as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}

pub(super) unsafe extern "system" fn canvas_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some((st, _)) = parent_state(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
    let cell = scale(CELL, st.dpi);
    let paint_at = |st: &mut State, lp: LPARAM| {
        let (x, y) = mouse(lp);
        let (cx, cy) = (x / cell, y / cell);
        let (Some(value), true) = (st.painting, (0..SPRITE as i32).contains(&cx) && (0..SPRITE as i32).contains(&cy)) else {
            return;
        };
        let i = cy as usize * SPRITE + cx as usize;
        let grid = st.drawing.grid_mut(st.pose);
        if grid[i] != value {
            grid[i] = value;
            let r = RECT { left: cx * cell, top: cy * cell, right: (cx + 1) * cell, bottom: (cy + 1) * cell };
            InvalidateRect(hwnd, &r, 0);
        }
    };
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let empty = [0u8; PIXELS];
            let grid = st.drawing.grid(st.pose).unwrap_or(&empty);
            // Numa pose extra, a pose parada aparece clarinha por baixo (papel vegetal).
            let guide = (st.pose != Pose::Idle).then_some(&st.drawing.px);
            for i in 0..PIXELS {
                let (x, y) = ((i % SPRITE) as i32, (i / SPRITE) as i32);
                let r = RECT { left: x * cell, top: y * cell, right: (x + 1) * cell, bottom: (y + 1) * cell };
                let p = grid[i] as usize;
                let checker = if (x + y) % 2 == 0 { 0xf4f4f4 } else { 0xe7e7e7 };
                let color = match (st.drawing.palette.get(p), guide.map(|g| g[i] as usize)) {
                    (Some(&c), _) if p != 0 => colorref(c),
                    (_, Some(g)) if g != 0 => colorref(mix(st.drawing.palette.get(g).copied().unwrap_or(0), checker, 0.25)),
                    // Transparente: xadrez claro.
                    _ => colorref(checker),
                };
                fill(dc, r, color);
                // Linhas finas da grade.
                fill(dc, RECT { left: r.right - 1, ..r }, rgb(0xdc, 0xdc, 0xdc));
                fill(dc, RECT { top: r.bottom - 1, ..r }, rgb(0xdc, 0xdc, 0xdc));
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            st.painting = Some(if msg == WM_LBUTTONDOWN { st.color } else { 0 });
            SetCapture(hwnd);
            paint_at(st, lp);
            0
        }
        WM_MOUSEMOVE => {
            paint_at(st, lp);
            0
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            st.painting = None;
            ReleaseCapture();
            0
        }
        WM_CAPTURECHANGED => {
            st.painting = None;
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Retângulo do espaço `slot` da paleta (0 = borracha).
fn swatch_rect(slot: usize, dpi: u32) -> RECT {
    let (col, row) = ((slot % 8) as i32, (slot / 8) as i32);
    let step = scale(SWATCH + SWATCH_GAP, dpi);
    let size = scale(SWATCH, dpi);
    RECT { left: col * step, top: row * step, right: col * step + size, bottom: row * step + size }
}

pub(super) unsafe extern "system" fn palette_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some((st, parent)) = parent_state(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
    let slots = st.drawing.palette.len().min(MAX_COLORS + 1);
    let dpi = st.dpi;
    let hit = |lp: LPARAM| {
        let (x, y) = mouse(lp);
        (0..slots).find(|&s| {
            let r = swatch_rect(s, dpi);
            x >= r.left && x < r.right && y >= r.top && y < r.bottom
        })
    };
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut client: RECT = zeroed();
            GetClientRect(hwnd, &mut client);
            fill(dc, client, colorref(crate::theme::card()));
            let one = scale(1, st.dpi).max(1);
            for slot in 0..slots {
                let r = swatch_rect(slot, st.dpi);
                let selected = slot as u8 == st.color;
                // Moldura (grossa na cor selecionada).
                let frame = if selected { rgb(0x2b, 0x1e, 0x2f) } else { rgb(0xc8, 0xc8, 0xc8) };
                fill(dc, r, frame);
                let pad = if selected { 3 * one } else { one };
                let inner = RECT { left: r.left + pad, top: r.top + pad, right: r.right - pad, bottom: r.bottom - pad };
                if slot == 0 {
                    // Borracha: xadrez com um risco vermelho.
                    fill(dc, inner, rgb(0xf4, 0xf4, 0xf4));
                    let half = (inner.right - inner.left) / 2;
                    fill(dc, RECT { right: inner.left + half, bottom: inner.top + half, ..inner }, rgb(0xdd, 0xdd, 0xdd));
                    fill(dc, RECT { left: inner.left + half, top: inner.top + half, ..inner }, rgb(0xdd, 0xdd, 0xdd));
                    for k in 0..(inner.right - inner.left) {
                        let p = RECT { left: inner.left + k, top: inner.bottom - 1 - k, right: inner.left + k + one, bottom: inner.bottom - k + one - 1 };
                        fill(dc, p, rgb(0xe0, 0x3a, 0x3a));
                    }
                } else {
                    fill(dc, inner, colorref(st.drawing.palette[slot]));
                    if Some(slot as u8) == st.drawing.eyes {
                        // Ponto branco: esta é a cor dos olhos.
                        let (cx, cy) = ((inner.left + inner.right) / 2, (inner.top + inner.bottom) / 2);
                        let d = 2 * one;
                        fill(dc, RECT { left: cx - d, top: cy - d, right: cx + d, bottom: cy + d }, rgb(0xff, 0xff, 0xff));
                    }
                }
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_LBUTTONDOWN => {
            if let Some(slot) = hit(lp) {
                st.color = slot as u8;
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_LBUTTONDBLCLK => {
            let Some(slot) = hit(lp).filter(|&s| s > 0) else { return 0 };
            let mut custom = [0u32; 16];
            let mut cc: CHOOSECOLORW = zeroed();
            cc.lStructSize = size_of::<CHOOSECOLORW>() as u32;
            cc.hwndOwner = parent;
            cc.rgbResult = colorref(st.drawing.palette[slot]);
            cc.lpCustColors = custom.as_mut_ptr();
            cc.Flags = CC_RGBINIT | CC_FULLOPEN;
            if ChooseColorW(&mut cc) != 0 {
                let c = cc.rgbResult;
                // Reobtém o estado: o diálogo roda um laço de mensagens próprio.
                if let Some((st, parent)) = parent_state(hwnd) {
                    st.drawing.palette[slot] = (c & 0xFF) << 16 | (c & 0xFF00) | (c >> 16) & 0xFF;
                    st.color = slot as u8;
                    redraw_editor(parent);
                }
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
