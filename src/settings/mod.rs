//! Janela de configurações: barra lateral com o mascote e as páginas (Geral,
//! Lembretes, Conversa, Criar mascote, Plugins), cada página em cartões brancos.
//! Campos nativos do Windows; fundo, cartões, navegação e botões desenhados aqui.
//! Criada só quando é aberta e destruída ao fechar. Trabalha numa cópia da
//! configuração: o Salvar manda a cópia para o app aplicar.

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
        Controls::*,
        HiDpi::{AdjustWindowRectExForDpi, GetDpiForSystem},
        Input::KeyboardAndMouse::{EnableWindow, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_RETURN},
        Shell::ShellExecuteW,
        WindowsAndMessaging::*,
    },
};

mod editor;

use editor::{canvas_proc, palette_proc, redraw_editor, CELL, SWATCH, SWATCH_GAP};

use crate::{
    chat::{self, WM_CHAT_REPLY},
    companion::{Reminder, ReminderKind},
    config::{self, ChatSettings, Config, Language, Size, Theme, MAX_REMINDERS, MAX_REMINDER_TEXT, PROVIDERS},
    mailbox,
    maker::{Drawing, Pose},
    memory,
    gallery,
    pack::{self, PackInfo},
    child::Reply,
    lang::{fill, tr},
    plugins::{self, Enabled, Kind as PluginKind, Plugin, Status},
    secret::{self, KeySource},
    gfx::Canvas,
    sprite::{Frame, PIXELS, SPRITE},
    theme::{self, argb, icon},
    ui,
    win::{self, text_of, ui_font, w},
};

/// OK clicado: o `Draft` vai para o app pelo `mailbox`.
pub const WM_SETTINGS_APPLY: u32 = WM_APP + 5;
/// Mascote criado aqui foi salvo: o id (`String`) vai para o app pelo `mailbox`.
pub const WM_MASCOT_SAVED: u32 = WM_APP + 8;
/// "Procurar atualização" na página Sobre: o app procura e o mascote responde.
pub const WM_UPDATE_REQUEST: u32 = WM_APP + 16;
/// Interna: um plugin foi marcado/desmarcado na lista (tratado fora da notificação,
/// porque pode abrir uma pergunta). O que mudou fica em `State::plugin_toggle`.
const WM_PLUGIN_TOGGLE: u32 = WM_APP + 20;

pub struct Draft {
    pub config: Config,
    pub autostart: bool,
}

/// Páginas, na ordem da barra lateral.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    General,
    Reminders,
    Chat,
    Maker,
    Plugins,
    Gallery,
    /// Fora da lista: aberta pelo item "Sobre" no rodapé da barra lateral.
    About,
}

/// Índice da página "Sobre" (depois das páginas da navegação).
const ABOUT: usize = Page::About as usize;
/// Links da página "Sobre".
const PROJECTS_URL: &str = "https://4tyllal.github.io/#projects";
const GITHUB_URL: &str = "https://github.com/4TyllaL/notStayAlone";

const PAGES: [(Page, char, &str); 6] = [
    (Page::General, icon::HOME, "Geral"),
    (Page::Reminders, icon::BELL, "Lembretes"),
    (Page::Chat, icon::CHAT, "Conversa"),
    (Page::Maker, icon::PALETTE, "Criar mascote"),
    (Page::Plugins, icon::PUZZLE, "Plugins"),
    (Page::Gallery, icon::SHOP, "Galeria"),
];
const SPEEDS: [&str; 4] = ["Devagar", "Normal", "Rápido", "Muito rápido"];

/// Medidas em pontos de 96 DPI. A área de conteúdo usa coordenadas próprias
/// (x a partir da barra lateral, y com `SHIFT` de folga para o título da página).
const SIDEBAR: i32 = 200;
const CONTENT: i32 = 580;
const HEIGHT: i32 = 692;
const SHIFT: i32 = 44;
const FOOTER: i32 = 688;
/// Margem dos cartões: horizontal (a partir da barra) e em volta do conteúdo.
const CARD_X: i32 = 20;
const CARD_PAD: i32 = 12;
const NAV_TOP: i32 = 176;
const NAV_STEP: i32 = 44;

const IDC_MODS: i32 = 106;
const IDC_BIRTHDAY: i32 = 107;
const IDC_UPDATES: i32 = 108;
const IDC_MEETINGS: i32 = 109;
const IDC_MEMORY: i32 = 130;
const IDC_MEMORY_STATUS: i32 = 131;
const IDC_MEMORY_OPEN: i32 = 132;
const IDC_MEMORY_CLEAR: i32 = 133;
const IDC_BUDDY: i32 = 135;
const IDC_THEME: i32 = 136;
const IDC_LANGUAGE: i32 = 137;
const IDC_GALLERY: i32 = 170;
const IDC_GALLERY_INFO: i32 = 171;
const IDC_GALLERY_INSTALL: i32 = 172;
const IDC_GALLERY_RELOAD: i32 = 173;
const IDC_GALLERY_STATUS: i32 = 174;
const IDC_ABOUT_PROJECTS: i32 = 180;
const IDC_ABOUT_GITHUB: i32 = 181;
const IDC_ABOUT_UPDATE: i32 = 182;
const IDC_ABOUT_STATUS: i32 = 183;
const IDC_WATER_GOAL: i32 = 116;
const IDC_FOCUS: i32 = 117;
const IDC_BREAK: i32 = 118;
const IDC_FOCUS_QUIET: i32 = 119;
const IDC_HOTKEY: i32 = 134;
const IDC_ACCESSORIES: i32 = 138;
const IDC_MASCOT: i32 = 101;
const IDC_SIZE: i32 = 102;
const IDC_SPEED: i32 = 103;
const IDC_AWAY: i32 = 104;
const IDC_AUTOSTART: i32 = 105;
const IDC_LIST: i32 = 110;
const IDC_TEXT: i32 = 111;
const IDC_MINUTES: i32 = 112;
const IDC_ADD: i32 = 113;
const IDC_UPDATE: i32 = 114;
const IDC_REMOVE: i32 = 115;
const IDC_PROVIDER: i32 = 120;
const IDC_BASE: i32 = 121;
const IDC_MODEL: i32 = 122;
const IDC_KEYENV: i32 = 123;
const IDC_KEY: i32 = 124;
const IDC_KEY_REMOVE: i32 = 125;
const IDC_KEY_STATUS: i32 = 126;
const IDC_GETKEY: i32 = 127;
const IDC_TEST: i32 = 128;
const IDC_TEST_RESULT: i32 = 129;
const IDC_CANVAS: i32 = 140;
const IDC_PALETTE: i32 = 141;
const IDC_NAME: i32 = 142;
const IDC_ABOUT: i32 = 143;
const IDC_PRONOUN: i32 = 144;
const IDC_TEMPLATE: i32 = 145;
const IDC_CLEAR: i32 = 146;
const IDC_MIRROR: i32 = 147;
const IDC_SAVE_MASCOT: i32 = 148;
const IDC_AI_TEXT: i32 = 149;
const IDC_AI_GO: i32 = 150;
const IDC_AI_STATUS: i32 = 151;
const IDC_POSE: i32 = 152;
const IDC_PLUGINS: i32 = 160;
const IDC_PLUGIN_INFO: i32 = 161;
const IDC_PLUGIN_TEST: i32 = 162;
const IDC_PLUGIN_FOLDER: i32 = 163;
const IDC_PLUGIN_RELOAD: i32 = 164;
const IDC_PLUGIN_RESULT: i32 = 165;
/// Títulos de seção e textos de ajuda (várias janelas com o mesmo id, só para a cor).
const IDC_SECTION: i32 = 198;
const IDC_HINT: i32 = 199;
const IDOK: i32 = 1;
const IDCANCEL: i32 = 2;

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

/// 0xRRGGBB → COLORREF (0x00BBGGRR).
fn colorref(c: u32) -> COLORREF {
    rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

thread_local! {
    static OPEN: Cell<HWND> = const { Cell::new(null_mut()) };
}

#[derive(Clone, Copy, PartialEq)]
enum Busy {
    Test,
    Draw,
    Plugin,
    GalleryList,
    GalleryInstall,
}

struct State {
    owner: HWND,
    draft: Config,
    chat: ChatSettings,
    packs: Vec<PackInfo>,
    font: HFONT,
    bold: HFONT,
    small: HFONT,
    title: HFONT,
    icons: HFONT,
    /// Fundo dos rótulos (branco, como os cartões).
    card_brush: HBRUSH,
    dpi: u32,
    page: usize,
    nav_hover: Option<usize>,
    /// Cartões brancos de cada página (coordenadas de layout).
    cards: Vec<(usize, RECT)>,
    /// Mascote da barra lateral: (nome, sprite parado).
    preview: (String, Vec<u32>),
    /// Preenchendo a lista: ignora as notificações de mudança.
    loading: bool,
    /// (aba, controle) — para mostrar só os controles da aba atual.
    pages: Vec<(usize, HWND)>,
    drawing: Drawing,
    /// Cor selecionada na paleta (0 = borracha).
    color: u8,
    /// Pintando com o mouse apertado (valor sendo aplicado).
    painting: Option<u8>,
    /// Pose sendo desenhada no editor.
    pose: Pose,
    busy: Option<Busy>,
    plugins: Vec<Plugin>,
    /// (linha, ligado?) marcado na lista de plugins, esperando ser tratado.
    plugin_toggle: Option<(usize, bool)>,
    /// Itens da galeria (carregados na primeira vez que a página abre).
    gallery: Vec<gallery::Entry>,
    gallery_loaded: bool,
    /// Estado dos interruptores (id do controle, ligado?).
    toggles: Vec<(i32, bool)>,
}

/// Para Tab/Enter/Esc funcionarem como numa caixa de diálogo.
pub fn is_dialog_message(msg: &MSG) -> bool {
    let hwnd = OPEN.with(Cell::get);
    if hwnd.is_null() {
        return false;
    }
    unsafe {
        // Enter num campo de texto aciona o botão ao lado, não o OK.
        if msg.message == WM_KEYDOWN && msg.wParam as u16 == VK_RETURN {
            let target = match GetDlgCtrlID(msg.hwnd) {
                IDC_AI_TEXT => Some(IDC_AI_GO),
                IDC_TEXT | IDC_MINUTES => Some(IDC_ADD),
                _ => None,
            };
            if let Some(id) = target.filter(|_| GetParent(msg.hwnd) == hwnd) {
                SendMessageW(hwnd, WM_COMMAND, id as WPARAM, 0);
                return true;
            }
        }
        IsDialogMessageW(hwnd, msg) != 0
    }
}

pub unsafe fn open(owner: HWND, config: &Config, small_icon: HICON, page: Page) {
    let existing = OPEN.with(Cell::get);
    if !existing.is_null() {
        ShowWindow(existing, SW_RESTORE);
        show_page(existing, page as usize);
        SetForegroundWindow(existing);
        return;
    }
    let icc = INITCOMMONCONTROLSEX { dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_LISTVIEW_CLASSES };
    InitCommonControlsEx(&icc);

    let hinstance = GetModuleHandleW(null());
    register(hinstance, "StayAloneSettings", proc, small_icon, 0);
    register(hinstance, "StayAlonePixels", canvas_proc, null_mut(), 0);
    register(hinstance, "StayAlonePalette", palette_proc, null_mut(), CS_DBLCLKS);

    let dpi = GetDpiForSystem();
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
    let mut rect = RECT { left: 0, top: 0, right: scale(SIDEBAR + CONTENT, dpi), bottom: scale(HEIGHT, dpi) };
    AdjustWindowRectExForDpi(&mut rect, style, 0, 0, dpi);
    let (ww, wh) = (rect.right - rect.left, rect.bottom - rect.top);
    let mut work: RECT = zeroed();
    SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut work as *mut RECT).cast(), 0);
    let x = work.left + (work.right - work.left - ww) / 2;
    let y = (work.top + (work.bottom - work.top - wh) / 2).max(work.top);

    let hwnd = CreateWindowExW(
        0,
        w("StayAloneSettings").as_ptr(),
        w(tr("Configurações — !StayAlone")).as_ptr(),
        style,
        x,
        y,
        ww,
        wh,
        null_mut(),
        null_mut(),
        hinstance,
        null(),
    );
    if hwnd.is_null() {
        return;
    }
    win::caption_color(hwnd, theme::bg());
    win::dark_title(hwnd, theme::is_dark());
    let font = |size: i32, weight: u32| ui_font(scale(size, dpi), weight);
    let packs = pack::list();
    let preview = preview_of(packs.iter().find(|p| p.id == config.mascot));
    let state = Box::new(State {
        owner,
        draft: config.clone(),
        chat: ChatSettings::load(),
        packs,
        font: font(15, FW_NORMAL),
        bold: font(15, FW_SEMIBOLD),
        small: font(12, FW_NORMAL),
        title: font(24, FW_SEMIBOLD),
        icons: theme::icon_font(scale(16, dpi)),
        card_brush: CreateSolidBrush(theme::colorref(theme::card())),
        dpi,
        page: page as usize,
        nav_hover: None,
        cards: Vec::new(),
        preview,
        loading: false,
        pages: Vec::new(),
        drawing: Drawing::blank(),
        color: 1,
        painting: None,
        pose: Pose::Idle,
        busy: None,
        plugins: plugins::list(),
        plugin_toggle: None,
        gallery: Vec::new(),
        gallery_loaded: false,
        toggles: TOGGLES.iter().map(|&id| (id, false)).collect(),
    });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
    OPEN.with(|o| o.set(hwnd));
    build(hwnd);
    populate(hwnd);
    show_page(hwnd, page as usize);
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
}

