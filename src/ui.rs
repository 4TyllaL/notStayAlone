//! Peças de interface compartilhadas pelas janelas comuns (Configurações e a
//! primeira abertura): botões e interruptores desenhados à mão e o tema escuro
//! dos controles nativos.

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, ClientToScreen, DrawTextW, FillRect, GetStockObject, SelectObject, SetBkMode, SetDCBrushColor, DC_BRUSH, DT_NOPREFIX, HBRUSH, HGDIOBJ, TRANSPARENT, EndPaint, ExcludeClipRect, GetWindowDC, InvalidateRect, RedrawWindow, ReleaseDC, RDW_FRAME, RDW_INVALIDATE, SetTextColor, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_SINGLELINE, DT_VCENTER, HDC,
        HFONT, PAINTSTRUCT,
    },
    UI::{
        Controls::{
            CDDS_ITEMPREPAINT, CDDS_PREPAINT, CDDS_POSTPAINT, CDRF_DODEFAULT, CDRF_NOTIFYITEMDRAW, CDRF_NOTIFYPOSTPAINT, CDRF_SKIPDEFAULT, HDITEMW, HDI_TEXT, HDM_GETITEMCOUNT, HDM_GETITEMRECT, HDM_GETITEMW, DRAWITEMSTRUCT, LVM_GETHEADER, LVM_SETBKCOLOR,
            LVM_SETTEXTBKCOLOR, LVM_SETTEXTCOLOR, NMCUSTOMDRAW, NMHDR, NM_CUSTOMDRAW, ODS_DISABLED, ODS_FOCUS, ODS_NOFOCUSRECT,
            ODS_SELECTED, WM_MOUSELEAVE,
        },
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{GetFocus, IsWindowEnabled, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT},
        Shell::{DefSubclassProc, SetWindowSubclass},
        WindowsAndMessaging::{
            GetClientRect, GetWindowLongW, GetWindowRect, SendMessageW, SetWindowPos, GWL_EXSTYLE,
            SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WM_NCCALCSIZE, WM_NCPAINT,
            WS_EX_CLIENTEDGE, GWL_STYLE, BS_OWNERDRAW, BS_TYPEMASK, CB_GETITEMHEIGHT, CB_SETITEMHEIGHT, CB_GETCURSEL, CB_GETDROPPEDSTATE, CB_GETLBTEXT, CB_GETLBTEXTLEN, WM_ENABLE, WM_ERASEBKGND,
            WM_GETFONT, WM_KILLFOCUS, WM_MOUSEMOVE, WM_NOTIFY, WM_PAINT, WM_SETFOCUS,
        },
    },
};

use std::ptr::null_mut;

use crate::{
    gfx::Canvas,
    theme::{self, argb},
    win::{self, text_of, w},
};

/// O papel de um botão — a mesma hierarquia em todas as janelas.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A ação principal da área: laranja cheio (uma por área).
    Primary,
    /// Ações comuns: fundo neutro, sem borda.
    Secondary,
    /// Ações leves (abrir pasta, links): só o texto em laranja.
    Ghost,
    /// Apagar/remover: fundo neutro com texto vermelho.
    Danger,
}

/// Como um botão aparece.
pub struct ButtonStyle {
    pub kind: Kind,
    /// Cor atrás do botão (aparece nos cantos arredondados).
    pub background: u32,
    pub font: HFONT,
    pub bold: HFONT,
    /// Fonte das descrições dos interruptores (sem ela, usa `font`).
    pub small: Option<HFONT>,
    pub dpi: u32,
}

/// O controle com o mouse em cima (um por vez): botões e seletores.
static HOT: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

fn is_hot(hwnd: HWND) -> bool {
    HOT.load(std::sync::atomic::Ordering::Relaxed) == hwnd as isize
}

