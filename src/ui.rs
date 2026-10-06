//! Peças de interface compartilhadas pelas janelas comuns (Configurações e a
//! primeira abertura): botões e interruptores desenhados à mão e o tema escuro
//! dos controles nativos.

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, EndPaint, InvalidateRect, SetTextColor, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_SINGLELINE, DT_VCENTER, HDC,
        HFONT, PAINTSTRUCT,
    },
    UI::{
        Controls::{
            CDDS_ITEMPREPAINT, CDDS_PREPAINT, CDRF_DODEFAULT, CDRF_NOTIFYITEMDRAW, DRAWITEMSTRUCT, LVM_GETHEADER, LVM_SETBKCOLOR,
            LVM_SETTEXTBKCOLOR, LVM_SETTEXTCOLOR, NMCUSTOMDRAW, NMHDR, NM_CUSTOMDRAW, ODS_DISABLED, ODS_FOCUS, ODS_NOFOCUSRECT,
            ODS_SELECTED, WM_MOUSELEAVE,
        },
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{GetFocus, IsWindowEnabled, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT},
        Shell::{DefSubclassProc, SetWindowSubclass},
        WindowsAndMessaging::{
            GetClientRect, SendMessageW, CB_GETCURSEL, CB_GETDROPPEDSTATE, CB_GETLBTEXT, CB_GETLBTEXTLEN, WM_ENABLE, WM_ERASEBKGND,
            WM_GETFONT, WM_KILLFOCUS, WM_MOUSEMOVE, WM_NOTIFY, WM_PAINT, WM_SETFOCUS,
        },
    },
};

use crate::{
    gfx::Canvas,
    theme::{self, argb},
    win::{self, text_of, w},
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
        c.card((0, 0, w, h), radius, argb(theme::soft()), argb(theme::border()));
        theme::disabled()
    } else if style.primary {
        c.round_rect(0, 0, w, h, radius, argb(if pressed { theme::accent_dark() } else { theme::accent() }));
        if focused {
            // Anel claro por dentro: mostra o foco do teclado sem sair do laranja.
            c.card((s(2), s(2), w - s(4), h - s(4)), radius - 2, argb(theme::accent_dark()), argb(theme::focus_ring()));
            c.round_rect(s(3), s(3), w - s(6), h - s(6), radius - 3, argb(if pressed { theme::accent_dark() } else { theme::accent() }));
        }
        theme::on_accent()
    } else {
        let border = if focused { theme::accent() } else { theme::border() };
        c.card((0, 0, w, h), radius, argb(if pressed { theme::hover() } else { theme::card() }), argb(border));
        theme::text()
    };
    let font = if style.primary { style.bold } else { style.font };
    c.text(font, &text_of(di.hwndItem), RECT { left: 0, top: 0, right: w, bottom: h }, ink, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    c.blit(di.hDC, r.left, r.top);
}

