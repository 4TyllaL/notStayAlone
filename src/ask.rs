//! Pergunta de sim/não no visual do app (no lugar da `MessageBox` do Windows), curta:
//! ícone e título, uma frase, um quadro com uma linha por item (ícone + texto; laranja
//! quando pede atenção) e uma nota em cinza. É modal: o dono fica desabilitado até a
//! resposta. Esc, fechar ou "não" = `false`; o foco começa no "não", então Enter sem
//! pensar não aprova nada.
//!
//! O texto fica em controles STATIC (leitores de tela e o smoke test leem de lá);
//! o fundo, o cartão e os ícones são pintados.

use std::{
    cell::Cell,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};

use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::DRAWITEMSTRUCT,
        HiDpi::{AdjustWindowRectExForDpi, GetDpiForSystem},
        Input::KeyboardAndMouse::{EnableWindow, GetFocus, SetFocus, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_TAB},
        WindowsAndMessaging::*,
    },
};

use crate::{
    gfx::Canvas,
    theme::{self, argb},
    ui,
    win::{self, ui_font, w},
};

const CLASS: &str = "StayAloneAsk";
const WIDTH: i32 = 440;
/// Margem da janela até o cartão e do cartão até o conteúdo.
const MARGIN: i32 = 16;
const INSET: i32 = 20;
const ICON: i32 = 40;
const ID_TITLE: i32 = 10;
const ID_SUBTITLE: i32 = 11;
const ID_NOTE: i32 = 14;
/// Linhas do quadro: 30, 31, ...
const ID_ROW: i32 = 30;
/// Estilos de STATIC (winuser.h; o windows-sys não exporta): texto à esquerda, com quebra, sem &.
const SS_LEFT: u32 = 0x0;
const SS_NOPREFIX: u32 = 0x80;

/// Ícones (Segoe Fluent Icons / MDL2) para as linhas e o cabeçalho.
pub mod icon {
    pub const SHIELD: char = '\u{EA18}';
    pub const LOCK: char = '\u{E72E}';
    pub const GLOBE: char = '\u{E774}';
    pub const FOLDER: char = '\u{E8B7}';
    pub const WARNING: char = '\u{E7BA}';
}

/// Uma linha do quadro.
pub struct Row<'a> {
    pub icon: char,
    pub text: &'a str,
    /// Pede atenção (laranja).
    pub warn: bool,
}

/// O que perguntar.
pub struct Question<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub rows: &'a [Row<'a>],
    /// Nota em cinza no fim, numa linha curta.
    pub note: &'a str,
    pub yes: &'a str,
    pub no: &'a str,
}

struct State {
    dpi: u32,
    font: HFONT,
    bold: HFONT,
    title: HFONT,
    small: HFONT,
    icons: HFONT,
    big_icon: HFONT,
    card_brush: HBRUSH,
    panel_brush: HBRUSH,
    card: RECT,
    panel: Option<RECT>,
    /// Ícone de cada linha: posição, glifo, atenção?
    marks: Vec<(i32, i32, char, bool)>,
    warn: bool,
}

