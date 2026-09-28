//! Pergunta de sim/não no visual do app (no lugar da `MessageBox` do Windows):
//! cartão com ícone, texto, um destaque opcional, uma lista com ✓ e uma nota em cinza.
//! É modal: o dono fica desabilitado até a resposta. Esc, fechar ou "não" = `false`;
//! o foco começa no "não", então Enter sem pensar não aprova nada.
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
const WIDTH: i32 = 480;
const PAD: i32 = 20;
/// Onde o texto começa (à direita do ícone).
const TEXT_X: i32 = 92;
const ID_TITLE: i32 = 10;
const ID_BODY: i32 = 11;
/// Destaques: 30, 31, ...
const ID_CHIP: i32 = 30;
const ID_LIST_TITLE: i32 = 13;
const ID_ITEM: i32 = 20; // 20, 21, ...
const ID_NOTE: i32 = 14;
const SHIELD: char = '\u{EA18}';
const WARNING: char = '\u{E7BA}';
const CHECK: char = '\u{E73E}';
/// Estilos de STATIC (winuser.h; o windows-sys não exporta): texto à esquerda, com quebra, sem &.
const SS_LEFT: u32 = 0x0;
const SS_NOPREFIX: u32 = 0x80;

/// O que perguntar.
pub struct Question<'a> {
    pub title: &'a str,
    pub body: &'a str,
    /// Destaques logo abaixo do texto; `true` = alerta (laranja).
    pub chips: &'a [(&'a str, bool)],
    pub list_title: &'a str,
    pub items: &'a [&'a str],
    /// Nota em cinza no fim (ex.: o SHA-256).
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
    soft_brush: HBRUSH,
    alert_brush: HBRUSH,
    /// Cartão, ícone, destaque e as marcas ✓ (em pixels).
    card: RECT,
    chips: Vec<(RECT, bool)>,
    checks: Vec<(i32, i32)>,
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
    let dark = theme::is_dark();
    let mut st = Box::new(State {
        dpi,
        font: font(14, FW_NORMAL),
        bold: font(14, FW_SEMIBOLD),
        title: font(18, FW_SEMIBOLD),
        small: font(12, FW_NORMAL),
        icons: theme::icon_font(s(14)),
        big_icon: theme::icon_font(s(24)),
        card_brush: CreateSolidBrush(theme::colorref(theme::card())),
        soft_brush: CreateSolidBrush(theme::colorref(theme::soft())),
        alert_brush: CreateSolidBrush(theme::colorref(theme::accent_soft())),
        card: zeroed(),
        chips: Vec::new(),
        checks: Vec::new(),
    });

    // Layout: mede cada texto na largura disponível e empilha.
    let text_w = s(WIDTH - PAD - TEXT_X);
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
    let x = s(TEXT_X);
    let mut labels: Vec<(i32, RECT, HFONT, &str)> = Vec::new();
    let mut y = s(PAD + 20);
    let title_h = measure(st.title, q.title, text_w);
    labels.push((ID_TITLE, RECT { left: x, top: y, right: x + text_w, bottom: y + title_h }, st.title, q.title));
    y += title_h + s(6);
    if !q.body.is_empty() {
        let h = measure(st.font, q.body, text_w);
        labels.push((ID_BODY, RECT { left: x, top: y, right: x + text_w, bottom: y + h }, st.font, q.body));
        y += h + s(12);
    }
    for (i, &(text, alert)) in q.chips.iter().enumerate() {
        let inner = text_w - s(40);
        let h = measure(st.font, text, inner);
        let chip = RECT { left: x, top: y, right: x + text_w, bottom: y + h + s(16) };
        let id = ID_CHIP + i as i32;
        labels.push((id, RECT { left: x + s(32), top: y + s(8), right: x + s(32) + inner, bottom: y + s(8) + h }, st.font, text));
        st.chips.push((chip, alert));
        y = chip.bottom + s(6);
    }
    if !q.chips.is_empty() {
        y += s(10);
    }
    if !q.items.is_empty() {
        let h = measure(st.bold, q.list_title, text_w);
        labels.push((ID_LIST_TITLE, RECT { left: x, top: y, right: x + text_w, bottom: y + h }, st.bold, q.list_title));
        y += h + s(6);
        for (i, item) in q.items.iter().enumerate() {
            let inner = text_w - s(24);
            let h = measure(st.font, item, inner);
            st.checks.push((x, y));
            labels.push((ID_ITEM + i as i32, RECT { left: x + s(24), top: y, right: x + s(24) + inner, bottom: y + h }, st.font, item));
            y += h + s(4);
        }
        y += s(10);
    }
    if !q.note.is_empty() {
        let h = measure(st.small, q.note, text_w);
        labels.push((ID_NOTE, RECT { left: x, top: y, right: x + text_w, bottom: y + h }, st.small, q.note));
        y += h;
    }
    st.card = RECT { left: s(PAD), top: s(PAD), right: s(WIDTH - PAD), bottom: y + s(20) };
    let (button_h, button_top) = (s(32), st.card.bottom + s(14));
    let (cw, ch) = (s(WIDTH), button_top + button_h + s(PAD - 4));

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
    win::dark_title(hwnd, dark);

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
    let (yes_w, no_w) = (s(136), s(120));
    let yes_x = cw - s(PAD) - yes_w;
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
    // Ícone: escudo num círculo (alerta, se algum destaque for um alerta).
    let alert = st.chips.iter().any(|&(_, alert)| alert);
    let (ix, iy, size) = (r.left + s(20), r.top + s(20), s(44));
    c.round_rect(ix, iy, size, size, size / 2, argb(theme::accent_soft()));
    let glyph = if alert { WARNING } else { SHIELD };
    let icon_rect = RECT { left: ix, top: iy, right: ix + size, bottom: iy + size };
    c.text(st.big_icon, &glyph.to_string(), icon_rect, theme::accent(), DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    for &(chip, alert) in &st.chips {
        let fill = if alert { theme::accent_soft() } else { theme::soft() };
        c.round_rect(chip.left, chip.top, chip.right - chip.left, chip.bottom - chip.top, s(8), argb(fill));
        let mark = RECT { left: chip.left + s(8), top: chip.top + s(8), right: chip.left + s(28), bottom: chip.top + s(28) };
        let (glyph, ink) = if alert { (WARNING, theme::accent_dark()) } else { (CHECK, theme::muted()) };
        c.text(st.icons, &glyph.to_string(), mark, ink, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    }
    for &(x, y) in &st.checks {
        let mark = RECT { left: x, top: y + s(2), right: x + s(18), bottom: y + s(20) };
        c.text(st.icons, &CHECK.to_string(), mark, theme::accent(), DT_LEFT | DT_TOP | DT_SINGLELINE);
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
            let chip = usize::try_from(id - ID_CHIP).ok().and_then(|i| st.chips.get(i));
            let (ink, brush) = match chip {
                Some((_, true)) => (theme::accent_dark(), st.alert_brush),
                Some((_, false)) => (theme::text(), st.soft_brush),
                None if id == ID_NOTE => (theme::muted(), st.card_brush),
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
                DeleteObject(st.soft_brush);
                DeleteObject(st.alert_brush);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