unsafe fn register(hinstance: HINSTANCE, class: &str, proc: WndProcFn, icon: HICON, style: u32) {
    let name = w(class);
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style,
        lpfnWndProc: Some(proc),
        hInstance: hinstance,
        hCursor: LoadCursorW(null_mut(), if class == "StayAloneSettings" { IDC_ARROW } else { IDC_HAND }),
        hbrBackground: if class == "StayAloneSettings" { null_mut() } else { GetSysColorBrush(COLOR_WINDOW) },
        lpszClassName: name.as_ptr(),
        hIcon: icon,
        hIconSm: icon,
        ..zeroed()
    };
    RegisterClassExW(&wc);
}

type WndProcFn = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

fn scale(v: i32, dpi: u32) -> i32 {
    v * dpi as i32 / 96
}

unsafe fn state<'a>(hwnd: HWND) -> Option<&'a mut State> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State).as_mut()
}

unsafe fn item(hwnd: HWND, id: i32) -> HWND {
    GetDlgItem(hwnd, id)
}

unsafe fn set_text(hwnd: HWND, id: i32, text: &str) {
    SetWindowTextW(item(hwnd, id), w(text).as_ptr());
}

/// Limita quanto se pode digitar num campo.
unsafe fn limit(control: HWND, chars: usize) {
    SendMessageW(control, EM_LIMITTEXT, chars, 0);
}

unsafe fn info(hwnd: HWND, text: &str) {
    MessageBoxW(hwnd, w(text).as_ptr(), w("!StayAlone").as_ptr(), MB_ICONINFORMATION);
}

// --- montagem ---------------------------------------------------------------