thread_local! {
    static ANSWER: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Mostra a pergunta e espera a resposta (`true` = sim).
pub unsafe fn ask(owner: HWND, q: &Question) -> bool {
    let hinstance = GetModuleHandleW(null());
    let class = w(CLASS);
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: hinstance,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        lpszClassName: class.as_ptr(),
        ..zeroed()
    };
    RegisterClassExW(&wc); // da segunda vez falha (já existe), e tudo bem

    let dpi = GetDpiForSystem();
    let s = |v: i32| v * dpi as i32 / 96;
    let font = |size: i32, weight: u32| ui_font(s(size), weight);
    let warn = q.rows.iter().any(|r| r.warn);
    let mut st = Box::new(State {
        dpi,
        font: font(14, FW_NORMAL),
        bold: font(14, FW_SEMIBOLD),
        title: font(17, FW_SEMIBOLD),
        small: font(12, FW_NORMAL),
        icons: theme::icon_font(s(15)),
        big_icon: theme::icon_font(s(20)),
        card_brush: CreateSolidBrush(theme::colorref(theme::card())),
        panel_brush: CreateSolidBrush(theme::colorref(theme::soft())),
        card: zeroed(),
        panel: None,
        marks: Vec::new(),
        warn,
    });

    let measure = |font: HFONT, text: &str, width: i32| -> i32 {
        let dc = CreateCompatibleDC(null_mut());
        let old = SelectObject(dc, font);
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let mut r = RECT { left: 0, top: 0, right: width, bottom: 0 };
        DrawTextW(dc, wide.as_mut_ptr(), wide.len() as i32, &mut r, DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX);
        SelectObject(dc, old);
        DeleteDC(dc);
        r.bottom
    };

    // Layout: tudo alinhado à mesma margem do cartão; só o título fica ao lado do ícone.
    let (left, right) = (s(MARGIN + INSET), s(WIDTH - MARGIN - INSET));
    let mut labels: Vec<(i32, RECT, HFONT, &str)> = Vec::new();
    let top = s(MARGIN + INSET);
    let head_x = left + s(ICON + 14);
    let title_h = measure(st.title, q.title, right - head_x);
    let sub_h = if q.subtitle.is_empty() { 0 } else { measure(st.small, q.subtitle, right - head_x) };
    let head_h = title_h + if sub_h > 0 { s(2) + sub_h } else { 0 };
    // Título (e frase) centralizados na altura do ícone quando cabem.
    let head_y = top + (s(ICON) - head_h).max(0) / 2;
    labels.push((ID_TITLE, RECT { left: head_x, top: head_y, right, bottom: head_y + title_h }, st.title, q.title));
    if sub_h > 0 {
        let y = head_y + title_h + s(2);
        labels.push((ID_SUBTITLE, RECT { left: head_x, top: y, right, bottom: y + sub_h }, st.small, q.subtitle));
    }
    let mut y = (top + s(ICON)).max(head_y + head_h) + s(16);
    if !q.rows.is_empty() {
        let panel_top = y;
        y += s(10);
        let text_x = left + s(12 + 26);
        for (i, row) in q.rows.iter().enumerate() {
            let h = measure(st.font, row.text, right - s(12) - text_x).max(s(20));
            st.marks.push((left + s(12), y, row.icon, row.warn));
            labels.push((ID_ROW + i as i32, RECT { left: text_x, top: y, right: right - s(12), bottom: y + h }, st.font, row.text));
            y += h + s(8);
        }
        y += s(2);
        st.panel = Some(RECT { left, top: panel_top, right, bottom: y });
        y += s(12);
    }
    if !q.note.is_empty() {
        let h = measure(st.small, q.note, right - left);
        labels.push((ID_NOTE, RECT { left, top: y, right, bottom: y + h }, st.small, q.note));
        y += h;
    }
    st.card = RECT { left: s(MARGIN), top: s(MARGIN), right: s(WIDTH - MARGIN), bottom: y + s(INSET) };
    let (button_h, button_top) = (s(32), st.card.bottom + s(14));
    let (cw, ch) = (s(WIDTH), button_top + button_h + s(MARGIN));

    let style = WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
    let mut rect = RECT { left: 0, top: 0, right: cw, bottom: ch };
    AdjustWindowRectExForDpi(&mut rect, style, 0, 0, dpi);
    let (ww, wh) = (rect.right - rect.left, rect.bottom - rect.top);
    // No meio do dono (ou da tela).
    let mut area: RECT = zeroed();
    if owner.is_null() || GetWindowRect(owner, &mut area) == 0 {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut area as *mut RECT).cast(), 0);
    }
    let (wx, wy) = (area.left + (area.right - area.left - ww) / 2, (area.top + (area.bottom - area.top - wh) / 2).max(0));
    let hwnd = CreateWindowExW(
        WS_EX_DLGMODALFRAME,
        class.as_ptr(),
        w("!StayAlone").as_ptr(),
        style,
        wx,
        wy,
        ww,
        wh,
        owner,
        null_mut(),
        hinstance,
        null(),
    );
    if hwnd.is_null() {
        return false;
    }
    let (font, bold) = (st.font, st.bold);
    // O estado vem antes dos controles: os botões já se desenham com ele.
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
    win::caption_color(hwnd, theme::bg());
    win::dark_title(hwnd, theme::is_dark());

    let add = |class: &str, text: &str, style: u32, r: RECT, id: i32, font: HFONT| {
        let control = CreateWindowExW(
            0,
            w(class).as_ptr(),
            w(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            hwnd,
            id as isize as HMENU,
            hinstance,
            null(),
        );
        SendMessageW(control, WM_SETFONT, font as WPARAM, 1);
        control
    };
    for &(id, r, font, text) in &labels {
        add("STATIC", text, SS_LEFT | SS_NOPREFIX, r, id, font);
    }
    let button = BS_OWNERDRAW as u32 | WS_TABSTOP;
    let (yes_w, no_w) = (s(128), s(112));
    let yes_x = cw - s(MARGIN) - yes_w;
    let no_x = yes_x - s(8) - no_w;
    let no = add("BUTTON", q.no, button, RECT { left: no_x, top: button_top, right: no_x + no_w, bottom: button_top + button_h }, IDNO, font);
    add("BUTTON", q.yes, button, RECT { left: yes_x, top: button_top, right: yes_x + yes_w, bottom: button_top + button_h }, IDYES, bold);

    // Modal: o dono fica desabilitado até a resposta.
    ANSWER.with(|a| a.set(None));
    let owner_was_enabled = !owner.is_null() && EnableWindow(owner, 0) == 0;
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    SetFocus(no);
    let mut msg: MSG = zeroed();
    while ANSWER.with(Cell::get).is_none() {
        let got = GetMessageW(&mut msg, null_mut(), 0, 0);
        if got <= 0 {
            if got == 0 {
                PostQuitMessage(msg.wParam as i32); // devolve o WM_QUIT para o laço principal
            }
            break;
        }
        if msg.message == WM_KEYDOWN && (msg.hwnd == hwnd || IsChild(hwnd, msg.hwnd) != 0) {
            let focus_yes = GetDlgCtrlID(GetFocus()) == IDYES;
            match msg.wParam as u16 {
                VK_ESCAPE => ANSWER.with(|a| a.set(Some(false))),
                VK_RETURN => ANSWER.with(|a| a.set(Some(focus_yes))),
                VK_TAB | VK_LEFT | VK_RIGHT => {
                    SetFocus(GetDlgItem(hwnd, if focus_yes { IDNO } else { IDYES }));
                }
                _ => {}
            }
            continue;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    let answer = ANSWER.with(Cell::get).unwrap_or(false);
    if owner_was_enabled {
        EnableWindow(owner, 1);
    }
    DestroyWindow(hwnd);
    if !owner.is_null() {
        SetForegroundWindow(owner);
    }
    answer
}

unsafe fn state<'a>(hwnd: HWND) -> Option<&'a mut State> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State).as_mut()
}