/// Desenha um botão `BS_OWNERDRAW` (resposta ao `WM_DRAWITEM`).
pub unsafe fn draw_button(di: &DRAWITEMSTRUCT, style: &ButtonStyle) {
    let s = |v: i32| v * style.dpi as i32 / 96;
    let r = di.rcItem;
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let pressed = di.itemState & ODS_SELECTED != 0;
    let disabled = di.itemState & ODS_DISABLED != 0;
    let focused = di.itemState & ODS_FOCUS != 0 && di.itemState & ODS_NOFOCUSRECT == 0;
    let hot = is_hot(di.hwndItem) && !disabled;
    let radius = s(theme::size::RADIUS);
    let mut c = Canvas::new(w.max(1), h.max(1));
    c.fill(0, 0, w, h, argb(style.background));
    // (fundo, texto) de cada papel; `None` = sem fundo (só aparece com o mouse em cima).
    let (fill, ink) = match (style.kind, disabled) {
        (Kind::Ghost, true) => (None, theme::disabled()),
        (_, true) => (Some(theme::soft()), theme::disabled()),
        (Kind::Primary, _) => (Some(if pressed || hot { theme::accent_dark() } else { theme::accent() }), theme::on_accent()),
        (Kind::Secondary, _) => (Some(if pressed { theme::switch_off() } else if hot { theme::hover() } else { theme::soft() }), theme::text()),
        (Kind::Danger, _) => (Some(if pressed || hot { theme::danger_soft() } else { theme::soft() }), theme::danger()),
        (Kind::Ghost, _) => ((pressed || hot).then(theme::accent_soft), theme::accent()),
    };
    if focused {
        // Anel de foco de 2 px por dentro da borda (não sai do botão).
        let ring = if style.kind == Kind::Primary { theme::focus_ring() } else { theme::accent() };
        c.round_rect(0, 0, w, h, radius, argb(ring));
        let inner = fill.unwrap_or(style.background);
        c.round_rect(s(2), s(2), w - s(4), h - s(4), radius - s(2), argb(inner));
    } else if let Some(fill) = fill {
        c.round_rect(0, 0, w, h, radius, argb(fill));
    }
    c.text(style.bold, &text_of(di.hwndItem), RECT { left: s(8), top: 0, right: w - s(8), bottom: h }, ink, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
    c.blit(di.hDC, r.left, r.top);
}

/// Linha com interruptor: rótulo à esquerda (e, depois de uma quebra de linha, uma
/// descrição em cinza) e o interruptor à direita. Substitui as caixinhas de marcar, que o Windows
/// não deixa pintar direito no tema escuro. O estado fica com quem desenha.
pub unsafe fn draw_toggle(di: &DRAWITEMSTRUCT, on: bool, style: &ButtonStyle) {
    let s = |v: i32| v * style.dpi as i32 / 96;
    let r = di.rcItem;
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let disabled = di.itemState & ODS_DISABLED != 0;
    let focused = di.itemState & ODS_FOCUS != 0 && di.itemState & ODS_NOFOCUSRECT == 0;
    let hot = is_hot(di.hwndItem) && !disabled;
    let mut c = Canvas::new(w.max(1), h.max(1));
    c.fill(0, 0, w, h, argb(style.background));
    let (sw, sh) = (s(theme::size::SWITCH_W), s(theme::size::SWITCH_H));
    let (sx, sy) = (w - sw - s(2), (h - sh) / 2);
    if focused {
        c.round_rect(sx - s(3), sy - s(3), sw + s(6), sh + s(6), sh / 2 + s(3), argb(theme::accent()));
        c.round_rect(sx - s(1), sy - s(1), sw + s(2), sh + s(2), sh / 2 + s(1), argb(style.background));
    }
    let track = match (disabled, on) {
        (true, _) => theme::disabled(),
        (false, true) => if hot { theme::accent_dark() } else { theme::accent() },
        (false, false) => if hot { theme::muted() } else { theme::switch_off() },
    };
    c.round_rect(sx, sy, sw, sh, sh / 2, argb(track));
    let knob = sh - s(6);
    let kx = if on { sx + sw - knob - s(3) } else { sx + s(3) };
    c.round_rect(kx, sy + s(3), knob, knob, knob / 2, argb(theme::knob()));
    let text = text_of(di.hwndItem);
    let (label, about) = text.split_once('\n').map_or((text.as_str(), ""), |(l, a)| (l, a.trim()));
    let ink = if disabled { theme::disabled() } else { theme::text() };
    let right = sx - s(16);
    let flags = DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS;
    if about.is_empty() {
        c.text(style.font, label, RECT { left: 0, top: 0, right, bottom: h }, ink, flags | DT_VCENTER);
    } else {
        // Duas linhas centralizadas juntas: rótulo (20 px) e descrição (16 px).
        let top = (h - s(36)) / 2;
        c.text(style.font, label, RECT { left: 0, top, right, bottom: top + s(20) }, ink, flags | DT_VCENTER);
        let small = style.small.unwrap_or(style.font);
        c.text(small, about, RECT { left: 0, top: top + s(20), right, bottom: top + s(36) }, theme::muted(), flags | DT_VCENTER);
    }
    c.blit(di.hDC, r.left, r.top);
}

/// Deixa um controle nativo (campo, lista, seletor) no tema atual.
pub unsafe fn theme_control(control: HWND, class: &str) {
    if class == "BUTTON" && GetWindowLongW(control, GWL_STYLE) as u32 & BS_TYPEMASK as u32 == BS_OWNERDRAW as u32 {
        // Botões e interruptores desenhados à mão: realce ao passar o mouse.
        SetWindowSubclass(control, Some(button_proc), 4, 0);
    }
    if class == "COMBOBOX" {
        // A caixa fechada é desenhada à mão (igual aos botões); a lista aberta segue nativa.
        SetWindowSubclass(control, Some(combo_proc), 2, 0);
    }
    if class == "EDIT" && GetWindowLongW(control, GWL_EXSTYLE) as u32 & WS_EX_CLIENTEDGE != 0 {
        // Campo de uma linha: a borda 3D do Windows vira um cartão arredondado.
        SetWindowSubclass(control, Some(edit_proc), 3, 0);
        SetWindowPos(control, null_mut(), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED);
    }
    if class == "SysListView32" {
        let (bg, ink) = (theme::colorref(theme::card()), theme::colorref(theme::text()));
        SendMessageW(control, LVM_SETBKCOLOR, 0, bg as LPARAM);
        SendMessageW(control, LVM_SETTEXTBKCOLOR, 0, bg as LPARAM);
        SendMessageW(control, LVM_SETTEXTCOLOR, 0, ink as LPARAM);
        // Os títulos das colunas são pintados pela lista (`list_proc`), nos dois temas.
        if SendMessageW(control, LVM_GETHEADER, 0, 0) != 0 {
            SetWindowSubclass(control, Some(list_proc), 1, 0);
        }
    }
    if !theme::is_dark() {
        return;
    }
    let sub = match class {
        "EDIT" | "COMBOBOX" => "DarkMode_CFD",
        // Sem tema na lista: o tema escuro dela risca as colunas; a rolagem segue o app.
        "SysListView32" => "",
        _ => "DarkMode_Explorer",
    };
    if !sub.is_empty() {
        set_window_theme(control, sub);
    }
}

/// A lista recebe as notificações do próprio cabeçalho: pinta a faixa dos títulos
/// das colunas no estilo do app — fundo do cartão, texto cinza e uma linha embaixo.
unsafe extern "system" fn list_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if msg == WM_NOTIFY {
        let header = SendMessageW(hwnd, LVM_GETHEADER, 0, 0) as HWND;
        let hdr = &*(lp as *const NMHDR);
        if hdr.hwndFrom == header && hdr.code == NM_CUSTOMDRAW {
            let draw = &*(lp as *const NMCUSTOMDRAW);
            match draw.dwDrawStage {
                CDDS_PREPAINT => {
                    let mut all: RECT = std::mem::zeroed();
                    GetClientRect(header, &mut all);
                    fill_rect(draw.hdc, all, theme::card());
                    fill_rect(draw.hdc, RECT { top: all.bottom - 1, ..all }, theme::border());
                    return (CDRF_NOTIFYITEMDRAW | CDRF_NOTIFYPOSTPAINT) as LRESULT;
                }
                CDDS_POSTPAINT => {
                    // O que sobra à direita da última coluna (o tema escuro pinta de branco).
                    let mut all: RECT = std::mem::zeroed();
                    GetClientRect(header, &mut all);
                    let count = SendMessageW(header, HDM_GETITEMCOUNT, 0, 0).max(1) as usize;
                    let mut last: RECT = std::mem::zeroed();
                    SendMessageW(header, HDM_GETITEMRECT, count - 1, &mut last as *mut _ as LPARAM);
                    let rest = RECT { left: last.right, ..all };
                    fill_rect(draw.hdc, rest, theme::card());
                    fill_rect(draw.hdc, RECT { top: all.bottom - 1, ..rest }, theme::border());
                    return CDRF_DODEFAULT as LRESULT;
                }
                CDDS_ITEMPREPAINT => {
                    let r = draw.rc;
                    fill_rect(draw.hdc, RECT { bottom: r.bottom - 1, ..r }, theme::card());
                    let mut title = [0u16; 64];
                    let mut item: HDITEMW = std::mem::zeroed();
                    item.mask = HDI_TEXT;
                    item.pszText = title.as_mut_ptr();
                    item.cchTextMax = title.len() as i32;
                    SendMessageW(header, HDM_GETITEMW, draw.dwItemSpec, &mut item as *mut _ as LPARAM);
                    let len = title.iter().position(|&c| c == 0).unwrap_or(0);
                    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                    let mut text = RECT { left: r.left + 8 * dpi / 96, right: r.right - 4 * dpi / 96, bottom: r.bottom - 1, ..r };
                    let old = SelectObject(draw.hdc, SendMessageW(hwnd, WM_GETFONT, 0, 0) as HGDIOBJ);
                    SetBkMode(draw.hdc, TRANSPARENT as _);
                    SetTextColor(draw.hdc, theme::colorref(theme::muted()));
                    DrawTextW(draw.hdc, title.as_mut_ptr(), len as i32, &mut text, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);
                    SelectObject(draw.hdc, old);
                    return CDRF_SKIPDEFAULT as LRESULT;
                }
                _ => {}
            }
        }
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

unsafe fn fill_rect(dc: HDC, r: RECT, rgb: u32) {
    SetDCBrushColor(dc, theme::colorref(rgb));
    FillRect(dc, &r, GetStockObject(DC_BRUSH) as HBRUSH);
}

/// `SetWindowTheme` da uxtheme.dll, carregada só de System32.
unsafe fn set_window_theme(control: HWND, sub: &str) {
    type SetTheme = unsafe extern "system" fn(HWND, *const u16, *const u16) -> i32;
    let Some(proc) = win::system_proc("uxtheme.dll", c"SetWindowTheme") else { return };
    let set: SetTheme = std::mem::transmute::<unsafe extern "system" fn() -> isize, SetTheme>(proc);
    set(control, w(sub).as_ptr(), std::ptr::null());
}

/// Liga o modo escuro dos controles do Windows neste processo (listas suspensas,
/// barras de rolagem). É uma função sem nome da uxtheme.dll (ordinal 135); se
/// não existir nesta versão do Windows, nada muda.
pub unsafe fn allow_dark_mode(dark: bool) {
    type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
    if let Some(proc) = win::system_proc_ordinal("uxtheme.dll", 135) {
        let set: SetPreferredAppMode = std::mem::transmute::<unsafe extern "system" fn() -> isize, SetPreferredAppMode>(proc);
        set(if dark { 2 } else { 0 }); // 2 = sempre escuro, 0 = padrão
    }
}

/// Marca o controle com o mouse em cima e pede para redesenhar quando muda.
unsafe fn track_hot(hwnd: HWND, msg: u32) {
    use std::sync::atomic::Ordering::Relaxed;
    if msg == WM_MOUSELEAVE {
        // Sempre redesenha: o realce já pode ter ido para outro controle.
        let _ = HOT.compare_exchange(hwnd as isize, 0, Relaxed, Relaxed);
        InvalidateRect(hwnd, std::ptr::null(), 0);
    } else if HOT.swap(hwnd as isize, Relaxed) != hwnd as isize {
        let mut track = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
        TrackMouseEvent(&mut track);
        InvalidateRect(hwnd, std::ptr::null(), 0);
    }
}

unsafe extern "system" fn button_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if msg == WM_MOUSEMOVE || msg == WM_MOUSELEAVE {
        track_hot(hwnd, msg);
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

/// Altura do seletor fechado (a do Windows segue a fonte): ajusta o campo de seleção
/// até a caixa inteira ficar com `height` pixels.
pub unsafe fn set_combo_height(combo: HWND, height: i32) {
    let mut r: RECT = std::mem::zeroed();
    let mut field = SendMessageW(combo, CB_GETITEMHEIGHT, usize::MAX, 0) as i32;
    for _ in 0..3 {
        GetWindowRect(combo, &mut r);
        let diff = height - (r.bottom - r.top);
        if diff == 0 {
            break;
        }
        field = (field + diff).max(8);
        SendMessageW(combo, CB_SETITEMHEIGHT, usize::MAX, field as LPARAM);
    }
}

/// Seletor fechado no estilo do app: cartão arredondado, texto do item escolhido e
/// uma setinha; borda laranja com foco ou aberto.
unsafe extern "system" fn combo_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            paint_combo(hwnd, dc);
            EndPaint(hwnd, &ps);
            return 0;
        }
        WM_ERASEBKGND => return 1,
        WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE => {
            let r = DefSubclassProc(hwnd, msg, wp, lp);
            InvalidateRect(hwnd, std::ptr::null(), 0);
            return r;
        }
        WM_MOUSEMOVE | WM_MOUSELEAVE => track_hot(hwnd, msg),
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

unsafe fn paint_combo(hwnd: HWND, dc: HDC) {
    let mut r: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut r);
    let (w, h) = (r.right.max(1), r.bottom.max(1));
    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let s = |v: i32| v * dpi / 96;
    let enabled = IsWindowEnabled(hwnd) != 0;
    let open = SendMessageW(hwnd, CB_GETDROPPEDSTATE, 0, 0) != 0;
    let focused = GetFocus() == hwnd;
    let hot = is_hot(hwnd);
    let mut c = Canvas::new(w, h);
    c.fill(0, 0, w, h, argb(theme::card()));
    // Igual aos campos de texto: superfície do cartão e contorno firme; laranja com foco.
    let fill = if !enabled { theme::soft() } else if hot || open { theme::soft() } else { theme::card() };
    let border = if !enabled { theme::border() } else if focused || open { theme::accent() } else { theme::field_border() };
    c.card((0, 0, w, h), s(theme::size::RADIUS), argb(fill), argb(border));
    // Texto do item escolhido.
    let sel = SendMessageW(hwnd, CB_GETCURSEL, 0, 0);
    let mut label = String::new();
    if sel >= 0 {
        let len = SendMessageW(hwnd, CB_GETLBTEXTLEN, sel as WPARAM, 0).max(0) as usize;
        let mut buf = vec![0u16; len + 1];
        SendMessageW(hwnd, CB_GETLBTEXT, sel as WPARAM, buf.as_mut_ptr() as LPARAM);
        label = String::from_utf16_lossy(&buf[..len]);
    }
    let font = SendMessageW(hwnd, WM_GETFONT, 0, 0) as HFONT;
    let ink = if enabled { theme::text() } else { theme::disabled() };
    let text = RECT { left: s(12), top: 0, right: w - s(32), bottom: h };
    c.text(font, &label, text, ink, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
    // Setinha (um "v" de 2 px), virada para cima com a lista aberta.
    let (ax, ay, aw) = (w - s(22), h / 2, s(4));
    let stroke = s(2).max(2);
    let arrow = argb(if enabled { theme::muted() } else { theme::disabled() });
    for i in 0..=aw {
        let dy = if open { aw - i } else { i } - aw / 2;
        c.fill(ax + i, ay + dy - stroke / 2, stroke, stroke, arrow);
        c.fill(ax + 2 * aw - i, ay + dy - stroke / 2, stroke, stroke, arrow);
    }
    c.blit(dc, 0, 0);
}
/// Campo de texto no estilo do app: margem por dentro e moldura arredondada pintada
/// na área não cliente (o texto e o cursor continuam nativos).
unsafe extern "system" fn edit_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    match msg {
        WM_NCCALCSIZE => {
            let dpi = GetDpiForWindow(hwnd).max(96) as i32;
            let r = &mut *(lp as *mut RECT);
            let (h, v) = (10 * dpi / 96, ((r.bottom - r.top) - 20 * dpi / 96).max(2) / 2);
            r.left += h;
            r.right -= h;
            r.top += v;
            r.bottom -= v;
            return 0;
        }
        WM_NCPAINT => {
            paint_edit_frame(hwnd);
            return 0;
        }
        WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE => {
            let r = DefSubclassProc(hwnd, msg, wp, lp);
            RedrawWindow(hwnd, std::ptr::null(), null_mut(), RDW_FRAME | RDW_INVALIDATE);
            return r;
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

unsafe fn paint_edit_frame(hwnd: HWND) {
    let (mut win, mut client): (RECT, RECT) = (std::mem::zeroed(), std::mem::zeroed());
    GetWindowRect(hwnd, &mut win);
    GetClientRect(hwnd, &mut client);
    let mut origin = POINT { x: 0, y: 0 };
    ClientToScreen(hwnd, &mut origin);
    let (w, h) = ((win.right - win.left).max(1), (win.bottom - win.top).max(1));
    let (cx, cy) = (origin.x - win.left, origin.y - win.top);
    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let enabled = IsWindowEnabled(hwnd) != 0;
    let focused = GetFocus() == hwnd;
    // Sem realce ao passar o mouse: rastrear a saída do mouse num campo com moldura
    // própria gerava um laço de mensagens que deixava outros controles sem pintar.
    let border = if !enabled { theme::border() } else if focused { theme::accent() } else { theme::field_border() };
    let radius = theme::size::RADIUS * dpi / 96;
    let mut c = Canvas::new(w, h);
    c.fill(0, 0, w, h, argb(theme::card()));
    c.card((0, 0, w, h), radius, argb(theme::card()), argb(border));
    if focused {
        // Sublinhado laranja de 2 px, como nos campos do Windows 11.
        c.round_rect(radius, h - 2, w - 2 * radius, 2, 1, argb(theme::accent()));
    }
    let dc = GetWindowDC(hwnd);
    ExcludeClipRect(dc, cx, cy, cx + client.right, cy + client.bottom);
    c.blit(dc, 0, 0);
    ReleaseDC(hwnd, dc);
}