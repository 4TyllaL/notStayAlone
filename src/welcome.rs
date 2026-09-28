//! Boas-vindas: aparece só na primeira vez que o app abre (sem config.ini).
//! Três passos — escolher o mascote, os lembretes e (opcional) a chave da IA —
//! e o resultado vai para o app pelo `mailbox` (`WM_WELCOME_DONE`).

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
        Controls::{BST_CHECKED, DRAWITEMSTRUCT, EM_LIMITTEXT, EM_SETCUEBANNER},
        HiDpi::{AdjustWindowRectExForDpi, GetDpiForSystem},
        Input::KeyboardAndMouse::SetFocus,
        Shell::ShellExecuteW,
        WindowsAndMessaging::*,
    },
};

use crate::{
    gfx::Canvas,
    mailbox,
    pack::{self, PackInfo},
    secret,
    sprite::{Frame, PIXELS},
    theme::{self, argb},
    ui,
    win::{self, text_of, ui_font, w},
};

/// Terminou (ou fechou): as escolhas (`Choices`) vão para o app pelo `mailbox`.
pub const WM_WELCOME_DONE: u32 = WM_APP + 13;

const WIDTH: i32 = 560;
const HEIGHT: i32 = 470;
const STEPS: usize = 3;
/// Mascotes mostrados no primeiro passo (os embutidos).
const TILES: usize = 4;
const TILE: i32 = 96;
const TILE_GAP: i32 = 16;
const TILE_TOP: i32 = 150;

const IDC_BACK: i32 = 10;
const IDC_NEXT: i32 = 11;
const IDC_WATER: i32 = 20;
const IDC_STRETCH: i32 = 21;
const IDC_EYES: i32 = 22;
const IDC_AUTOSTART: i32 = 23;
const IDC_GETKEY: i32 = 30;
const IDC_KEY: i32 = 31;

/// O que você escolheu.
pub struct Choices {
    pub mascot: String,
    /// Água, alongar, olhos.
    pub reminders: [bool; 3],
    pub autostart: bool,
}

/// (título, texto) de cada passo.
const TEXTS: [(&str, &str); STEPS] = [
    (
        "Oi! Eu sou o !StayAlone",
        "Um mascote que te faz companhia enquanto você usa o PC. Escolha quem vai morar na sua área de trabalho:",
    ),
    (
        "Como eu posso ajudar",
        "Lembretes gentis, contados só enquanto você usa o PC. Dá para mudar tudo depois nas Configurações.",
    ),
    (
        "Quer conversar comigo?",
        "Opcional: com uma chave gratuita do Google Gemini você conversa com o mascote e ele até desenha mascotes novos.",
    ),
];

struct State {
    owner: HWND,
    dpi: u32,
    step: usize,
    packs: Vec<(PackInfo, Vec<u32>)>,
    chosen: usize,
    font: HFONT,
    bold: HFONT,
    title: HFONT,
    brush: HBRUSH,
    /// (passo, controle): só os do passo atual aparecem.
    pages: Vec<(usize, HWND)>,
    done: bool,
}

thread_local! {
    static OPEN: Cell<HWND> = const { Cell::new(null_mut()) };
}

fn scale(v: i32, dpi: u32) -> i32 {
    v * dpi as i32 / 96
}

unsafe fn state<'a>(hwnd: HWND) -> Option<&'a mut State> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State).as_mut()
}

/// Tab/Enter/Esc como numa caixa de diálogo.
pub fn is_dialog_message(msg: &MSG) -> bool {
    let hwnd = OPEN.with(Cell::get);
    !hwnd.is_null() && unsafe { IsDialogMessageW(hwnd, msg) } != 0
}