unsafe fn paint(hwnd: HWND, st: &State) {
    let mut ps: PAINTSTRUCT = zeroed();
    let dc = BeginPaint(hwnd, &mut ps);
    let mut client: RECT = zeroed();
    GetClientRect(hwnd, &mut client);
    let s = |v: i32| v * st.dpi as i32 / 96;
    let mut c = Canvas::new(client.right.max(1), client.bottom.max(1));
    c.fill(0, 0, client.right, client.bottom, argb(theme::bg()));
    let r = st.card;
    c.card((r.left, r.top, r.right - r.left, r.bottom - r.top), s(10), argb(theme::card()), argb(theme::border()));
    // Ícone do cabeçalho: escudo (ou alerta, se alguma linha pede atenção).
    let (ix, iy, size) = (r.left + s(INSET), r.top + s(INSET), s(ICON));
    c.round_rect(ix, iy, size, size, size / 2, argb(theme::accent_soft()));
    let glyph = if st.warn { icon::WARNING } else { icon::SHIELD };
    let icon_rect = RECT { left: ix, top: iy, right: ix + size, bottom: iy + size };
    c.text(st.big_icon, &glyph.to_string(), icon_rect, theme::accent(), DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    if let Some(p) = st.panel {
        c.round_rect(p.left, p.top, p.right - p.left, p.bottom - p.top, s(8), argb(theme::soft()));
    }
    for &(x, y, glyph, warn) in &st.marks {
        let mark = RECT { left: x, top: y, right: x + s(20), bottom: y + s(20) };
        let ink = if warn { theme::accent() } else { theme::muted() };
        c.text(st.icons, &glyph.to_string(), mark, ink, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    }
    c.blit(dc, 0, 0);
    EndPaint(hwnd, &ps);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            match state(hwnd) {
                Some(st) => paint(hwnd, st),
                None => return DefWindowProcW(hwnd, msg, wp, lp),
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            let Some(st) = state(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
            let dc = wp as HDC;
            SetBkMode(dc, TRANSPARENT as _);
            let id = GetDlgCtrlID(lp as HWND);
            let row = usize::try_from(id - ID_ROW).ok().and_then(|i| st.marks.get(i));
            let (ink, brush) = match row {
                Some(&(_, _, _, true)) => (theme::accent_dark(), st.panel_brush),
                Some(_) => (theme::text(), st.panel_brush),
                None if id == ID_NOTE || id == ID_SUBTITLE => (theme::muted(), st.card_brush),
                None => (theme::text(), st.card_brush),
            };
            SetTextColor(dc, theme::colorref(ink));
            brush as LRESULT
        }
        WM_DRAWITEM => {
            let Some(st) = state(hwnd) else { return 0 };
            let di = &*(lp as *const DRAWITEMSTRUCT);
            let style = ui::ButtonStyle {
                primary: di.CtlID as i32 == IDYES,
                background: theme::bg(),
                font: st.font,
                bold: st.bold,
                dpi: st.dpi,
            };
            ui::draw_button(di, &style);
            1
        }
        WM_COMMAND => {
            match (wp & 0xFFFF) as i32 {
                IDYES => ANSWER.with(|a| a.set(Some(true))),
                IDNO | IDCANCEL => ANSWER.with(|a| a.set(Some(false))),
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            ANSWER.with(|a| a.set(Some(false)));
            0
        }
        WM_NCDESTROY => {
            let st = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
            if !st.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let st = Box::from_raw(st);
                for font in [st.font, st.bold, st.title, st.small, st.icons, st.big_icon] {
                    DeleteObject(font);
                }
                DeleteObject(st.card_brush);
                DeleteObject(st.panel_brush);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