unsafe fn build(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let dpi = st.dpi;
    let s = |v: i32| scale(v, dpi);
    let (font, bold) = (st.font, st.bold);
    let pages = &mut st.pages;
    let cards = std::cell::RefCell::new(Vec::<(usize, RECT)>::new());
    // Coordenadas de layout: x dentro da área de conteúdo (à direita da barra lateral),
    // y com `SHIFT` de folga. Cada controle de uma página estica o cartão atual dela.
    let mut add = |page: Option<usize>, class: &str, text: &str, style: u32, ex: u32, rect: (i32, i32, i32, i32), id: i32| {
        let (x, y, cw, ch) = rect;
        let control = CreateWindowExW(
            ex,
            w(class).as_ptr(),
            w(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            s(SIDEBAR + x),
            s(y - SHIFT),
            s(cw),
            s(ch),
            hwnd,
            id as isize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(control, WM_SETFONT, if id == IDC_SECTION { bold } else { font } as WPARAM, 1);
        ui::theme_control(control, class);
        if let Some(p) = page {
            pages.push((p, control));
            // Combos informam a altura da lista aberta; na tela ocupam uma linha.
            let visible = if class == "COMBOBOX" { 26 } else { ch };
            if let Some((_, card)) = cards.borrow_mut().iter_mut().rev().find(|(cp, _)| *cp == p) {
                card.bottom = card.bottom.max(y + visible + CARD_PAD);
            }
        }
        control
    };

    let x0 = 36;
    let (cx, cw) = (210, 300);
    let combo = CBS_DROPDOWNLIST as u32 | WS_VSCROLL | WS_TABSTOP;
    let edit = ES_AUTOHSCROLL as u32 | WS_TABSTOP;
    let number = ES_NUMBER as u32 | WS_TABSTOP;
    let button = BS_OWNERDRAW as u32 | WS_TABSTOP;

    macro_rules! card {
        ($p:expr, $y:expr) => {
            cards.borrow_mut().push(($p, RECT { left: CARD_X, top: $y - CARD_PAD, right: CONTENT - CARD_X, bottom: $y }))
        };
    }
    macro_rules! section {
        ($p:expr, $text:expr, $y:expr) => {{
            card!($p, $y);
            add(Some($p), "STATIC", $text, 0, 0, (x0, $y, CONTENT - 2 * x0, 22), IDC_SECTION)
        }};
    }
    macro_rules! label {
        ($p:expr, $text:expr, $x:expr, $y:expr, $w:expr) => {
            add(Some($p), "STATIC", $text, 0, 0, ($x, $y + 3, $w, 20), 0)
        };
    }
    macro_rules! hint {
        ($p:expr, $text:expr, $y:expr, $h:expr) => {
            add(Some($p), "STATIC", $text, 0, 0, (x0, $y, CONTENT - 2 * x0, $h), IDC_HINT)
        };
    }

    // --- Geral
    section!(0, tr("Seu mascote"), 116);
    label!(0, tr("Mascote"), x0, 146, 170);
    add(Some(0), "COMBOBOX", "", combo, 0, (cx, 144, 196, 300), IDC_MASCOT);
    add(Some(0), "BUTTON", tr("Abrir pasta"), button, 0, (cx + 204, 142, 96, 28), IDC_MODS);
    label!(0, tr("Amigo na tela"), x0, 180, 170);
    add(Some(0), "COMBOBOX", "", combo, 0, (cx, 178, cw, 300), IDC_BUDDY);
    label!(0, tr("Tamanho"), x0, 214, 170);
    add(Some(0), "COMBOBOX", "", combo, 0, (cx, 212, cw, 200), IDC_SIZE);
    label!(0, tr("Velocidade ao andar"), x0, 248, 170);
    add(Some(0), "COMBOBOX", "", combo, 0, (cx, 246, cw, 200), IDC_SPEED);
    section!(0, tr("Comportamento"), 304);
    label!(0, tr("Considerar ausente após"), x0, 334, 170);
    add(Some(0), "EDIT", "", number, WS_EX_CLIENTEDGE, (cx, 332, 50, 24), IDC_AWAY);
    label!(0, tr("min sem usar o PC"), cx + 58, 334, 200);
    label!(0, tr("Seu aniversário"), x0, 368, 170);
    let birthday = add(Some(0), "EDIT", "", edit, WS_EX_CLIENTEDGE, (cx, 366, 70, 24), IDC_BIRTHDAY);
    SendMessageW(birthday, EM_SETCUEBANNER, 1, w("dd/mm").as_ptr() as LPARAM);
    limit(birthday, 5);
    label!(0, tr("o mascote comemora com você"), cx + 78, 368, 230);
    let check = BS_OWNERDRAW as u32 | WS_TABSTOP; // interruptor desenhado aqui
    add(Some(0), "BUTTON", tr("Iniciar junto com o Windows"), check, 0, (x0, 402, 400, 22), IDC_AUTOSTART);
    add(Some(0), "BUTTON", tr("Procurar versões novas (uma vez por dia, no GitHub)"), check, 0, (x0, 430, 440, 22), IDC_UPDATES);
    add(Some(0), "BUTTON", tr("Ficar quieto em reuniões (Teams, Zoom, Webex...)"), check, 0, (x0, 458, 440, 22), IDC_MEETINGS);
    hint!(0, tr("Ele olha só o nome do programa aberto, nunca o que está na tela."), 484, 20);
    section!(0, tr("Aparência"), 532);
    label!(0, tr("Tema"), x0, 562, 170);
    add(Some(0), "COMBOBOX", "", combo, 0, (cx, 560, cw, 200), IDC_THEME);
    label!(0, tr("Idioma"), x0, 596, 170);
    add(Some(0), "COMBOBOX", "", combo, 0, (cx, 594, cw, 200), IDC_LANGUAGE);
    add(Some(0), "BUTTON", tr("Acessórios de época (Natal, Halloween, aniversário...)"), check, 0, (x0, 630, 460, 22), IDC_ACCESSORIES);

    // --- Lembretes
    section!(1, tr("Seus lembretes"), 116);
    hint!(1, tr("Marque para ativar. Eles contam só o tempo em que você está usando o PC."), 140, 20);
    let list_style = LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS | LVS_NOSORTHEADER | WS_TABSTOP;
    let list = add(Some(1), "SysListView32", "", list_style, WS_EX_CLIENTEDGE, (x0, 166, CONTENT - 2 * x0, 200), IDC_LIST);
    let ex = LVS_EX_CHECKBOXES | LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER;
    SendMessageW(list, LVM_SETEXTENDEDLISTVIEWSTYLE, ex as WPARAM, ex as LPARAM);
    add_columns(list, &[(tr("Lembrete"), CONTENT - 2 * x0 - 110), (tr("A cada"), 86)], dpi);
    label!(1, tr("Texto"), x0, 382, 48);
    let text = add(Some(1), "EDIT", "", edit, WS_EX_CLIENTEDGE, (x0 + 50, 380, 256, 24), IDC_TEXT);
    SendMessageW(text, EM_SETCUEBANNER, 1, w(tr("Ex.: Conferir o e-mail")).as_ptr() as LPARAM);
    limit(text, MAX_REMINDER_TEXT);
    label!(1, tr("a cada"), x0 + 316, 382, 46);
    add(Some(1), "EDIT", "30", number, WS_EX_CLIENTEDGE, (x0 + 364, 380, 48, 24), IDC_MINUTES);
    label!(1, "min", x0 + 418, 382, 40);
    add(Some(1), "BUTTON", tr("Adicionar"), button, 0, (x0, 418, 110, 30), IDC_ADD);
    add(Some(1), "BUTTON", tr("Salvar alteração"), button, 0, (x0 + 118, 418, 140, 30), IDC_UPDATE);
    add(Some(1), "BUTTON", tr("Remover"), button, 0, (x0 + 266, 418, 100, 30), IDC_REMOVE);
    section!(1, tr("Água e foco"), 484);
    label!(1, tr("Meta de água"), x0, 514, 170);
    let goal = add(Some(1), "EDIT", "", number, WS_EX_CLIENTEDGE, (cx, 512, 50, 24), IDC_WATER_GOAL);
    limit(goal, 2);
    label!(1, tr("copos por dia (0 = sem meta)"), cx + 58, 514, 240);
    label!(1, tr("Foco (pomodoro)"), x0, 548, 170);
    let focus = add(Some(1), "EDIT", "", number, WS_EX_CLIENTEDGE, (cx, 546, 50, 24), IDC_FOCUS);
    limit(focus, 3);
    label!(1, tr("min, pausa de"), cx + 58, 548, 100);
    let pause = add(Some(1), "EDIT", "", number, WS_EX_CLIENTEDGE, (cx + 160, 546, 50, 24), IDC_BREAK);
    limit(pause, 2);
    label!(1, "min", cx + 218, 548, 60);
    let check = BS_OWNERDRAW as u32 | WS_TABSTOP; // interruptor desenhado aqui
    add(Some(1), "BUTTON", tr("Durante o foco, os lembretes esperam a pausa"), check, 0, (x0, 582, 460, 22), IDC_FOCUS_QUIET);

    // --- Conversa
    section!(2, tr("Conversar com o mascote"), 116);
    hint!(2, tr("Serviços no padrão da API da OpenAI. Sem chave, nada sai do seu PC."), 140, 20);
    label!(2, tr("Serviço"), x0, 172, 170);
    add(Some(2), "COMBOBOX", "", combo, 0, (cx, 170, cw, 240), IDC_PROVIDER);
    label!(2, tr("Endereço da API"), x0, 206, 170);
    let base = add(Some(2), "EDIT", "", edit, WS_EX_CLIENTEDGE, (cx, 204, cw, 24), IDC_BASE);
    limit(base, 300);
    label!(2, tr("Modelo"), x0, 240, 170);
    let model = add(Some(2), "EDIT", "", edit, WS_EX_CLIENTEDGE, (cx, 238, cw, 24), IDC_MODEL);
    limit(model, 100);
    label!(2, tr("Nome da chave"), x0, 274, 170);
    let key_name = add(Some(2), "EDIT", "", edit, WS_EX_CLIENTEDGE, (cx, 272, cw, 24), IDC_KEYENV);
    limit(key_name, 64);
    let check = BS_OWNERDRAW as u32 | WS_TABSTOP; // interruptor desenhado aqui
    add(Some(2), "BUTTON", tr("Ctrl+Alt+M abre a conversa de qualquer lugar"), check, 0, (x0, 308, 460, 22), IDC_HOTKEY);
    section!(2, tr("Chave da API"), 364);
    label!(2, tr("Colar a chave"), x0, 392, 170);
    let key = add(Some(2), "EDIT", "", edit | ES_PASSWORD as u32, WS_EX_CLIENTEDGE, (cx, 390, 180, 24), IDC_KEY);
    SendMessageW(key, EM_SETCUEBANNER, 1, w(tr("cole aqui para salvar")).as_ptr() as LPARAM);
    limit(key, 512);
    add(Some(2), "BUTTON", tr("Remover chave"), button, 0, (cx + 188, 388, 112, 28), IDC_KEY_REMOVE);
    add(Some(2), "STATIC", "", 0, 0, (cx, 422, 180, 20), IDC_KEY_STATUS);
    add(Some(2), "BUTTON", tr("Criar chave"), button, 0, (cx + 188, 418, 112, 28), IDC_GETKEY);
    hint!(
        2,
        tr("Fica no Gerenciador de Credenciais do Windows, protegida pela sua conta, e só vai por HTTPS ao serviço escolhido."),
        452,
        40
    );
    add(Some(2), "BUTTON", tr("Testar conversa"), button, 0, (x0, 498, 140, 30), IDC_TEST);
    add(Some(2), "STATIC", "", 0, 0, (x0 + 150, 496, CONTENT - 2 * x0 - 150, 40), IDC_TEST_RESULT);
    section!(2, tr("Memória"), 566);
    add(Some(2), "BUTTON", tr("Lembrar do que eu contar na conversa (fica só neste PC)"), check, 0, (x0, 592, 460, 22), IDC_MEMORY);
    add(Some(2), "STATIC", "", 0, 0, (x0, 624, 200, 20), IDC_MEMORY_STATUS);
    add(Some(2), "BUTTON", tr("Ver e editar"), button, 0, (x0 + 206, 618, 118, 30), IDC_MEMORY_OPEN);
    add(Some(2), "BUTTON", tr("Esquecer tudo"), button, 0, (x0 + 332, 618, 124, 30), IDC_MEMORY_CLEAR);

    // --- Criar mascote
    card!(3, 118);
    let grid = SPRITE as i32 * CELL;
    add(Some(3), "StayAlonePixels", "", 0, 0, (x0, 118, grid, grid), IDC_CANVAS);
    let palette_w = 8 * (SWATCH + SWATCH_GAP);
    add(Some(3), "StayAlonePalette", "", 0, 0, (x0, 118 + grid + 10, palette_w, 2 * (SWATCH + SWATCH_GAP)), IDC_PALETTE);
    add(
        Some(3),
        "STATIC",
        tr("Clique pinta • botão direito apaga • duplo clique numa cor troca a cor. A cor com ponto branco são os olhos."),
        0,
        0,
        (x0, 118 + grid + 10 + 2 * (SWATCH + SWATCH_GAP) + 2, grid, 62),
        IDC_HINT,
    );
    let xr = x0 + grid + 20;
    let rw = CONTENT - x0 - xr;
    label!(3, tr("Nome"), xr, 116, rw);
    let name = add(Some(3), "EDIT", "", edit, WS_EX_CLIENTEDGE, (xr, 138, rw, 24), IDC_NAME);
    limit(name, 32);
    label!(3, tr("Quem é (personalidade)"), xr, 170, rw);
    let about = add(Some(3), "EDIT", "", edit, WS_EX_CLIENTEDGE, (xr, 192, rw, 24), IDC_ABOUT);
    SendMessageW(about, EM_SETCUEBANNER, 1, w(tr("Ex.: um polvo roxo curioso")).as_ptr() as LPARAM);
    limit(about, 200);
    label!(3, tr("É"), xr, 224, rw);
    add(Some(3), "COMBOBOX", "", combo, 0, (xr, 246, rw, 120), IDC_PRONOUN);
    label!(3, tr("Começar a partir de"), xr, 278, rw);
    add(Some(3), "COMBOBOX", "", combo, 0, (xr, 300, rw, 300), IDC_TEMPLATE);
    let half = (rw - 8) / 2;
    add(Some(3), "BUTTON", tr("Limpar"), button, 0, (xr, 338, half, 30), IDC_CLEAR);
    add(Some(3), "BUTTON", tr("Espelhar"), button, 0, (xr + half + 8, 338, half, 30), IDC_MIRROR);
    add(Some(3), "BUTTON", tr("Salvar e usar"), button, 0, (xr, 376, rw, 34), IDC_SAVE_MASCOT);
    label!(3, tr("Pose que você está desenhando"), xr, 418, rw);
    add(Some(3), "COMBOBOX", "", combo, 0, (xr, 440, rw, 200), IDC_POSE);
    add(
        Some(3),
        "STATIC",
        tr("Poses em branco são criadas sozinhas a partir da pose parada (piscar, dormir, andar, pular)."),
        0,
        0,
        (xr, 474, rw, 60),
        IDC_HINT,
    );
    section!(3, tr("Ou peça para a IA desenhar"), 552);
    let ai = add(Some(3), "EDIT", "", edit, WS_EX_CLIENTEDGE, (x0, 578, 372, 26), IDC_AI_TEXT);
    SendMessageW(ai, EM_SETCUEBANNER, 1, w(tr("Ex.: um polvo roxo de chapéu de marinheiro")).as_ptr() as LPARAM);
    limit(ai, 300);
    add(Some(3), "BUTTON", tr("Criar com IA"), button, 0, (x0 + 380, 576, CONTENT - 2 * x0 - 380, 30), IDC_AI_GO);
    add(Some(3), "STATIC", "", 0, 0, (x0, 610, CONTENT - 2 * x0, 20), IDC_AI_STATUS);

    // --- Plugins
    section!(4, tr("Plugins instalados"), 116);
    hint!(
        4,
        tr("Programas que dão novos poderes ao mascote. Plugins novos chegam desligados: ligue só os de quem você confia."),
        140,
        40
    );
    let plugin_list = add(Some(4), "SysListView32", "", list_style, WS_EX_CLIENTEDGE, (x0, 188, CONTENT - 2 * x0, 190), IDC_PLUGINS);
    SendMessageW(plugin_list, LVM_SETEXTENDEDLISTVIEWSTYLE, ex as WPARAM, ex as LPARAM);
    add_columns(plugin_list, &[("Plugin", CONTENT - 2 * x0 - 236), (tr("Tipo"), 86), (tr("Situação"), 146)], dpi);
    add(Some(4), "STATIC", "", 0, 0, (x0, 388, CONTENT - 2 * x0, 58), IDC_PLUGIN_INFO);
    add(Some(4), "BUTTON", tr("Testar"), button, 0, (x0, 454, 100, 30), IDC_PLUGIN_TEST);
    add(Some(4), "BUTTON", tr("Abrir pasta de plugins"), button, 0, (x0 + 108, 454, 180, 30), IDC_PLUGIN_FOLDER);
    add(Some(4), "BUTTON", tr("Atualizar lista"), button, 0, (x0 + 296, 454, 130, 30), IDC_PLUGIN_RELOAD);
    add(Some(4), "STATIC", "", 0, 0, (x0, 494, CONTENT - 2 * x0, 44), IDC_PLUGIN_RESULT);
    hint!(
        4,
        tr("Crie o seu com um .exe ou um script PowerShell: veja o LEIA-ME.txt e o exemplo \"Curiosidades\" na pasta de plugins."),
        548,
        40
    );

    // --- Galeria
    section!(5, tr("Mascotes da comunidade"), 116);
    hint!(
        5,
        tr("Feitos pela comunidade: só desenho e falas, nada que rode no seu PC. Conferidos (SHA-256) antes de instalar."),
        140,
        40
    );
    let gallery_list = add(Some(5), "SysListView32", "", list_style, WS_EX_CLIENTEDGE, (x0, 188, CONTENT - 2 * x0, 214), IDC_GALLERY);
    SendMessageW(gallery_list, LVM_SETEXTENDEDLISTVIEWSTYLE, (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as WPARAM, (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as LPARAM);
    add_columns(gallery_list, &[(tr("Nome"), CONTENT - 2 * x0 - 150), (tr("Autor"), 146)], dpi);
    add(Some(5), "STATIC", "", 0, 0, (x0, 412, CONTENT - 2 * x0, 40), IDC_GALLERY_INFO);
    add(Some(5), "BUTTON", tr("Instalar"), button, 0, (x0, 460, 110, 30), IDC_GALLERY_INSTALL);
    add(Some(5), "BUTTON", tr("Atualizar lista"), button, 0, (x0 + 118, 460, 140, 30), IDC_GALLERY_RELOAD);
    add(Some(5), "STATIC", "", 0, 0, (x0, 500, CONTENT - 2 * x0, 40), IDC_GALLERY_STATUS);

    // --- Sobre
    section!(6, "!StayAlone", 116);
    hint!(
        6,
        tr("Um mascote em pixel art que faz companhia na área de trabalho: lembra de beber água e fazer pausas, conversa com você e fica levinho, feito direto na API do Windows."),
        142,
        60
    );
    let version = crate::update::current();
    let date = release_date();
    for (i, (name, value)) in
        [(tr("Versão"), version), (tr("Lançada em"), date.as_str()), (tr("Autor"), "Atylla Azevedo"), (tr("Licença"), "MIT")]
            .into_iter()
            .enumerate()
    {
        let y = 214 + i as i32 * 30;
        label!(6, name, x0, y, 170);
        label!(6, value, cx, y, 280);
    }
    add(Some(6), "BUTTON", tr("Meus projetos"), button, 0, (x0, 344, 150, 32), IDC_ABOUT_PROJECTS);
    add(Some(6), "BUTTON", tr("Página no GitHub"), button, 0, (x0 + 158, 344, 170, 32), IDC_ABOUT_GITHUB);
    add(Some(6), "BUTTON", tr("Procurar atualização"), button, 0, (x0 + 336, 344, 172, 32), IDC_ABOUT_UPDATE);
    add(Some(6), "STATIC", "", 0, 0, (x0, 384, CONTENT - 2 * x0, 20), IDC_ABOUT_STATUS);

    // --- Rodapé (fora dos cartões)
    add(None, "BUTTON", tr("Salvar"), button, 0, (CONTENT - 20 - 216, FOOTER, 104, 32), IDOK);
    add(None, "BUTTON", tr("Cancelar"), button, 0, (CONTENT - 20 - 104, FOOTER, 104, 32), IDCANCEL);
    st.cards = cards.into_inner();
}

unsafe fn add_columns(list: HWND, columns: &[(&str, i32)], dpi: u32) {
    for (i, &(title, width)) in columns.iter().enumerate() {
        let mut text = w(title);
        let mut column: LVCOLUMNW = zeroed();
        column.mask = LVCF_TEXT | LVCF_WIDTH;
        column.cx = scale(width, dpi);
        column.pszText = text.as_mut_ptr();
        SendMessageW(list, LVM_INSERTCOLUMNW, i, &column as *const _ as LPARAM);
    }
}

/// Acrescenta a linha `i` com um texto por coluna.
unsafe fn insert_row(list: HWND, i: usize, cells: &[&str]) {
    let mut row: LVITEMW = zeroed();
    row.mask = LVIF_TEXT;
    row.iItem = i as i32;
    for (column, cell) in cells.iter().enumerate() {
        let mut text = w(cell);
        row.iSubItem = column as i32;
        row.pszText = text.as_mut_ptr();
        let message = if column == 0 { LVM_INSERTITEMW } else { LVM_SETITEMTEXTW };
        SendMessageW(list, message, if column == 0 { 0 } else { i }, &row as *const _ as LPARAM);
    }
}

/// Marca (ou desmarca) a caixinha da linha `i`.
unsafe fn set_checked(list: HWND, i: usize, on: bool) {
    set_state(list, i, LVIS_STATEIMAGEMASK, if on { 2 << 12 } else { 1 << 12 });
}

/// A caixinha da linha mudou? Devolve o novo valor.
fn check_toggled(nm: &NMLISTVIEW) -> Option<bool> {
    let (old, new) = (nm.uOldState, nm.uNewState);
    let toggled = nm.uChanged & LVIF_STATE != 0 && (old ^ new) & LVIS_STATEIMAGEMASK != 0 && old & LVIS_STATEIMAGEMASK != 0;
    toggled.then_some((new & LVIS_STATEIMAGEMASK) >> 12 == 2)
}

unsafe fn show_page(hwnd: HWND, page: usize) {
    let Some(st) = state(hwnd) else { return };
    st.page = page.min(ABOUT);
    for &(p, control) in &st.pages {
        ShowWindow(control, if p == st.page { SW_SHOW } else { SW_HIDE });
    }
    InvalidateRect(hwnd, null(), 0);
    if st.page == Page::Gallery as usize && !st.gallery_loaded && st.busy.is_none() {
        load_gallery(hwnd);
    }
}

// --- galeria ----------------------------------------------------------------------

unsafe fn load_gallery(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    if st.busy.is_some() {
        return;
    }
    st.busy = Some(Busy::GalleryList);
    set_text(hwnd, IDC_GALLERY_STATUS, tr("Buscando a galeria..."));
    gallery::list(hwnd, WM_CHAT_REPLY);
}

unsafe fn fill_gallery(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let list = item(hwnd, IDC_GALLERY);
    SendMessageW(list, LVM_DELETEALLITEMS, 0, 0);
    for (i, e) in st.gallery.iter().enumerate() {
        insert_row(list, i, &[&e.name, &e.author]);
    }
    show_gallery_item(hwnd);
}

fn selected_row(hwnd: HWND, id: i32) -> Option<usize> {
    let i = unsafe { SendMessageW(item(hwnd, id), LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as LPARAM) };
    (i >= 0).then_some(i as usize)
}

unsafe fn show_gallery_item(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let entry = selected_row(hwnd, IDC_GALLERY).and_then(|i| st.gallery.get(i));
    set_text(hwnd, IDC_GALLERY_INFO, entry.map_or(tr("Selecione um item para ver o que ele faz."), |e| e.about.as_str()));
    EnableWindow(item(hwnd, IDC_GALLERY_INSTALL), (entry.is_some() && st.busy.is_none()) as BOOL);
}

unsafe fn on_gallery_command(hwnd: HWND, id: i32) {
    let Some(st) = state(hwnd) else { return };
    match id {
        IDC_GALLERY_RELOAD => load_gallery(hwnd),
        IDC_GALLERY_INSTALL => {
            let Some(entry) = selected_row(hwnd, IDC_GALLERY).and_then(|i| st.gallery.get(i).cloned()) else { return };
            if st.busy.is_some() {
                return;
            }
            st.busy = Some(Busy::GalleryInstall);
            set_text(hwnd, IDC_GALLERY_STATUS, &fill(tr("Instalando \"{}\"..."), &[&entry.name]));
            show_gallery_item(hwnd);
            gallery::install_in_child(hwnd, WM_CHAT_REPLY, &entry.id);
        }
        _ => {}
    }
}

unsafe fn fill_combo(hwnd: HWND, id: i32, labels: &[String], selected: usize) {
    let combo = item(hwnd, id);
    SendMessageW(combo, CB_RESETCONTENT, 0, 0);
    for label in labels {
        SendMessageW(combo, CB_ADDSTRING, 0, w(label).as_ptr() as LPARAM);
    }
    SendMessageW(combo, CB_SETCURSEL, selected, 0);
}

unsafe fn combo_index(hwnd: HWND, id: i32) -> usize {
    SendMessageW(item(hwnd, id), CB_GETCURSEL, 0, 0).max(0) as usize
}

unsafe fn populate(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    fill_mascot_combos(hwnd);
    let sizes: Vec<String> = Size::ALL.iter().map(|s| s.label().to_string()).collect();
    fill_combo(hwnd, IDC_SIZE, &sizes, Size::ALL.iter().position(|s| *s == st.draft.size).unwrap_or(2));
    let speeds: Vec<String> = SPEEDS.iter().map(|s| tr(s).to_string()).collect();
    fill_combo(hwnd, IDC_SPEED, &speeds, (st.draft.speed.clamp(1, 4) - 1) as usize);
    set_text(hwnd, IDC_AWAY, &st.draft.companion.away_minutes.to_string());
    set_checked_box(hwnd, IDC_AUTOSTART, config::autostart_enabled());
    set_checked_box(hwnd, IDC_UPDATES, st.draft.updates);
    set_checked_box(hwnd, IDC_MEETINGS, st.draft.quiet_in_meetings);
    set_checked_box(hwnd, IDC_MEMORY, st.draft.memory);
    set_checked_box(hwnd, IDC_HOTKEY, st.draft.chat_hotkey);
    set_checked_box(hwnd, IDC_ACCESSORIES, st.draft.accessories);
    set_checked_box(hwnd, IDC_FOCUS_QUIET, st.draft.companion.focus_holds_reminders);
    set_text(hwnd, IDC_WATER_GOAL, &st.draft.companion.water_goal.to_string());
    set_text(hwnd, IDC_FOCUS, &st.draft.companion.focus_minutes.to_string());
    set_text(hwnd, IDC_BREAK, &st.draft.companion.break_minutes.to_string());
    let buddies: Vec<String> = std::iter::once(tr("Nenhum").to_string()).chain(st.packs.iter().map(|p| p.name.clone())).collect();
    let buddy = st.packs.iter().position(|p| p.id == st.draft.buddy).map_or(0, |i| i + 1);
    fill_combo(hwnd, IDC_BUDDY, &buddies, buddy);
    let themes = [tr("Igual ao Windows").to_string(), tr("Claro").into(), tr("Escuro").into()];
    fill_combo(hwnd, IDC_THEME, &themes, Theme::ALL.iter().position(|t| *t == st.draft.theme).unwrap_or(0));
    let languages = [tr("Igual ao Windows").to_string(), "Português".into(), "English".into()];
    fill_combo(hwnd, IDC_LANGUAGE, &languages, Language::ALL.iter().position(|l| *l == st.draft.language).unwrap_or(0));
    show_memory_status(hwnd);
    if let Some((day, month)) = st.draft.birthday {
        set_text(hwnd, IDC_BIRTHDAY, &format!("{day:02}/{month:02}"));
    }
    fill_list(hwnd, None);

    let mut providers: Vec<String> = PROVIDERS.iter().map(|p| tr(p.0).to_string()).collect();
    providers.push(tr("Personalizado").into());
    fill_combo(hwnd, IDC_PROVIDER, &providers, st.chat.provider_index().unwrap_or(PROVIDERS.len()));
    show_chat_fields(hwnd);

    fill_combo(hwnd, IDC_PRONOUN, &[tr("Ele (o mascote)").into(), tr("Ela (a mascote)").into()], 0);
    let poses: Vec<String> = Pose::ALL.iter().map(|p| p.label().to_string()).collect();
    fill_combo(hwnd, IDC_POSE, &poses, 0);
    fill_plugins(hwnd, None);
}

/// Combos que listam mascotes (Geral e tr("Começar a partir de")).
unsafe fn fill_mascot_combos(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let names: Vec<String> = st.packs.iter().map(|p| p.name.clone()).collect();
    let current = st.packs.iter().position(|p| p.id == st.draft.mascot).unwrap_or(0);
    fill_combo(hwnd, IDC_MASCOT, &names, current);
    let templates: Vec<String> = std::iter::once(tr("Desenho em branco").to_string()).chain(names).collect();
    fill_combo(hwnd, IDC_TEMPLATE, &templates, 0);
}

// --- lembretes ----------------------------------------------------------------

unsafe fn fill_list(hwnd: HWND, select: Option<usize>) {
    let Some(st) = state(hwnd) else { return };
    let list = item(hwnd, IDC_LIST);
    st.loading = true;
    SendMessageW(list, LVM_DELETEALLITEMS, 0, 0);
    for (i, r) in st.draft.companion.reminders.iter().enumerate() {
        insert_row(list, i, &[r.kind.label(), &format!("{} min", r.minutes)]);
        set_checked(list, i, r.on);
    }
    if let Some(i) = select {
        set_state(list, i, LVIS_SELECTED | LVIS_FOCUSED, LVIS_SELECTED | LVIS_FOCUSED);
        SendMessageW(list, LVM_ENSUREVISIBLE, i, 0);
    }
    st.loading = false;
    show_selection(hwnd, select);
}

unsafe fn set_state(list: HWND, index: usize, mask: u32, state: u32) {
    let mut row: LVITEMW = zeroed();
    row.stateMask = mask;
    row.state = state;
    SendMessageW(list, LVM_SETITEMSTATE, index, &row as *const _ as LPARAM);
}

unsafe fn selected(hwnd: HWND) -> Option<usize> {
    let i = SendMessageW(item(hwnd, IDC_LIST), LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as LPARAM);
    (i >= 0).then_some(i as usize)
}

/// Mostra o lembrete selecionado nos campos de edição.
unsafe fn show_selection(hwnd: HWND, index: Option<usize>) {
    let Some(st) = state(hwnd) else { return };
    let reminder = index.and_then(|i| st.draft.companion.reminders.get(i));
    let custom = reminder.is_some_and(|r| matches!(r.kind, ReminderKind::Custom(_)));
    if let Some(r) = reminder {
        set_text(hwnd, IDC_TEXT, r.kind.label());
        set_text(hwnd, IDC_MINUTES, &r.minutes.to_string());
    }
    // O texto dos lembretes embutidos vem das falas do mascote: só o intervalo muda.
    EnableWindow(item(hwnd, IDC_TEXT), (reminder.is_none() || custom) as BOOL);
    EnableWindow(item(hwnd, IDC_UPDATE), reminder.is_some() as BOOL);
    EnableWindow(item(hwnd, IDC_REMOVE), custom as BOOL);
}

/// Lê um número dos campos; avisa e devolve `None` se estiver fora da faixa.
unsafe fn number(hwnd: HWND, id: i32, range: std::ops::RangeInclusive<u32>, what: &str) -> Option<u32> {
    let value = text_of(item(hwnd, id)).parse::<u32>().ok().filter(|v| range.contains(v));
    if value.is_none() {
        info(hwnd, &fill(tr("{}: use um número entre {} e {}."), &[&what, range.start(), range.end()]));
        SetFocus(item(hwnd, id));
    }
    value
}

unsafe fn on_reminder_command(hwnd: HWND, id: i32) {
    let Some(st) = state(hwnd) else { return };
    match id {
        IDC_ADD | IDC_UPDATE => {
            let editing = if id == IDC_UPDATE { selected(hwnd) } else { None };
            let Some(minutes) = number(hwnd, IDC_MINUTES, 1..=1440, tr("Intervalo")) else { return };
            let text = text_of(item(hwnd, IDC_TEXT));
            let builtin = editing.is_some_and(|i| !matches!(st.draft.companion.reminders[i].kind, ReminderKind::Custom(_)));
            if text.is_empty() && !builtin {
                info(hwnd, tr("Escreva o texto do lembrete."));
                SetFocus(item(hwnd, IDC_TEXT));
                return;
            }
            let reminders = &mut st.draft.companion.reminders;
            let index = match editing {
                Some(i) => {
                    reminders[i].minutes = minutes;
                    if !builtin {
                        reminders[i].kind = ReminderKind::Custom(text);
                    }
                    i
                }
                None => {
                    if reminders.len() >= ReminderKind::BUILT_IN.len() + MAX_REMINDERS {
                        return info(hwnd, &fill(tr("Dá para ter até {} lembretes seus."), &[&MAX_REMINDERS]));
                    }
                    reminders.push(Reminder { on: true, minutes, kind: ReminderKind::Custom(text) });
                    set_text(hwnd, IDC_TEXT, "");
                    reminders.len() - 1
                }
            };
            fill_list(hwnd, Some(index));
        }
        IDC_REMOVE => {
            if let Some(i) = selected(hwnd) {
                if matches!(st.draft.companion.reminders[i].kind, ReminderKind::Custom(_)) {
                    st.draft.companion.reminders.remove(i);
                    set_text(hwnd, IDC_TEXT, "");
                    fill_list(hwnd, None);
                }
            }
        }
        _ => {}
    }
}

unsafe fn on_list_change(hwnd: HWND, nm: &NMLISTVIEW) {
    let Some(st) = state(hwnd) else { return };
    if st.loading || nm.uChanged & LVIF_STATE == 0 || nm.iItem < 0 {
        return;
    }
    let i = nm.iItem as usize;
    if let (Some(on), Some(r)) = (check_toggled(nm), st.draft.companion.reminders.get_mut(i)) {
        r.on = on;
    }
    if (nm.uOldState ^ nm.uNewState) & LVIS_SELECTED != 0 {
        show_selection(hwnd, selected(hwnd));
    }
}

// --- conversa -------------------------------------------------------------------

unsafe fn show_chat_fields(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    set_text(hwnd, IDC_BASE, &st.chat.api_base);
    set_text(hwnd, IDC_MODEL, &st.chat.model);
    set_text(hwnd, IDC_KEYENV, &st.chat.key_env);
    show_key_status(hwnd);
}

unsafe fn show_key_status(hwnd: HWND) {
    let name = text_of(item(hwnd, IDC_KEYENV));
    let (text, removable) = if name.is_empty() {
        (tr("Este serviço não usa chave.").to_string(), false)
    } else if !secret::valid_name(&name) {
        (tr("Use só letras, números e _ no nome.").to_string(), false)
    } else {
        match secret::find(&name) {
            Some(KeySource::Vault) => (tr("✓ Chave salva.").to_string(), true),
            Some(KeySource::Environment) => (fill(tr("✓ Usando a variável de ambiente {}."), &[&name]), false),
            None => (tr("Nenhuma chave salva ainda.").to_string(), false),
        }
    };
    set_text(hwnd, IDC_KEY_STATUS, &text);
    EnableWindow(item(hwnd, IDC_KEY_REMOVE), removable as BOOL);
    EnableWindow(item(hwnd, IDC_KEY), (!name.is_empty()) as BOOL);
}

/// Mostra um problema num campo da aba Conversa e devolve `false`.
unsafe fn chat_field_error(hwnd: HWND, id: i32, text: &str) -> bool {
    show_page(hwnd, Page::Chat as usize);
    info(hwnd, text);
    SetFocus(item(hwnd, id));
    false
}

/// Lê os campos da aba Conversa, grava o chat.ini e a chave (se colada).
unsafe fn save_chat(hwnd: HWND) -> bool {
    let Some(st) = state(hwnd) else { return false };
    let base = text_of(item(hwnd, IDC_BASE)).trim_end_matches('/').to_string();
    if !config::endpoint_allowed(&base) {
        let text = tr("O endereço da API precisa começar com https://
(http:// só para serviços no seu próprio PC, como o Ollama).");
        return chat_field_error(hwnd, IDC_BASE, text);
    }
    let key_name = text_of(item(hwnd, IDC_KEYENV));
    if !key_name.is_empty() && !secret::valid_name(&key_name) {
        return chat_field_error(hwnd, IDC_KEYENV, tr("O nome da chave só pode ter letras, números e _ (ex.: GEMINI_API_KEY)."));
    }
    let mut key = text_of(item(hwnd, IDC_KEY));
    if !key.is_empty() {
        let saved = secret::save(&key_name, &key);
        key.as_bytes_mut().fill(0); // não deixa a chave solta na memória
        if let Err(e) = saved {
            return chat_field_error(hwnd, IDC_KEY, &fill(tr("Não salvei a chave: {}"), &[&e]));
        }
        set_text(hwnd, IDC_KEY, "");
    }
    st.chat.api_base = base;
    st.chat.model = text_of(item(hwnd, IDC_MODEL));
    st.chat.key_env = key_name;
    // Serviço pronto escolhido: usa o "esforço" dele; personalizado: não envia.
    st.chat.effort = st.chat.provider_index().map_or(String::new(), |i| PROVIDERS[i].4.to_string());
    st.chat.save();
    show_key_status(hwnd);
    true
}

unsafe fn on_chat_command(hwnd: HWND, id: i32) {
    let Some(st) = state(hwnd) else { return };
    match id {
        IDC_KEY_REMOVE => {
            secret::remove(&text_of(item(hwnd, IDC_KEYENV)));
            show_key_status(hwnd);
        }
        IDC_GETKEY => {
            let url = PROVIDERS[combo_index(hwnd, IDC_PROVIDER).min(PROVIDERS.len() - 1)].5;
            ShellExecuteW(hwnd, w("open").as_ptr(), w(url).as_ptr(), null(), null(), SW_SHOWNORMAL);
        }
        IDC_TEST => {
            if st.busy.is_some() || !save_chat(hwnd) {
                return;
            }
            let plugin = Plugin::native();
            st.busy = Some(Busy::Test);
            set_text(hwnd, IDC_TEST_RESULT, tr("Testando..."));
            plugins::request(hwnd, WM_CHAT_REPLY, &plugin, None, hello_payload(), |reply| reply);
        }
        _ => {}
    }
}

fn hello_payload() -> String {
    chat::payload(tr("Responda em uma frase curta, em português."), &[(true, tr("Diga oi!").into())], None)
}

// --- plugins ----------------------------------------------------------------------

unsafe fn fill_plugins(hwnd: HWND, select: Option<usize>) {
    let Some(st) = state(hwnd) else { return };
    let list = item(hwnd, IDC_PLUGINS);
    st.loading = true;
    SendMessageW(list, LVM_DELETEALLITEMS, 0, 0);
    for (i, p) in st.plugins.iter().enumerate() {
        let status = p.status(&st.draft.plugins);
        let label = match status {
            Status::On => tr("ligado"),
            Status::Off => tr("desligado"),
            Status::Changed => tr("arquivo mudou"),
            Status::Missing => tr("falta o arquivo"),
        };
        insert_row(list, i, &[&p.name, p.kind.label(), label]);
        set_checked(list, i, status == Status::On);
    }
    if let Some(i) = select {
        set_state(list, i, LVIS_SELECTED | LVIS_FOCUSED, LVIS_SELECTED | LVIS_FOCUSED);
    }
    st.loading = false;
    show_plugin(hwnd, select);
}

fn selected_plugin(hwnd: HWND) -> Option<usize> {
    let i = unsafe { SendMessageW(item(hwnd, IDC_PLUGINS), LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as LPARAM) };
    (i >= 0).then_some(i as usize)
}

/// Descrição do plugin selecionado.
unsafe fn show_plugin(hwnd: HWND, index: Option<usize>) {
    let Some(st) = state(hwnd) else { return };
    let plugin = index.and_then(|i| st.plugins.get(i));
    let text = plugin.map_or(tr("Selecione um plugin para ver os detalhes.").to_string(), |p| {
        let file = p.program.file_name().map_or(String::new(), |f| f.to_string_lossy().into_owned());
        let when = if p.kind == PluginKind::Notice { fill(tr("  •  fala a cada {} min"), &[&p.every]) } else { String::new() };
        let warning = match p.status(&st.draft.plugins) {
            Status::Changed => tr("
O arquivo mudou depois que você ligou: marque de novo só se confiar na nova versão."),
            Status::Missing => tr("
O arquivo do plugin não está mais na pasta."),
            _ => "",
        };
        format!("{}\n{} {file}{when}{warning}", p.about, tr("Arquivo:"))
    });
    set_text(hwnd, IDC_PLUGIN_INFO, &text);
    EnableWindow(item(hwnd, IDC_PLUGIN_TEST), plugin.is_some() as BOOL);
}

unsafe fn on_plugins_change(hwnd: HWND, nm: &NMLISTVIEW) {
    let Some(st) = state(hwnd) else { return };
    if st.loading || nm.iItem < 0 {
        return;
    }
    if let Some(on) = check_toggled(nm) {
        // A pergunta de confirmação não pode abrir dentro da notificação da lista.
        st.plugin_toggle = Some((nm.iItem as usize, on));
        PostMessageW(hwnd, WM_PLUGIN_TOGGLE, 0, 0);
    }
    if (nm.uOldState ^ nm.uNewState) & LVIS_SELECTED != 0 {
        show_plugin(hwnd, selected_plugin(hwnd));
    }
}

/// Liga ou desliga o plugin da linha `i`. Só um plugin de conversa fica ligado.
unsafe fn toggle_plugin(hwnd: HWND, i: usize, on: bool) {
    let Some(plugin) = state(hwnd).and_then(|st| st.plugins.get(i).cloned()) else { return };
    let approved = if on { approve(hwnd, &plugin) } else { None };
    // Reobtém o estado: a pergunta roda um laço de mensagens próprio.
    let Some(st) = state(hwnd) else { return };
    st.draft.plugins.retain(|e| e.id != plugin.id);
    if let Some(enabled) = approved {
        if plugin.kind == PluginKind::Chat {
            let chats: Vec<&str> = st.plugins.iter().filter(|p| p.kind == PluginKind::Chat).map(|p| p.id.as_str()).collect();
            st.draft.plugins.retain(|e| !chats.contains(&e.id.as_str()));
        }
        st.draft.plugins.push(enabled);
    }
    fill_plugins(hwnd, Some(i));
}

/// Pede a sua confirmação e guarda a impressão digital do programa.
unsafe fn approve(hwnd: HWND, plugin: &Plugin) -> Option<Enabled> {
    if !plugin.program.is_file() {
        info(hwnd, &fill(tr("O arquivo do plugin \"{}\" não está mais na pasta."), &[&plugin.name]));
        return None;
    }
    if plugin.is_native() {
        return Some(Enabled { id: plugin.id.clone(), fingerprint: String::new() });
    }
    let file = plugin.program.file_name().map_or(String::new(), |f| f.to_string_lossy().into_owned());
    let question = fill(
        tr("Ligar o plugin \"{}\"?\n\n{}\n\nEle é um programa ({}) que vai rodar no seu PC com as suas permissões. \
            Ligue só plugins de quem você confia."),
        &[&plugin.name, &plugin.about, &file],
    );
    if MessageBoxW(hwnd, w(&question).as_ptr(), w("!StayAlone").as_ptr(), MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2) != IDYES {
        return None;
    }
    let Some(fingerprint) = plugin.fingerprint() else {
        info(hwnd, tr("Não consegui ler o arquivo do plugin."));
        return None;
    };
    Some(Enabled { id: plugin.id.clone(), fingerprint })
}

unsafe fn on_plugin_command(hwnd: HWND, id: i32) {
    let Some(st) = state(hwnd) else { return };
    match id {
        IDC_PLUGIN_TEST => {
            let Some(plugin) = selected_plugin(hwnd).and_then(|i| st.plugins.get(i).cloned()) else { return };
            if st.busy.is_some() {
                return;
            }
            if plugin.status(&st.draft.plugins) != Status::On {
                set_text(hwnd, IDC_PLUGIN_RESULT, tr("Ligue o plugin para testar."));
                return;
            }
            let input = match plugin.kind {
                PluginKind::Chat => hello_payload(),
                PluginKind::Notice => {
                    let mascot = st.packs.iter().find(|p| p.id == st.draft.mascot).and_then(|p| pack::load(p).ok());
                    let (name, female) = mascot.map_or((tr("Mascote").into(), false), |p| (p.art.name.clone(), p.art.female()));
                    plugins::notice_input(&name, female, 12)
                }
            };
            st.busy = Some(Busy::Plugin);
            set_text(hwnd, IDC_PLUGIN_RESULT, &fill(tr("Testando \"{}\"..."), &[&plugin.name]));
            let approved = plugin.approved(&st.draft.plugins);
            plugins::request(hwnd, WM_CHAT_REPLY, &plugin, approved, input, |reply| reply);
        }
        IDC_PLUGIN_FOLDER => {
            if let Some(dir) = plugins::prepare_dir() {
                ShellExecuteW(hwnd, w("open").as_ptr(), w(&dir.to_string_lossy()).as_ptr(), null(), null(), SW_SHOWNORMAL);
            }
        }
        IDC_PLUGIN_RELOAD => {
            st.plugins = plugins::list();
            fill_plugins(hwnd, None);
        }
        _ => {}
    }
}

// --- criar mascote --------------------------------------------------------------

/// Coloca um desenho no editor (e nos campos de nome/personalidade).
unsafe fn load_drawing(hwnd: HWND, drawing: Drawing, keep_name: bool) {
    let Some(st) = state(hwnd) else { return };
    if keep_name {
        set_text(hwnd, IDC_NAME, &drawing.name);
    }
    set_text(hwnd, IDC_ABOUT, &drawing.about);
    SendMessageW(item(hwnd, IDC_PRONOUN), CB_SETCURSEL, drawing.female as usize, 0);
    st.drawing = drawing;
    st.color = 1;
    st.pose = Pose::Idle;
    SendMessageW(item(hwnd, IDC_POSE), CB_SETCURSEL, 0, 0);
    redraw_editor(hwnd);
}

unsafe fn on_maker_command(hwnd: HWND, id: i32, code: u32) {
    let Some(st) = state(hwnd) else { return };
    match id {
        IDC_TEMPLATE if code == CBN_SELCHANGE => {
            let i = combo_index(hwnd, IDC_TEMPLATE);
            let drawing = match i.checked_sub(1).and_then(|p| st.packs.get(p)) {
                None => Drawing::blank(),
                Some(info) => match pack::load(info) {
                    Ok(p) => match Drawing::from_sheet(&p.art.sheet, p.art.female()) {
                        Some(d) => d,
                        None => return,
                    },
                    Err(e) => return info_err(hwnd, &e),
                },
            };
            // O nome fica para você escolher: salvar com o nome de um embutido o substituiria.
            load_drawing(hwnd, drawing, false);
        }
        IDC_POSE if code == CBN_SELCHANGE => {
            st.pose = Pose::ALL[combo_index(hwnd, IDC_POSE).min(Pose::ALL.len() - 1)];
            redraw_editor(hwnd);
        }
        IDC_CLEAR => {
            st.drawing.clear(st.pose);
            redraw_editor(hwnd);
        }
        IDC_MIRROR => {
            st.drawing.mirror(st.pose);
            redraw_editor(hwnd);
        }
        IDC_SAVE_MASCOT => save_mascot(hwnd),
        IDC_AI_GO => {
            let description = text_of(item(hwnd, IDC_AI_TEXT));
            if st.busy.is_some() {
                return;
            }
            if description.is_empty() {
                info(hwnd, tr("Descreva o mascote que você quer (ex.: um polvo roxo de chapéu)."));
                SetFocus(item(hwnd, IDC_AI_TEXT));
                return;
            }
            if !save_chat(hwnd) {
                return;
            }
            let plugin = Plugin::native();
            st.busy = Some(Busy::Draw);
            set_text(hwnd, IDC_AI_STATUS, tr("Desenhando... isso pode levar alguns segundos."));
            let payload = chat::payload(&chat::draw_prompt(), &[(true, description)], Some(4096));
            plugins::request(hwnd, WM_CHAT_REPLY, &plugin, None, payload, |reply| reply);
        }
        _ => {}
    }
}

unsafe fn info_err(hwnd: HWND, e: &str) {
    info(hwnd, &fill(tr("Não consegui abrir esse mascote.\n\n{}"), &[&e]));
}

unsafe fn save_mascot(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    st.drawing.name = text_of(item(hwnd, IDC_NAME));
    st.drawing.about = text_of(item(hwnd, IDC_ABOUT));
    st.drawing.female = combo_index(hwnd, IDC_PRONOUN) == 1;
    if st.drawing.name.is_empty() {
        info(hwnd, tr("Dê um nome para o seu mascote."));
        SetFocus(item(hwnd, IDC_NAME));
        return;
    }
    if st.drawing.is_empty() {
        info(hwnd, tr("Desenhe alguma coisa primeiro (ou comece a partir de um mascote)."));
        return;
    }
    let id = st.drawing.id();
    let Some(dir) = pack::user_dir().map(|d| d.join(&id)) else { return };
    let replaces = pack::EMBEDDED.iter().any(|(e, ..)| *e == id) || dir.exists();
    if replaces {
        let msg = fill(tr("Já existe um mascote chamado \"{}\". Substituir?"), &[&st.drawing.name]);
        if MessageBoxW(hwnd, w(&msg).as_ptr(), w("!StayAlone").as_ptr(), MB_YESNO | MB_ICONQUESTION) != IDYES {
            return;
        }
    }
    let written = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("mascot.txt"), st.drawing.to_mascot_txt()));
    if let Err(e) = written {
        return info(hwnd, &fill(tr("Não consegui salvar o mascote.\n\n{}"), &[&e]));
    }
    // Atualiza as listas e já coloca o mascote novo na tela.
    st.packs = pack::list();
    st.draft.mascot = id.clone();
    let template = combo_index(hwnd, IDC_TEMPLATE);
    fill_mascot_combos(hwnd);
    SendMessageW(item(hwnd, IDC_TEMPLATE), CB_SETCURSEL, template, 0);
    mailbox::post(st.owner, WM_MASCOT_SAVED, id);
    let saved = if st.drawing.female { tr("Salvo! A {} já está na sua área de trabalho.") } else { tr("Salvo! O {} já está na sua área de trabalho.") };
    set_text(hwnd, IDC_AI_STATUS, &fill(saved, &[&st.drawing.name]));
}

unsafe fn on_reply(hwnd: HWND, reply: Reply) {
    let Some(st) = state(hwnd) else { return };
    match (st.busy.take(), reply) {
        (Some(Busy::Test), Ok(text)) => set_text(hwnd, IDC_TEST_RESULT, &fill(tr("✓ Funcionou! \"{}\""), &[&chat::shorten(&text)])),
        (Some(Busy::Test), Err(e)) => set_text(hwnd, IDC_TEST_RESULT, &format!("✗ {}", chat::shorten(&e))),
        (Some(Busy::Draw), Ok(text)) => match Drawing::from_ai(&text) {
            Ok(drawing) => {
                SendMessageW(item(hwnd, IDC_TEMPLATE), CB_SETCURSEL, 0, 0);
                load_drawing(hwnd, drawing, true);
                set_text(hwnd, IDC_AI_STATUS, tr("Pronto! Ajuste o desenho se quiser e clique em Salvar e usar."));
            }
            Err(e) => set_text(hwnd, IDC_AI_STATUS, &format!("Hmm, {e}")),
        },
        (Some(Busy::Draw), Err(e)) => set_text(hwnd, IDC_AI_STATUS, &fill(tr("Não deu: {}"), &[&chat::shorten(&e)])),
        (Some(Busy::Plugin), Ok(text)) if text.is_empty() => {
            set_text(hwnd, IDC_PLUGIN_RESULT, tr("✓ Rodou, mas não tinha nada para falar desta vez."));
        }
        (Some(Busy::Plugin), Ok(text)) => set_text(hwnd, IDC_PLUGIN_RESULT, &fill(tr("✓ Respondeu: \"{}\""), &[&chat::shorten(&text)])),
        (Some(Busy::Plugin), Err(e)) => set_text(hwnd, IDC_PLUGIN_RESULT, &format!("✗ {}", chat::shorten(&e))),
        (Some(Busy::GalleryList), Ok(text)) => {
            st.gallery = gallery::parse_lines(&text);
            st.gallery_loaded = true;
            let count = st.gallery.len();
            fill_gallery(hwnd);
            set_text(hwnd, IDC_GALLERY_STATUS, &fill(tr("{} mascotes na galeria."), &[&count]));
        }
        (Some(Busy::GalleryList), Err(e)) => {
            set_text(hwnd, IDC_GALLERY_STATUS, &fill(tr("Não consegui abrir a galeria: {}"), &[&chat::shorten(&e)]));
        }
        (Some(Busy::GalleryInstall), Ok(_)) => {
            st.packs = pack::list();
            fill_mascot_combos(hwnd);
            set_text(hwnd, IDC_GALLERY_STATUS, tr("Instalado! Escolha o mascote na página Geral (ou no painel)."));
            show_gallery_item(hwnd);
        }
        (Some(Busy::GalleryInstall), Err(e)) => {
            set_text(hwnd, IDC_GALLERY_STATUS, &fill(tr("Não instalei: {}"), &[&chat::shorten(&e)]));
            show_gallery_item(hwnd);
        }
        (None, _) => {}
    }
}

// --- OK -------------------------------------------------------------------------

unsafe fn show_memory_status(hwnd: HWND) {
    let text = match memory::load().len() {
        0 => tr("Ainda não lembra de nada.").to_string(),
        1 => tr("Lembra de 1 coisa.").to_string(),
        n => fill(tr("Lembra de {} coisas."), &[&n]),
    };
    set_text(hwnd, IDC_MEMORY_STATUS, &text);
}

unsafe fn on_memory_command(hwnd: HWND, id: i32) {
    match id {
        IDC_MEMORY_OPEN => {
            if let Some(path) = memory::create() {
                ShellExecuteW(hwnd, w("open").as_ptr(), w(&path.to_string_lossy()).as_ptr(), null(), null(), SW_SHOWNORMAL);
            }
        }
        IDC_MEMORY_CLEAR => {
            let question = w(tr("Esquecer tudo o que o mascote lembra de você?"));
            if MessageBoxW(hwnd, question.as_ptr(), w("!StayAlone").as_ptr(), MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2) == IDYES {
                memory::forget_all();
                show_memory_status(hwnd);
            }
        }
        _ => {}
    }
}

/// Os interruptores (desenhados aqui; o estado fica em `State::toggles`).
const TOGGLES: [i32; 7] = [IDC_AUTOSTART, IDC_UPDATES, IDC_MEETINGS, IDC_MEMORY, IDC_FOCUS_QUIET, IDC_HOTKEY, IDC_ACCESSORIES];

unsafe fn is_checked(hwnd: HWND, id: i32) -> bool {
    state(hwnd).and_then(|st| st.toggles.iter().find(|t| t.0 == id).map(|t| t.1)).unwrap_or(false)
}

unsafe fn set_checked_box(hwnd: HWND, id: i32, on: bool) {
    if let Some(toggle) = state(hwnd).and_then(|st| st.toggles.iter_mut().find(|t| t.0 == id)) {
        toggle.1 = on;
    }
    InvalidateRect(item(hwnd, id), null(), 0);
}

unsafe fn on_ok(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let Some(away) = number(hwnd, IDC_AWAY, 1..=120, tr("Tempo para considerar ausente")) else {
        return show_page(hwnd, Page::General as usize);
    };
    let Some(water_goal) = number(hwnd, IDC_WATER_GOAL, 0..=30, tr("Meta de água")) else {
        return show_page(hwnd, Page::Reminders as usize);
    };
    let Some(focus) = number(hwnd, IDC_FOCUS, 1..=180, tr("Foco (pomodoro)")) else {
        return show_page(hwnd, Page::Reminders as usize);
    };
    let Some(pause) = number(hwnd, IDC_BREAK, 1..=60, tr("Pausa do foco")) else {
        return show_page(hwnd, Page::Reminders as usize);
    };
    if !save_chat(hwnd) {
        return;
    }
    st.draft.companion.water_goal = water_goal;
    st.draft.companion.focus_minutes = focus;
    st.draft.companion.break_minutes = pause;
    st.draft.companion.focus_holds_reminders = is_checked(hwnd, IDC_FOCUS_QUIET);
    st.draft.chat_hotkey = is_checked(hwnd, IDC_HOTKEY);
    st.draft.accessories = is_checked(hwnd, IDC_ACCESSORIES);
    if let Some(p) = st.packs.get(combo_index(hwnd, IDC_MASCOT)) {
        st.draft.mascot = p.id.clone();
    }
    st.draft.size = Size::ALL[combo_index(hwnd, IDC_SIZE).min(Size::ALL.len() - 1)];
    st.draft.speed = combo_index(hwnd, IDC_SPEED) as u32 + 1;
    st.draft.companion.away_minutes = away;
    let birthday = text_of(item(hwnd, IDC_BIRTHDAY));
    st.draft.birthday = config::parse_birthday(&birthday);
    if !birthday.is_empty() && st.draft.birthday.is_none() {
        show_page(hwnd, Page::General as usize);
        info(hwnd, tr("Aniversário: use dia/mês, por exemplo 25/12."));
        SetFocus(item(hwnd, IDC_BIRTHDAY));
        return;
    }
    st.draft.updates = is_checked(hwnd, IDC_UPDATES);
    st.draft.quiet_in_meetings = is_checked(hwnd, IDC_MEETINGS);
    st.draft.memory = is_checked(hwnd, IDC_MEMORY);
    st.draft.buddy = combo_index(hwnd, IDC_BUDDY).checked_sub(1).and_then(|i| st.packs.get(i)).map_or(String::new(), |p| p.id.clone());
    st.draft.theme = Theme::ALL[combo_index(hwnd, IDC_THEME).min(Theme::ALL.len() - 1)];
    st.draft.language = Language::ALL[combo_index(hwnd, IDC_LANGUAGE).min(Language::ALL.len() - 1)];
    let autostart = is_checked(hwnd, IDC_AUTOSTART);
    mailbox::post(st.owner, WM_SETTINGS_APPLY, Draft { config: st.draft.clone(), autostart });
    DestroyWindow(hwnd);
}

// --- pintura: barra lateral, título da página e cartões ------------------------------

/// Item `i` da navegação, em pixels.
fn nav_rect(i: usize, dpi: u32) -> RECT {
    let top = NAV_TOP + i as i32 * NAV_STEP;
    RECT { left: scale(12, dpi), top: scale(top, dpi), right: scale(SIDEBAR - 12, dpi), bottom: scale(top + 40, dpi) }
}

/// O item "Sobre", no rodapé da barra lateral.
fn about_rect(dpi: u32) -> RECT {
    RECT { left: scale(12, dpi), top: scale(HEIGHT - 56, dpi), right: scale(SIDEBAR - 12, dpi), bottom: scale(HEIGHT - 16, dpi) }
}

fn nav_hit(lp: LPARAM, dpi: u32) -> Option<usize> {
    let (x, y) = ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
    let inside = |r: RECT| x >= r.left && x < r.right && y >= r.top && y < r.bottom;
    (0..PAGES.len()).find(|&i| inside(nav_rect(i, dpi))).or(inside(about_rect(dpi)).then_some(ABOUT))
}

/// Data de lançamento (gravada na compilação), no formato do idioma.
fn release_date() -> String {
    let date = env!("STAYALONE_RELEASE_DATE"); // AAAA-MM-DD
    let (year, month, day) = (&date[0..4], &date[5..7], &date[8..10]);
    if !crate::lang::is_english() {
        return format!("{day}/{month}/{year}");
    }
    const MONTHS: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let m = month.parse::<usize>().unwrap_or(1).clamp(1, 12);
    format!("{} {}, {year}", MONTHS[m - 1], day.trim_start_matches('0'))
}

unsafe fn paint(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let mut ps: PAINTSTRUCT = zeroed();
    let dc = BeginPaint(hwnd, &mut ps);
    let mut client: RECT = zeroed();
    GetClientRect(hwnd, &mut client);
    let s = |v: i32| scale(v, st.dpi);
    let (center, left) = (DT_CENTER | DT_VCENTER | DT_SINGLELINE, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
    // Tudo num bitmap e depois de uma vez na tela: sem piscar.
    let mut c = Canvas::new(client.right.max(1), client.bottom.max(1));
    let (width, height) = (c.width, c.height);
    c.fill(0, 0, width, height, argb(theme::bg()));

    // Barra lateral: o mascote escolhido (muda ao vivo) e a navegação.
    let side = s(SIDEBAR);
    c.fill(0, 0, side, height, argb(theme::sidebar()));
    c.fill(side - 1, 0, 1, height, argb(theme::border()));
    let box_size = s(84);
    let bx = (side - box_size) / 2;
    c.card((bx, s(24), box_size, box_size), s(20), argb(theme::card()), argb(theme::border()));
    c.sprite_fit(bx + (box_size - s(64)) / 2, s(24) + (box_size - s(64)) / 2, s(64), &st.preview.1);
    c.text(st.bold, &st.preview.0, RECT { left: 0, top: s(116), right: side, bottom: s(140) }, theme::text(), center);
    c.text(st.small, tr("Configurações"), RECT { left: 0, top: s(138), right: side, bottom: s(156) }, theme::muted(), center);
    let version = fill(tr("Sobre · v{}"), &[&crate::update::current()]);
    let items = PAGES.iter().map(|&(_, glyph, label)| (glyph, tr(label).to_string())).chain(std::iter::once((icon::INFO, version)));
    for (i, (glyph, label)) in items.enumerate() {
        let r = if i == ABOUT { about_rect(st.dpi) } else { nav_rect(i, st.dpi) };
        let (x, y, w, h) = (r.left, r.top, r.right - r.left, r.bottom - r.top);
        let selected = i == st.page;
        if selected {
            c.card((x, y, w, h), s(8), argb(theme::card()), argb(theme::border()));
            c.round_rect(x + s(6), y + s(11), s(3), h - s(22), s(2), argb(theme::accent()));
        } else if st.nav_hover == Some(i) {
            c.round_rect(x, y, w, h, s(8), argb(theme::hover()));
        }
        let (ink, font) = if selected { (theme::accent(), st.bold) } else { (theme::muted(), st.font) };
        c.text(st.icons, &glyph.to_string(), RECT { left: x + s(14), top: y, right: x + s(38), bottom: y + h }, ink, center);
        let text_ink = if selected { theme::text() } else { theme::muted() };
        c.text(font, &label, RECT { left: x + s(46), top: y, right: x + w - s(6), bottom: y + h }, text_ink, left);
    }

    // Título da página e os cartões atrás dos controles.
    let title = RECT { left: side + s(CARD_X), top: s(14), right: width - s(20), bottom: s(50) };
    let page_title = if st.page == ABOUT { tr("Sobre") } else { tr(PAGES[st.page].2) };
    c.text(st.title, page_title, title, theme::text(), left);
    for (_, r) in st.cards.iter().filter(|(p, _)| *p == st.page) {
        let (x, y) = (side + s(r.left), s(r.top - SHIFT));
        c.card((x, y, s(r.right - r.left), s(r.bottom - r.top)), s(12), argb(theme::card()), argb(theme::border()));
    }
    c.blit(dc, 0, 0);
    EndPaint(hwnd, &ps);
}

/// Botões arredondados: o principal de cada área em laranja, os outros brancos.
unsafe fn draw_button(hwnd: HWND, di: &DRAWITEMSTRUCT) {
    let Some(st) = state(hwnd) else { return };
    let id = di.CtlID as i32;
    let style = ui::ButtonStyle {
        primary: matches!(id, IDOK | IDC_SAVE_MASCOT | IDC_AI_GO | IDC_ADD | IDC_TEST | IDC_PLUGIN_TEST),
        // Os cantos mostram o fundo de onde o botão está: cartão ou rodapé.
        background: if matches!(id, IDOK | IDCANCEL) { theme::bg() } else { theme::card() },
        font: st.font,
        bold: st.bold,
        dpi: st.dpi,
    };
    if TOGGLES.contains(&id) {
        ui::draw_toggle(di, is_checked(hwnd, id), &style);
    } else {
        ui::draw_button(di, &style);
    }
}

/// Mascote mostrado na barra lateral: (nome, sprite parado).
fn preview_of(info: Option<&PackInfo>) -> (String, Vec<u32>) {
    let Some(pack) = info.and_then(|i| pack::load(i).ok()) else { return (String::new(), vec![0; PIXELS]) };
    (pack.art.name.clone(), pack.art.pixels(Frame::Idle))
}

unsafe fn refresh_preview(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let chosen = st.packs.get(combo_index(hwnd, IDC_MASCOT));
    st.preview = preview_of(chosen);
    let side = RECT { left: 0, top: 0, right: scale(SIDEBAR, st.dpi), bottom: scale(NAV_TOP, st.dpi) };
    InvalidateRect(hwnd, &side, 0);
}

unsafe fn set_nav_hover(hwnd: HWND, hover: Option<usize>) {
    let Some(st) = state(hwnd) else { return };
    if st.nav_hover != hover {
        st.nav_hover = hover;
        let side = RECT { left: 0, top: 0, right: scale(SIDEBAR, st.dpi), bottom: scale(HEIGHT, st.dpi) };
        InvalidateRect(hwnd, &side, 0);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_MOUSEMOVE => {
            let dpi = state(hwnd).map_or(96, |st| st.dpi);
            set_nav_hover(hwnd, nav_hit(lp, dpi));
            let mut track = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
            TrackMouseEvent(&mut track);
            0
        }
        WM_MOUSELEAVE => {
            set_nav_hover(hwnd, None);
            0
        }
        WM_LBUTTONUP => {
            let dpi = state(hwnd).map_or(96, |st| st.dpi);
            if let Some(page) = nav_hit(lp, dpi) {
                show_page(hwnd, page);
            }
            0
        }
        WM_DRAWITEM => {
            draw_button(hwnd, &*(lp as *const DRAWITEMSTRUCT));
            1
        }
        WM_COMMAND => {
            let (id, code) = ((wp & 0xFFFF) as i32, ((wp >> 16) & 0xFFFF) as u32);
            match id {
                IDOK => on_ok(hwnd),
                IDCANCEL => {
                    DestroyWindow(hwnd);
                }
                IDC_MASCOT if code == CBN_SELCHANGE => refresh_preview(hwnd),
                IDC_MODS => {
                    if let Some(dir) = pack::prepare_user_dir() {
                        ShellExecuteW(hwnd, w("open").as_ptr(), w(&dir.to_string_lossy()).as_ptr(), null(), null(), SW_SHOWNORMAL);
                    }
                }
                IDC_ADD | IDC_UPDATE | IDC_REMOVE => on_reminder_command(hwnd, id),
                IDC_ABOUT_UPDATE => {
                    if let Some(st) = state(hwnd) {
                        PostMessageW(st.owner, WM_UPDATE_REQUEST, 0, 0);
                    }
                    set_text(hwnd, IDC_ABOUT_STATUS, tr("Procurando... o mascote avisa o que encontrar."));
                }
                IDC_ABOUT_PROJECTS | IDC_ABOUT_GITHUB => {
                    let url = if id == IDC_ABOUT_PROJECTS { PROJECTS_URL } else { GITHUB_URL };
                    ShellExecuteW(hwnd, w("open").as_ptr(), w(url).as_ptr(), null(), null(), SW_SHOWNORMAL);
                }
                IDC_PROVIDER if code == CBN_SELCHANGE => {
                    let i = combo_index(hwnd, IDC_PROVIDER);
                    if let Some(st) = state(hwnd).filter(|_| i < PROVIDERS.len()) {
                        let max_tokens = st.chat.max_tokens;
                        st.chat = ChatSettings { max_tokens, ..ChatSettings::provider(i) };
                        show_chat_fields(hwnd);
                    }
                }
                IDC_KEYENV if code == EN_CHANGE => show_key_status(hwnd),
                IDC_KEY_REMOVE | IDC_GETKEY | IDC_TEST => on_chat_command(hwnd, id),
                IDC_MEMORY_OPEN | IDC_MEMORY_CLEAR => on_memory_command(hwnd, id),
                IDC_TEMPLATE | IDC_POSE | IDC_CLEAR | IDC_MIRROR | IDC_SAVE_MASCOT | IDC_AI_GO => on_maker_command(hwnd, id, code),
                IDC_PLUGIN_TEST | IDC_PLUGIN_FOLDER | IDC_PLUGIN_RELOAD => on_plugin_command(hwnd, id),
                IDC_GALLERY_INSTALL | IDC_GALLERY_RELOAD => on_gallery_command(hwnd, id),
                id if TOGGLES.contains(&id) && code == BN_CLICKED => set_checked_box(hwnd, id, !is_checked(hwnd, id)),
                _ => {}
            }
            0
        }
        WM_NOTIFY => {
            let hdr = &*(lp as *const NMHDR);
            if hdr.idFrom == IDC_LIST as usize && hdr.code == LVN_ITEMCHANGED {
                on_list_change(hwnd, &*(lp as *const NMLISTVIEW));
            } else if hdr.idFrom == IDC_GALLERY as usize && hdr.code == LVN_ITEMCHANGED {
                show_gallery_item(hwnd);
            } else if hdr.idFrom == IDC_PLUGINS as usize && hdr.code == LVN_ITEMCHANGED {
                on_plugins_change(hwnd, &*(lp as *const NMLISTVIEW));
            }
            0
        }
        WM_PLUGIN_TOGGLE => {
            if let Some((i, on)) = state(hwnd).and_then(|st| st.plugin_toggle.take()) {
                toggle_plugin(hwnd, i, on);
            }
            0
        }
        WM_CHAT_REPLY => {
            if let Some(reply) = mailbox::take::<Reply>(hwnd, msg) {
                on_reply(hwnd, reply);
            }
            0
        }
        // Rótulos ficam sobre os cartões brancos; títulos em destaque e dicas em cinza.
        WM_CTLCOLORSTATIC => {
            let Some(st) = state(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
            let dc = wp as HDC;
            SetBkMode(dc, TRANSPARENT as _);
            let ink = match GetDlgCtrlID(lp as HWND) {
                IDC_SECTION => theme::accent(),
                IDC_HINT => theme::muted(),
                _ => theme::text(),
            };
            SetTextColor(dc, theme::colorref(ink));
            st.card_brush as LRESULT
        }
        // Campos de texto e a lista aberta dos seletores com as cores do tema.
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            let Some(st) = state(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
            let dc = wp as HDC;
            SetTextColor(dc, theme::colorref(theme::text()));
            SetBkColor(dc, theme::colorref(theme::card()));
            st.card_brush as LRESULT
        }
        WM_DESTROY => {
            OPEN.with(|o| o.set(null_mut()));
            0
        }
        WM_NCDESTROY => {
            mailbox::discard(hwnd); // resposta da IA que chegou tarde demais
            let st = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
            if !st.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let st = Box::from_raw(st);
                for object in [st.font, st.bold, st.title, st.small, st.icons, st.card_brush] {
                    DeleteObject(object);
                }
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