/// Abre as boas-vindas com `current` pré-selecionado.
pub unsafe fn open(owner: HWND, current: &str, icon: HICON) {
    if !OPEN.with(Cell::get).is_null() {
        return;
    }
    let hinstance = GetModuleHandleW(null());
    let class = w("StayAloneWelcome");
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: hinstance,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hIcon: icon,
        hIconSm: icon,
        lpszClassName: class.as_ptr(),
        ..zeroed()
    };
    RegisterClassExW(&wc);

    let dpi = GetDpiForSystem();
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
    let mut rect = RECT { left: 0, top: 0, right: scale(WIDTH, dpi), bottom: scale(HEIGHT, dpi) };
    AdjustWindowRectExForDpi(&mut rect, style, 0, 0, dpi);
    let (ww, wh) = (rect.right - rect.left, rect.bottom - rect.top);
    let mut work: RECT = zeroed();
    SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut work as *mut RECT).cast(), 0);
    let (x, y) = (work.left + (work.right - work.left - ww) / 2, (work.top + (work.bottom - work.top - wh) / 2).max(work.top));
    let hwnd = CreateWindowExW(0, class.as_ptr(), w("Bem-vindo — !StayAlone").as_ptr(), style, x, y, ww, wh, null_mut(), null_mut(), hinstance, null());
    if hwnd.is_null() {
        return;
    }
    win::caption_color(hwnd, theme::BG);

    let packs: Vec<(PackInfo, Vec<u32>)> = pack::list()
        .into_iter()
        .take(TILES)
        .map(|info| {
            let mut pixels = vec![0; PIXELS];
            if let Ok(p) = pack::load(&info) {
                p.art.draw(Frame::Idle, false, 1, &mut pixels);
            }
            (info, pixels)
        })
        .collect();
    let chosen = packs.iter().position(|(p, _)| p.id == current).unwrap_or(0);
    let st = Box::new(State {
        owner,
        dpi,
        step: 0,
        packs,
        chosen,
        font: ui_font(scale(15, dpi), FW_NORMAL),
        bold: ui_font(scale(15, dpi), FW_SEMIBOLD),
        title: ui_font(scale(24, dpi), FW_SEMIBOLD),
        brush: CreateSolidBrush(theme::colorref(theme::CARD)),
        pages: Vec::new(),
        done: false,
    });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
    OPEN.with(|o| o.set(hwnd));
    build(hwnd);
    show_step(hwnd, 0);
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
}