/// Interruptor com rótulo (substitui as caixinhas de marcar, que o Windows não
/// deixa pintar direito no tema escuro). O estado fica com quem desenha.
pub unsafe fn draw_toggle(di: &DRAWITEMSTRUCT, on: bool, style: &ButtonStyle) {
    let s = |v: i32| v * style.dpi as i32 / 96;
    let r = di.rcItem;
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let disabled = di.itemState & ODS_DISABLED != 0;
    let focused = di.itemState & ODS_FOCUS != 0 && di.itemState & ODS_NOFOCUSRECT == 0;
    let mut c = Canvas::new(w.max(1), h.max(1));
    c.fill(0, 0, w, h, argb(style.background));
    let (sw, sh) = (s(34), s(18));
    let (sx, sy) = (s(2), (h - sh) / 2);
    if focused {
        c.round_rect(sx - s(2), sy - s(2), sw + s(4), sh + s(4), sh / 2 + s(2), argb(theme::accent_soft()));
    }
    let track = if disabled { theme::disabled() } else if on { theme::accent() } else { theme::switch_off() };
    c.round_rect(sx, sy, sw, sh, sh / 2, argb(track));
    let knob = sh - s(6);
    let kx = if on { sx + sw - knob - s(3) } else { sx + s(3) };
    c.round_rect(kx, sy + s(3), knob, knob, knob / 2, argb(theme::knob()));
    let ink = if disabled { theme::disabled() } else { theme::text() };
    let label = RECT { left: sx + sw + s(10), top: 0, right: w, bottom: h };
    c.text(style.font, &text_of(di.hwndItem), label, ink, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
    c.blit(di.hDC, r.left, r.top);
}

/// Deixa um controle nativo (campo, lista, seletor) no tema atual.
pub unsafe fn theme_control(control: HWND, class: &str) {
    if class == "COMBOBOX" {
        // A caixa fechada é desenhada à mão (igual aos botões); a lista aberta segue nativa.
        SetWindowSubclass(control, Some(combo_proc), 2, 0);
    }
    if !theme::is_dark() {
        return;
    }
    let sub = match class {
        "EDIT" | "COMBOBOX" => "DarkMode_CFD",
        _ => "DarkMode_Explorer",
    };
    set_window_theme(control, sub);
    if class == "SysListView32" {
        let (bg, ink) = (theme::colorref(theme::card()), theme::colorref(theme::text()));
        SendMessageW(control, LVM_SETBKCOLOR, 0, bg as LPARAM);
        SendMessageW(control, LVM_SETTEXTBKCOLOR, 0, bg as LPARAM);
        SendMessageW(control, LVM_SETTEXTCOLOR, 0, ink as LPARAM);
        let header = SendMessageW(control, LVM_GETHEADER, 0, 0) as HWND;
        if !header.is_null() {
            set_window_theme(header, "DarkMode_ItemsView");
            // O tema escuro pinta o fundo do cabeçalho, mas deixa o texto escuro:
            // a lista passa a pintar o texto dos títulos das colunas com a cor do tema.
            SetWindowSubclass(control, Some(list_proc), 1, 0);
        }
    }
}

/// A lista recebe as notificações do próprio cabeçalho: na hora de desenhar cada
/// título de coluna, troca a cor do texto.
unsafe extern "system" fn list_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if msg == WM_NOTIFY {
        let header = SendMessageW(hwnd, LVM_GETHEADER, 0, 0) as HWND;
        let hdr = &*(lp as *const NMHDR);
        if hdr.hwndFrom == header && hdr.code == NM_CUSTOMDRAW {
            let draw = &*(lp as *const NMCUSTOMDRAW);
            match draw.dwDrawStage {
                CDDS_PREPAINT => return CDRF_NOTIFYITEMDRAW as LRESULT,
                CDDS_ITEMPREPAINT => {
                    SetTextColor(draw.hdc, theme::colorref(theme::text()));
                    return CDRF_DODEFAULT as LRESULT;
                }
                _ => {}
            }
        }
    }
    DefSubclassProc(hwnd, msg, wp, lp)
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
        WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE | WM_MOUSELEAVE => {
            if msg == WM_MOUSELEAVE {
                HOT.store(0, std::sync::atomic::Ordering::Relaxed);
            }
            let r = DefSubclassProc(hwnd, msg, wp, lp);
            InvalidateRect(hwnd, std::ptr::null(), 0);
            return r;
        }
        WM_MOUSEMOVE => {
            let mut track = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
            TrackMouseEvent(&mut track);
            if HOT.swap(hwnd as isize, std::sync::atomic::Ordering::Relaxed) != hwnd as isize {
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

/// O seletor com o mouse em cima (um por vez).
static HOT: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

unsafe fn paint_combo(hwnd: HWND, dc: HDC) {
    let mut r: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut r);
    let (w, h) = (r.right.max(1), r.bottom.max(1));
    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let s = |v: i32| v * dpi / 96;
    let enabled = IsWindowEnabled(hwnd) != 0;
    let open = SendMessageW(hwnd, CB_GETDROPPEDSTATE, 0, 0) != 0;
    let focused = GetFocus() == hwnd;
    let hot = HOT.load(std::sync::atomic::Ordering::Relaxed) == hwnd as isize;
    let mut c = Canvas::new(w, h);
    c.fill(0, 0, w, h, argb(theme::card()));
    let fill = if !enabled { theme::soft() } else if hot || open { theme::hover() } else { theme::soft() };
    let border = if !enabled { theme::border() } else if focused || open { theme::accent() } else { theme::border() };
    c.card((0, 0, w, h), s(7), argb(fill), argb(border));
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
    let text = RECT { left: s(10), top: 0, right: w - s(30), bottom: h };
    c.text(font, &label, text, ink, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
    // Setinha (um "v" de 2 px), virada para cima com a lista aberta.
    let (ax, ay, aw) = (w - s(20), h / 2, s(4));
    let stroke = s(2).max(2);
    let arrow = argb(if enabled { theme::muted() } else { theme::disabled() });
    for i in 0..=aw {
        let dy = if open { aw - i } else { i } - aw / 2;
        c.fill(ax + i, ay + dy - stroke / 2, stroke, stroke, arrow);
        c.fill(ax + 2 * aw - i, ay + dy - stroke / 2, stroke, stroke, arrow);
    }
    c.blit(dc, 0, 0);
}