unsafe fn build(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let dpi = st.dpi;
    let s = |v: i32| scale(v, dpi);
    let font = st.font;
    let pages = &mut st.pages;
    let mut add = |step: Option<usize>, class: &str, text: &str, style: u32, rect: (i32, i32, i32, i32), id: i32| {
        let (x, y, cw, ch) = rect;
        let ex = if class == "EDIT" { WS_EX_CLIENTEDGE } else { 0 };
        let control = CreateWindowExW(
            ex,
            w(class).as_ptr(),
            w(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            s(x),
            s(y),
            s(cw),
            s(ch),
            hwnd,
            id as isize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(control, WM_SETFONT, font as WPARAM, 1);
        if let Some(p) = step {
            pages.push((p, control));
        }
        control
    };
    let check = BS_AUTOCHECKBOX as u32 | WS_TABSTOP;
    let button = BS_OWNERDRAW as u32 | WS_TABSTOP;

    for (i, (label, on)) in [("Lembrar de beber água (a cada 45 min)", true), ("Lembrar de alongar (a cada 60 min)", true), ("Lembrar de descansar os olhos (a cada 20 min)", false)]
        .into_iter()
        .enumerate()
    {
        let control = add(Some(1), "BUTTON", label, check, (44, 160 + i as i32 * 32, 440, 24), IDC_WATER + i as i32);
        SendMessageW(control, BM_SETCHECK, on as WPARAM, 0);
    }
    add(Some(1), "BUTTON", "Abrir junto com o Windows", check, (44, 270, 440, 24), IDC_AUTOSTART);

    add(Some(2), "BUTTON", "Criar uma chave grátis", button, (44, 176, 200, 32), IDC_GETKEY);
    let key = add(Some(2), "EDIT", "", ES_AUTOHSCROLL as u32 | ES_PASSWORD as u32 | WS_TABSTOP, (44, 250, 330, 26), IDC_KEY);
    SendMessageW(key, EM_SETCUEBANNER, 1, w("cole a chave aqui (opcional)").as_ptr() as LPARAM);
    SendMessageW(key, EM_LIMITTEXT, 512, 0);

    add(None, "BUTTON", "Voltar", button, (24, HEIGHT - 58, 110, 34), IDC_BACK);
    add(None, "BUTTON", "Próximo", button, (WIDTH - 24 - 130, HEIGHT - 58, 130, 34), IDC_NEXT);
}

unsafe fn show_step(hwnd: HWND, step: usize) {
    let Some(st) = state(hwnd) else { return };
    st.step = step.min(STEPS - 1);
    for &(p, control) in &st.pages {
        ShowWindow(control, if p == st.step { SW_SHOW } else { SW_HIDE });
    }
    ShowWindow(GetDlgItem(hwnd, IDC_BACK), if st.step == 0 { SW_HIDE } else { SW_SHOW });
    let next = if st.step == STEPS - 1 { "Começar!" } else { "Próximo" };
    SetWindowTextW(GetDlgItem(hwnd, IDC_NEXT), w(next).as_ptr());
    InvalidateRect(hwnd, null(), 0);
}

/// Retângulo do mascote `i` no primeiro passo (em pixels).
fn tile_rect(i: usize, count: usize, dpi: u32) -> RECT {
    let total = count as i32 * TILE + (count as i32 - 1) * TILE_GAP;
    let x = (WIDTH - total) / 2 + i as i32 * (TILE + TILE_GAP);
    RECT { left: scale(x, dpi), top: scale(TILE_TOP, dpi), right: scale(x + TILE, dpi), bottom: scale(TILE_TOP + TILE, dpi) }
}

unsafe fn paint(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let mut ps: PAINTSTRUCT = zeroed();
    let dc = BeginPaint(hwnd, &mut ps);
    let mut client: RECT = zeroed();
    GetClientRect(hwnd, &mut client);
    let s = |v: i32| scale(v, st.dpi);
    let mut c = Canvas::new(client.right.max(1), client.bottom.max(1));
    let (width, height) = (c.width, c.height);
    c.fill(0, 0, width, height, argb(theme::BG));
    c.card((s(16), s(16), width - s(32), height - s(96)), s(14), argb(theme::CARD), argb(theme::BORDER));

    let (title, text) = TEXTS[st.step];
    let left = DT_LEFT | DT_TOP | DT_WORDBREAK;
    c.text(st.title, title, RECT { left: s(44), top: s(36), right: width - s(44), bottom: s(72) }, theme::TEXT, left);
    c.text(st.font, text, RECT { left: s(44), top: s(80), right: width - s(44), bottom: s(140) }, theme::MUTED, left);

    match st.step {
        0 => {
            let center = DT_CENTER | DT_TOP | DT_SINGLELINE;
            for (i, (info, pixels)) in st.packs.iter().enumerate() {
                let r = tile_rect(i, st.packs.len(), st.dpi);
                let size = r.right - r.left;
                if i == st.chosen {
                    c.round_rect(r.left, r.top, size, size, s(16), argb(theme::ACCENT));
                    c.round_rect(r.left + s(3), r.top + s(3), size - s(6), size - s(6), s(13), argb(theme::ACCENT_SOFT));
                } else {
                    c.round_rect(r.left, r.top, size, size, s(16), argb(theme::SOFT));
                }
                let px = (s(64) / 16).max(1);
                c.sprite(r.left + (size - 16 * px) / 2, r.top + (size - 16 * px) / 2, px, pixels);
                let name_rect = RECT { left: r.left - s(10), top: r.bottom + s(8), right: r.right + s(10), bottom: r.bottom + s(30) };
                let font = if i == st.chosen { st.bold } else { st.font };
                c.text(font, &info.name, name_rect, if i == st.chosen { theme::ACCENT } else { theme::TEXT }, center);
            }
            let hint = RECT { left: s(44), top: s(300), right: width - s(44), bottom: s(340) };
            c.text(st.font, "Dá para trocar depois pelo painel (botão direito no mascote).", hint, theme::MUTED, DT_CENTER | DT_TOP | DT_WORDBREAK);
        }
        1 => {
            let tip = RECT { left: s(56), top: s(316), right: width - s(56), bottom: s(370) };
            c.round_rect(tip.left - s(12), tip.top - s(10), tip.right - tip.left + s(24), tip.bottom - tip.top + s(12), s(10), argb(theme::ACCENT_SOFT));
            let text = "Dica: clique no mascote para fazer carinho, arraste para carregar e use o botão direito para abrir o painel.";
            c.text(st.font, text, tip, theme::TEXT, left);
        }
        _ => {
            let label = RECT { left: s(44), top: s(226), right: width - s(60), bottom: s(248) };
            c.text(st.bold, "Cole a chave aqui", label, theme::TEXT, left);
            let hint = RECT { left: s(44), top: s(290), right: width - s(44), bottom: s(360) };
            let text = "Ela fica no Gerenciador de Credenciais do Windows, nunca em arquivo. Pode pular: dá para fazer isso depois em Configurações → Conversa.";
            c.text(st.font, text, hint, theme::MUTED, left);
        }
    }

    // Bolinhas do passo atual, entre os botões.
    for i in 0..STEPS {
        let size = if i == st.step { s(10) } else { s(8) };
        let x = width / 2 + (i as i32 - 1) * s(22) - size / 2;
        let y = height - s(41) - size / 2;
        c.round_rect(x, y, size, size, size / 2, argb(if i == st.step { theme::ACCENT } else { theme::BORDER }));
    }
    c.blit(dc, 0, 0);
    EndPaint(hwnd, &ps);
}

/// "Próximo" no último passo (ou fechar): salva a chave e manda as escolhas ao app.
unsafe fn finish(hwnd: HWND) -> bool {
    let Some(st) = state(hwnd) else { return true };
    let mut key = text_of(GetDlgItem(hwnd, IDC_KEY));
    if !key.is_empty() {
        let saved = secret::save("GEMINI_API_KEY", &key);
        key.as_bytes_mut().fill(0);
        if let Err(e) = saved {
            show_step(hwnd, 2);
            MessageBoxW(hwnd, w(&format!("Não salvei a chave: {e}")).as_ptr(), w("!StayAlone").as_ptr(), MB_ICONINFORMATION);
            SetFocus(GetDlgItem(hwnd, IDC_KEY));
            return false;
        }
    }
    let checked = |id: i32| SendMessageW(GetDlgItem(hwnd, id), BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
    let choices = Choices {
        mascot: st.packs.get(st.chosen).map_or_else(|| pack::DEFAULT.to_string(), |(p, _)| p.id.clone()),
        reminders: [checked(IDC_WATER), checked(IDC_STRETCH), checked(IDC_EYES)],
        autostart: checked(IDC_AUTOSTART),
    };
    st.done = true;
    mailbox::post(st.owner, WM_WELCOME_DONE, choices);
    true
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_LBUTTONUP => {
            let Some(st) = state(hwnd) else { return 0 };
            if st.step == 0 {
                let (x, y) = ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
                let count = st.packs.len();
                if let Some(i) = (0..count).find(|&i| {
                    let r = tile_rect(i, count, st.dpi);
                    x >= r.left && x < r.right && y >= r.top && y < r.bottom
                }) {
                    st.chosen = i;
                    InvalidateRect(hwnd, null(), 0);
                }
            }
            0
        }
        WM_DRAWITEM => {
            let Some(st) = state(hwnd) else { return 0 };
            let di = &*(lp as *const DRAWITEMSTRUCT);
            let id = di.CtlID as i32;
            let style = ui::ButtonStyle {
                primary: id == IDC_NEXT,
                background: if id == IDC_GETKEY { theme::CARD } else { theme::BG },
                font: st.font,
                bold: st.bold,
                dpi: st.dpi,
            };
            ui::draw_button(di, &style);
            1
        }
        WM_CTLCOLORSTATIC => {
            let Some(st) = state(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
            SetBkMode(wp as HDC, TRANSPARENT as _);
            SetTextColor(wp as HDC, theme::colorref(theme::TEXT));
            st.brush as LRESULT
        }
        WM_COMMAND => {
            let Some(st) = state(hwnd) else { return 0 };
            match (wp & 0xFFFF) as i32 {
                IDC_BACK => show_step(hwnd, st.step.saturating_sub(1)),
                // Enter (IDOK) avança como o botão.
                IDC_NEXT | 1 if st.step + 1 < STEPS => show_step(hwnd, st.step + 1),
                IDC_NEXT | 1 => {
                    if finish(hwnd) {
                        DestroyWindow(hwnd);
                    }
                }
                IDC_GETKEY => {
                    ShellExecuteW(hwnd, w("open").as_ptr(), w("https://aistudio.google.com/apikey").as_ptr(), null(), null(), SW_SHOWNORMAL);
                }
                _ => {}
            }
            0
        }
        // Fechar no X também conta como "pronto": não pergunta de novo na próxima vez.
        WM_CLOSE => {
            if state(hwnd).is_some_and(|st| !st.done) && finish(hwnd) {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            OPEN.with(|o| o.set(null_mut()));
            0
        }
        WM_NCDESTROY => {
            let st = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
            if !st.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let st = Box::from_raw(st);
                for object in [st.font, st.bold, st.title, st.brush] {
                    DeleteObject(object);
                }
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mascot_tiles_are_centered_and_apart() {
        let rects: Vec<RECT> = (0..TILES).map(|i| tile_rect(i, TILES, 96)).collect();
        assert_eq!(rects[0].left, WIDTH - rects[TILES - 1].right);
        assert!(rects.windows(2).all(|p| p[0].right < p[1].left));
    }
}
