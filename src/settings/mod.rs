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
    ask,
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
    folders::Access,
    plugins::{self, Enabled, Kind as PluginKind, Plugin, Status},
    resources::{self, Sensitivity},
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
    Performance,
    /// Fora da lista: aberta pelo item "Sobre" no rodapé da barra lateral.
    About,
}

/// Índice da página "Sobre" (depois das páginas da navegação).
const ABOUT: usize = Page::About as usize;
/// Links da página "Sobre".
const PROJECTS_URL: &str = "https://4tyllal.github.io/#projects";
const GITHUB_URL: &str = "https://github.com/4TyllaL/notStayAlone";

const PAGES: [(Page, char, &str); 7] = [
    (Page::General, icon::HOME, "Geral"),
    (Page::Reminders, icon::BELL, "Lembretes"),
    (Page::Chat, icon::CHAT, "Conversa"),
    (Page::Maker, icon::PALETTE, "Criar mascote"),
    (Page::Plugins, icon::PUZZLE, "Plugins"),
    (Page::Gallery, icon::SHOP, "Galeria"),
    (Page::Performance, icon::GAUGE, "Desempenho"),
];
const SPEEDS: [&str; 4] = ["Devagar", "Normal", "Rápido", "Muito rápido"];

/// Medidas em pontos de 96 DPI. A área de conteúdo usa coordenadas próprias: x a
/// partir da barra lateral, y a partir do topo da janela.
const SIDEBAR: i32 = 200;
const CONTENT: i32 = 620;
const HEIGHT: i32 = 704;
/// Margem do conteúdo (dos lados e do título) e onde começa a primeira seção.
const MARGIN: i32 = 24;
const TOP: i32 = 60;
/// Folga interna dos cartões, dos lados e em cima/embaixo das linhas.
const PAD: i32 = 16;
const VPAD: i32 = 4;
/// Espaço entre um cartão e o título da seção seguinte.
const GAP: i32 = 16;
/// Altura de uma linha (rótulo + controle) e de uma linha com descrição.
const ROW: i32 = 40;
const ROW_TALL: i32 = 48;
/// Colunas das linhas: rótulo à esquerda, controle numa coluna fixa à direita.
const LABEL_X: i32 = MARGIN + PAD;
const RIGHT: i32 = CONTENT - MARGIN - PAD;
const VALUE_W: i32 = 280;
const VALUE_X: i32 = RIGHT - VALUE_W;
const INNER_W: i32 = RIGHT - LABEL_X;
/// Navegação: primeiro item, passo e altura de cada um.
const NAV_TOP: i32 = 84;
const NAV_STEP: i32 = 40;
const NAV_H: i32 = 36;
const CONTROL: i32 = theme::size::CONTROL;
const BUTTON: i32 = theme::size::BUTTON;

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
/// Linhas do cartão "Segurança e privacidade" (na ordem de `SECURITY_ROWS`).
const IDC_SEC_FIRST: i32 = 184;
/// Rótulo que corta o fim com "…" (winuser.h; o windows-sys não exporta).
const SS_ENDELLIPSIS: u32 = 0x4000;
/// Rótulo que mostra "&" como texto (sem sublinhar a letra seguinte).
const SS_NOPREFIX: u32 = 0x0080;
const SECURITY_ROWS: [&str; 7] = ["Commit", "SHA-256", "Proteções", "Atualizações", "Chave da API", "Conexões", "Plugins"];
const IDC_PC_MOOD: i32 = 153;
const IDC_PC_NOW: i32 = 154;
const IDC_PC_HELPER: i32 = 155;
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
    /// Os plugins como estavam salvos: ao fechar sem salvar, o acesso a pastas dado só
    /// para testar (aprovado, mas não salvo) é retirado.
    saved_plugins: Vec<Enabled>,
    saved: bool,
    chat: ChatSettings,
    packs: Vec<PackInfo>,
    font: HFONT,
    bold: HFONT,
    small: HFONT,
    title: HFONT,
    /// Título de cartão (o nome do app na página Sobre).
    heading: HFONT,
    icons: HFONT,
    /// Fundo dos rótulos (branco, como os cartões).
    card_brush: HBRUSH,
    /// Fundo da caixa do resultado do teste de plugin.
    soft_brush: HBRUSH,
    /// Fundo da janela (textos que ficam fora dos cartões).
    bg_brush: HBRUSH,
    dpi: u32,
    page: usize,
    nav_hover: Option<usize>,
    /// O que é pintado atrás dos controles de cada página (coordenadas de layout).
    sheet: Sheet,
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
    /// SHA-256 deste .exe (calculado na primeira vez que a página Sobre abre).
    exe_sha: Option<String>,
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
        saved_plugins: config.plugins.clone(),
        saved: false,
        chat: ChatSettings::load(),
        packs,
        font: font(theme::font::BODY, FW_NORMAL),
        bold: font(theme::font::BODY, FW_SEMIBOLD),
        small: font(theme::font::SMALL, FW_NORMAL),
        title: font(theme::font::DISPLAY, FW_SEMIBOLD),
        heading: font(theme::font::TITLE, FW_SEMIBOLD),
        icons: theme::icon_font(scale(theme::font::ICON, dpi)),
        card_brush: CreateSolidBrush(theme::colorref(theme::card())),
        soft_brush: CreateSolidBrush(theme::colorref(theme::soft())),
        bg_brush: CreateSolidBrush(theme::colorref(theme::bg())),
        dpi,
        page: page as usize,
        nav_hover: None,
        sheet: Sheet::default(),
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
        exe_sha: None,
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

/// Como um texto pintado atrás dos controles aparece.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ink {
    /// Título de seção (acima do cartão).
    Section,
    /// Rótulo de uma linha.
    Label,
    /// Descrição de uma linha, em cinza, numa linha só.
    Desc,
    /// Nome de um par nome/valor, em cinza.
    Key,
    /// Texto em cinza ao lado de um campo ("min").
    Unit,
    /// Notas em cinza, menores, com quebra de linha.
    Note,
    /// Parágrafo em cinza no tamanho do texto, com quebra de linha.
    Body,
    /// Título de cartão.
    Heading,
}

struct Text {
    page: usize,
    rect: RECT,
    text: String,
    ink: Ink,
}

/// O que a janela pinta atrás dos controles, por página (coordenadas de layout).
#[derive(Default)]
struct Sheet {
    cards: Vec<(usize, RECT)>,
    /// Linhas finas entre as linhas de um cartão.
    lines: Vec<(usize, RECT)>,
    /// Caixas em tom neutro (o resultado do teste de plugin fica numa).
    wells: Vec<(usize, RECT)>,
    /// Retrato do mascote (o cartão da página Sobre).
    portraits: Vec<(usize, RECT)>,
    texts: Vec<Text>,
}

/// Controles fora das páginas (o cabeçalho com Salvar e Cancelar).
const GLOBAL: usize = usize::MAX;

/// Monta uma página de cima para baixo: seções com título, cartões e linhas.
/// Cada linha tem o rótulo na coluna da esquerda e o controle na coluna fixa da
/// direita, então tudo fica alinhado e com a mesma largura.
struct Form<'a> {
    hwnd: HWND,
    st: &'a mut State,
    page: usize,
    y: i32,
    card_top: i32,
    rows: usize,
}

impl Form<'_> {
    fn s(&self, v: i32) -> i32 {
        scale(v, self.st.dpi)
    }

    unsafe fn control(&mut self, class: &str, text: &str, style: u32, ex: u32, (x, y, cw, ch): (i32, i32, i32, i32), id: i32) -> HWND {
        let control = CreateWindowExW(
            ex,
            w(class).as_ptr(),
            w(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            self.s(SIDEBAR + x),
            self.s(y),
            self.s(cw),
            self.s(ch),
            self.hwnd,
            id as isize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(control, WM_SETFONT, self.st.font as WPARAM, 1);
        ui::theme_control(control, class);
        if class == "COMBOBOX" {
            ui::set_combo_height(control, self.s(CONTROL));
        }
        if self.page != GLOBAL {
            self.st.pages.push((self.page, control));
        }
        control
    }

    fn paint(&mut self, ink: Ink, text: &str, (x, y, cw, ch): (i32, i32, i32, i32)) {
        let rect = RECT { left: x, top: y, right: x + cw, bottom: y + ch };
        self.st.sheet.texts.push(Text { page: self.page, rect, text: text.into(), ink });
    }

    /// Tamanho de `text` (em pontos de 96 DPI): largura numa linha só, ou altura
    /// quebrado em `width`.
    unsafe fn measure(&self, ink: Ink, text: &str, width: Option<i32>) -> (i32, i32) {
        self.measure_with(font_of(self.st, ink), text, width)
    }

    unsafe fn measure_with(&self, font: HFONT, text: &str, width: Option<i32>) -> (i32, i32) {
        let dc = CreateCompatibleDC(null_mut());
        let old = SelectObject(dc, font);
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let mut r = RECT { left: 0, top: 0, right: width.map_or(0, |v| self.s(v)), bottom: 0 };
        let wrap = if width.is_some() { DT_WORDBREAK } else { DT_SINGLELINE };
        DrawTextW(dc, wide.as_mut_ptr(), wide.len() as i32, &mut r, DT_CALCRECT | DT_NOPREFIX | wrap);
        SelectObject(dc, old);
        DeleteDC(dc);
        let dpi = self.st.dpi as i32;
        ((r.right * 96 + dpi - 1) / dpi, (r.bottom * 96 + dpi - 1) / dpi)
    }

    fn start(&mut self, page: usize) {
        self.page = page;
        self.y = TOP;
    }

    /// Título de seção (e uma nota opcional embaixo) e o começo do cartão dela.
    unsafe fn section(&mut self, title: &str, note: Option<&str>) {
        self.paint(Ink::Section, title, (MARGIN + 4, self.y, CONTENT - 2 * MARGIN - 8, 20));
        self.y += 24;
        if let Some(note) = note {
            self.y -= 2;
            self.note(note, MARGIN + 4, CONTENT - 2 * MARGIN - 8);
            self.y += 2;
        }
        self.open();
    }

    /// Nota em cinza, com quebra de linha, a partir do y atual.
    unsafe fn note(&mut self, text: &str, x: i32, width: i32) {
        let (_, h) = self.measure(Ink::Note, text, Some(width));
        self.paint(Ink::Note, text, (x, self.y, width, h));
        self.y += h + 8;
    }

    fn open(&mut self) {
        self.card_top = self.y;
        self.rows = 0;
        self.y += VPAD;
    }

    fn close(&mut self) {
        self.y += VPAD;
        let card = RECT { left: MARGIN, top: self.card_top, right: CONTENT - MARGIN, bottom: self.y };
        self.st.sheet.cards.push((self.page, card));
        self.y += GAP;
    }

    /// Nova linha de altura `h` (com a linha divisória em cima, a partir da segunda).
    fn row(&mut self, h: i32) -> i32 {
        if self.rows > 0 {
            let line = RECT { left: LABEL_X, top: self.y, right: RIGHT, bottom: self.y + 1 };
            self.st.sheet.lines.push((self.page, line));
        }
        self.rows += 1;
        let top = self.y;
        self.y += h;
        top
    }

    /// Linha com rótulo (e descrição) à esquerda; devolve o y do controle, centralizado.
    unsafe fn labeled(&mut self, label: &str, about: Option<&str>) -> i32 {
        let h = if about.is_some() { ROW_TALL } else { ROW };
        let top = self.row(h);
        let width = VALUE_X - 16 - LABEL_X;
        match about {
            None => self.paint(Ink::Label, label, (LABEL_X, top, width, h)),
            Some(about) => {
                let y = top + (h - 36) / 2;
                self.paint(Ink::Label, label, (LABEL_X, y, width, 20));
                self.paint(Ink::Desc, &capitalized(about), (LABEL_X, y + 20, width, 16));
            }
        }
        top + (h - CONTROL) / 2
    }

    unsafe fn combo(&mut self, label: &str, id: i32) -> HWND {
        let y = self.labeled(label, None);
        self.control("COMBOBOX", "", COMBO, 0, (VALUE_X, y, VALUE_W, 300), id)
    }

    unsafe fn field(&mut self, style: u32, (x, y, cw): (i32, i32, i32), id: i32) -> HWND {
        self.control("EDIT", "", style, WS_EX_CLIENTEDGE, (x, y, cw, CONTROL), id)
    }

    /// Texto em cinza depois de um campo; devolve onde ele termina.
    unsafe fn unit(&mut self, text: &str, x: i32, y: i32) -> i32 {
        let (width, _) = self.measure(Ink::Unit, text, None);
        self.paint(Ink::Unit, text, (x, y, width + 2, CONTROL));
        x + width
    }

    /// Linha inteira que liga e desliga (o clique vale na linha toda).
    unsafe fn toggle(&mut self, id: i32, label: &str, about: Option<&str>) {
        let h = if about.is_some() { ROW_TALL } else { ROW };
        let top = self.row(h);
        let text = about.map_or(label.to_string(), |a| format!("{label}\n{a}"));
        self.control("BUTTON", &text, BUTTON_STYLE, 0, (LABEL_X, top + 2, INNER_W, h - 4), id);
    }

    /// Largura de um botão para o texto dele.
    unsafe fn button_width(&self, text: &str) -> i32 {
        (self.measure_with(self.st.bold, text, None).0 + 32).max(88)
    }

    /// Botões alinhados à direita, terminando em `right` (o último fica na ponta).
    unsafe fn buttons(&mut self, list: &[(&str, i32)], right: i32, y: i32, height: i32) -> i32 {
        let widths: Vec<i32> = list.iter().map(|(text, _)| self.button_width(text)).collect();
        let mut x = right - widths.iter().sum::<i32>() - 8 * (list.len() as i32 - 1);
        let left = x;
        for (&(text, id), width) in list.iter().zip(widths) {
            self.control("BUTTON", text, BUTTON_STYLE, 0, (x, y, width, height), id);
            x += width + 8;
        }
        left
    }

    /// Texto que muda (estado, resultado): um rótulo do Windows sobre o cartão.
    unsafe fn status(&mut self, id: i32, style: u32, rect: (i32, i32, i32, i32)) -> HWND {
        self.control("STATIC", "", style, 0, rect, id)
    }

    /// Lista (tabela) ocupando a largura do cartão.
    unsafe fn list(&mut self, id: i32, height: i32, checkboxes: bool, columns: &[(&str, i32)]) -> HWND {
        let top = self.row(height + 8) + 4;
        let width = CONTENT - 2 * MARGIN - 16;
        let style = LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS | LVS_NOSORTHEADER | WS_TABSTOP;
        let list = self.control("SysListView32", "", style, 0, (MARGIN + 8, top, width, height), id);
        let ex = LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER | if checkboxes { LVS_EX_CHECKBOXES } else { 0 };
        SendMessageW(list, LVM_SETEXTENDEDLISTVIEWSTYLE, ex as WPARAM, ex as LPARAM);
        // A primeira coluna fica com o que sobra (menos a barra de rolagem).
        let rest: i32 = columns.iter().skip(1).map(|c| c.1).sum();
        let mut widths: Vec<(&str, i32)> = columns.to_vec();
        widths[0].1 = width - rest - 22;
        add_columns(list, &widths, self.st.dpi);
        list
    }
}

/// Primeira letra maiúscula (textos que eram continuação de frase viram descrição).
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
}

fn font_of(st: &State, ink: Ink) -> HFONT {
    match ink {
        Ink::Section => st.bold,
        Ink::Label | Ink::Key | Ink::Unit | Ink::Body => st.font,
        Ink::Desc | Ink::Note => st.small,
        Ink::Heading => st.heading,
    }
}

const COMBO: u32 = CBS_DROPDOWNLIST as u32 | WS_VSCROLL | WS_TABSTOP;
const BUTTON_STYLE: u32 = BS_OWNERDRAW as u32 | WS_TABSTOP;

unsafe fn build(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let mut f = Form { hwnd, st, page: GLOBAL, y: 0, card_top: 0, rows: 0 };
    let edit = ES_AUTOHSCROLL as u32 | WS_TABSTOP;
    let number = ES_NUMBER as u32 | WS_TABSTOP;
    let ellipsis = SS_ENDELLIPSIS | SS_NOPREFIX;

    // --- Cabeçalho: título da página (pintado) e as ações da janela.
    f.buttons(&[(tr("Cancelar"), IDCANCEL), (tr("Salvar"), IDOK)], CONTENT - MARGIN, 16, BUTTON);

    // --- Geral
    f.start(Page::General as usize);
    f.section(tr("Seu mascote"), None);
    // "Abrir pasta" é da seção inteira: fica na linha do título dela, à direita.
    let folder_w = f.button_width(tr("Abrir pasta"));
    f.control("BUTTON", tr("Abrir pasta"), BUTTON_STYLE, 0, (CONTENT - MARGIN - folder_w, f.card_top - 28, folder_w, 28), IDC_MODS);
    f.combo(tr("Mascote"), IDC_MASCOT);
    f.combo(tr("Amigo na tela"), IDC_BUDDY);
    f.combo(tr("Tamanho"), IDC_SIZE);
    f.combo(tr("Velocidade ao andar"), IDC_SPEED);
    f.close();
    f.section(tr("Comportamento"), None);
    let y = f.labeled(tr("Considerar ausente após"), None);
    f.field(number, (VALUE_X, y, 56), IDC_AWAY);
    f.unit(tr("min sem usar o PC"), VALUE_X + 64, y);
    let y = f.labeled(tr("Seu aniversário"), Some(tr("o mascote comemora com você")));
    let birthday = f.field(edit, (VALUE_X, y, 80), IDC_BIRTHDAY);
    SendMessageW(birthday, EM_SETCUEBANNER, 1, w("dd/mm").as_ptr() as LPARAM);
    limit(birthday, 5);
    f.toggle(IDC_AUTOSTART, tr("Iniciar junto com o Windows"), None);
    f.toggle(IDC_UPDATES, tr("Procurar versões novas (uma vez por dia, no GitHub)"), None);
    f.toggle(
        IDC_MEETINGS,
        tr("Ficar quieto em reuniões (Teams, Zoom, Webex...)"),
        Some(tr("Ele olha só o nome do programa aberto, nunca o que está na tela.")),
    );
    f.close();
    f.section(tr("Aparência"), None);
    f.combo(tr("Tema"), IDC_THEME);
    f.combo(tr("Idioma"), IDC_LANGUAGE);
    f.toggle(IDC_ACCESSORIES, tr("Acessórios de época (Natal, Halloween, aniversário...)"), None);
    f.close();

    // --- Lembretes
    f.start(Page::Reminders as usize);
    f.section(tr("Seus lembretes"), Some(tr("Marque para ativar. Eles contam só o tempo em que você está usando o PC.")));
    f.list(IDC_LIST, 176, true, &[(tr("Lembrete"), 0), (tr("A cada"), 96)]);
    // Editar: texto e intervalo numa linha, as ações embaixo (a principal na ponta).
    let top = f.row(ROW);
    let y = top + (ROW - CONTROL) / 2 + 4;
    let x = f.unit(tr("Texto"), LABEL_X, y) + 12;
    let text = f.field(edit, (x, y, 260), IDC_TEXT);
    SendMessageW(text, EM_SETCUEBANNER, 1, w(tr("Ex.: Conferir o e-mail")).as_ptr() as LPARAM);
    limit(text, MAX_REMINDER_TEXT);
    let x = f.unit(tr("a cada"), x + 260 + 12, y) + 8;
    let minutes = f.field(number, (x, y, 56), IDC_MINUTES);
    SetWindowTextW(minutes, w("30").as_ptr());
    f.unit("min", x + 64, y);
    // Sem linha entre os campos e os botões: são uma coisa só.
    f.y += 4;
    let y = f.y;
    f.y += BUTTON + 8;
    f.buttons(&[(tr("Remover"), IDC_REMOVE), (tr("Salvar alteração"), IDC_UPDATE), (tr("Adicionar"), IDC_ADD)], RIGHT, y, BUTTON);
    f.close();
    f.section(tr("Água e foco"), None);
    let y = f.labeled(tr("Meta de água"), None);
    let goal = f.field(number, (VALUE_X, y, 56), IDC_WATER_GOAL);
    limit(goal, 2);
    f.unit(tr("copos por dia (0 = sem meta)"), VALUE_X + 64, y);
    let y = f.labeled(tr("Foco (pomodoro)"), None);
    let focus = f.field(number, (VALUE_X, y, 56), IDC_FOCUS);
    limit(focus, 3);
    let x = f.unit(tr("min, pausa de"), VALUE_X + 64, y) + 8;
    let pause = f.field(number, (x, y, 56), IDC_BREAK);
    limit(pause, 2);
    f.unit("min", x + 64, y);
    f.toggle(IDC_FOCUS_QUIET, tr("Durante o foco, os lembretes esperam a pausa"), None);
    f.close();

    // --- Conversa
    f.start(Page::Chat as usize);
    f.section(tr("Conversar com o mascote"), Some(tr("Serviços no padrão da API da OpenAI. Sem chave, nada sai do seu PC.")));
    f.combo(tr("Serviço"), IDC_PROVIDER);
    let y = f.labeled(tr("Endereço da API"), None);
    let base = f.field(edit, (VALUE_X, y, VALUE_W), IDC_BASE);
    limit(base, 300);
    let y = f.labeled(tr("Modelo"), None);
    let model = f.field(edit, (VALUE_X, y, VALUE_W), IDC_MODEL);
    limit(model, 100);
    let y = f.labeled(tr("Nome da chave"), None);
    let key_name = f.field(edit, (VALUE_X, y, VALUE_W), IDC_KEYENV);
    limit(key_name, 64);
    f.toggle(IDC_HOTKEY, tr("Ctrl+Alt+M abre a conversa de qualquer lugar"), None);
    f.close();
    f.section(
        tr("Chave da API"),
        Some(tr("Fica no Gerenciador de Credenciais do Windows, protegida pela sua conta, e só vai por HTTPS ao serviço escolhido.")),
    );
    let y = f.labeled(tr("Colar a chave"), None);
    let key = f.field(edit | ES_PASSWORD as u32, (VALUE_X, y, VALUE_W), IDC_KEY);
    SendMessageW(key, EM_SETCUEBANNER, 1, w(tr("cole aqui para salvar")).as_ptr() as LPARAM);
    limit(key, 512);
    // Situação da chave à esquerda; criar e remover à direita.
    let top = f.row(ROW);
    let y = top + (ROW - CONTROL) / 2;
    let left = f.buttons(&[(tr("Criar chave"), IDC_GETKEY), (tr("Remover chave"), IDC_KEY_REMOVE)], RIGHT, y, CONTROL);
    f.status(IDC_KEY_STATUS, ellipsis, (LABEL_X, top + 10, left - LABEL_X - 12, 20));
    let top = f.row(ROW_TALL);
    let width = f.button_width(tr("Testar conversa"));
    f.buttons(&[(tr("Testar conversa"), IDC_TEST)], RIGHT, top + (ROW_TALL - BUTTON) / 2, BUTTON);
    f.status(IDC_TEST_RESULT, SS_NOPREFIX, (LABEL_X, top + 7, INNER_W - width - 16, 38));
    f.close();
    f.section(tr("Memória"), None);
    f.toggle(IDC_MEMORY, tr("Lembrar do que eu contar na conversa (fica só neste PC)"), None);
    let top = f.row(ROW);
    let y = top + (ROW - CONTROL) / 2;
    let left = f.buttons(&[(tr("Ver e editar"), IDC_MEMORY_OPEN), (tr("Esquecer tudo"), IDC_MEMORY_CLEAR)], RIGHT, y, CONTROL);
    f.status(IDC_MEMORY_STATUS, ellipsis, (LABEL_X, top + 10, left - LABEL_X - 12, 20));
    f.close();

    // --- Criar mascote: o desenho à esquerda, os dados à direita.
    f.start(Page::Maker as usize);
    f.open();
    let top = f.y + 12;
    let grid = SPRITE as i32 * CELL;
    f.control("StayAlonePixels", "", 0, 0, (LABEL_X, top, grid, grid), IDC_CANVAS);
    let palette_y = top + grid + 12;
    let palette_h = 2 * (SWATCH + SWATCH_GAP) - SWATCH_GAP;
    f.control("StayAlonePalette", "", 0, 0, (LABEL_X, palette_y, 8 * (SWATCH + SWATCH_GAP), palette_h), IDC_PALETTE);
    let tools_y = palette_y + palette_h + 12;
    let half = (grid - 8) / 2;
    f.control("BUTTON", tr("Limpar"), BUTTON_STYLE, 0, (LABEL_X, tools_y, half, CONTROL), IDC_CLEAR);
    f.control("BUTTON", tr("Espelhar"), BUTTON_STYLE, 0, (LABEL_X + half + 8, tools_y, half, CONTROL), IDC_MIRROR);
    f.y = tools_y + CONTROL + 10;
    f.note(tr("Clique pinta • botão direito apaga • duplo clique numa cor troca a cor. A cor com ponto branco são os olhos."), LABEL_X, grid);
    let left_bottom = f.y;
    let xr = LABEL_X + grid + 24;
    let rw = RIGHT - xr;
    let mut y = top - 2;
    let fields = [
        (tr("Nome"), IDC_NAME),
        (tr("Quem é (personalidade)"), IDC_ABOUT),
        (tr("É"), IDC_PRONOUN),
        (tr("Começar a partir de"), IDC_TEMPLATE),
        (tr("Pose que você está desenhando"), IDC_POSE),
    ];
    for (label, id) in fields {
        f.paint(Ink::Label, label, (xr, y, rw, 20));
        if matches!(id, IDC_NAME | IDC_ABOUT) {
            f.field(edit, (xr, y + 22, rw), id);
        } else {
            f.control("COMBOBOX", "", COMBO, 0, (xr, y + 22, rw, 300), id);
        }
        y += 22 + CONTROL + 12;
    }
    limit(item(hwnd, IDC_NAME), 32);
    let about = item(hwnd, IDC_ABOUT);
    SendMessageW(about, EM_SETCUEBANNER, 1, w(tr("Ex.: um polvo roxo curioso")).as_ptr() as LPARAM);
    limit(about, 200);
    f.y = y - 6;
    f.note(tr("Poses em branco são criadas sozinhas a partir da pose parada (piscar, dormir, andar, pular)."), xr, rw);
    f.control("BUTTON", tr("Salvar e usar"), BUTTON_STYLE, 0, (xr, f.y + 4, rw, BUTTON), IDC_SAVE_MASCOT);
    f.y = (f.y + 4 + BUTTON).max(left_bottom) + 12;
    f.close();
    f.section(tr("Ou peça para a IA desenhar"), None);
    let top = f.row(ROW + 8);
    let width = f.button_width(tr("Criar com IA"));
    let y = top + (ROW + 8 - BUTTON) / 2;
    let ai = f.control("EDIT", "", edit, WS_EX_CLIENTEDGE, (LABEL_X, y, INNER_W - width - 8, BUTTON), IDC_AI_TEXT);
    SendMessageW(ai, EM_SETCUEBANNER, 1, w(tr("Ex.: um polvo roxo de chapéu de marinheiro")).as_ptr() as LPARAM);
    limit(ai, 300);
    f.buttons(&[(tr("Criar com IA"), IDC_AI_GO)], RIGHT, y, BUTTON);
    f.close();
    // O andamento da IA fica logo abaixo do cartão.
    f.y -= GAP - 6;
    f.status(IDC_AI_STATUS, ellipsis | SS_NOPREFIX, (MARGIN + 4, f.y, CONTENT - 2 * MARGIN - 8, 20));

    // --- Plugins
    f.start(Page::Plugins as usize);
    f.section(
        tr("Plugins instalados"),
        Some(tr("Programas que dão novos poderes ao mascote. Plugins novos chegam desligados: ligue só os de quem você confia.")),
    );
    f.list(IDC_PLUGINS, 150, true, &[("Plugin", 0), (tr("Tipo"), 96), (tr("Situação"), 136)]);
    let top = f.row(96);
    f.status(IDC_PLUGIN_INFO, SS_NOPREFIX, (LABEL_X, top + 10, INNER_W, 80));
    let top = f.row(ROW + 8);
    let y = top + (ROW + 8 - BUTTON) / 2;
    let folder_w = f.button_width(tr("Abrir pasta de plugins"));
    f.control("BUTTON", tr("Abrir pasta de plugins"), BUTTON_STYLE, 0, (LABEL_X - 12, y, folder_w, BUTTON), IDC_PLUGIN_FOLDER);
    f.buttons(&[(tr("Atualizar lista"), IDC_PLUGIN_RELOAD), (tr("Testar"), IDC_PLUGIN_TEST)], RIGHT, y, BUTTON);
    // Resposta do teste: só leitura, com rolagem (uma resposta longa aparece inteira),
    // numa caixa em tom neutro.
    let well = RECT { left: LABEL_X, top: f.y, right: RIGHT, bottom: f.y + 84 };
    f.st.sheet.wells.push((f.page, well));
    let result = ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32 | WS_VSCROLL;
    let result = f.control("EDIT", "", result, 0, (LABEL_X + 12, f.y + 8, INNER_W - 16, 68), IDC_PLUGIN_RESULT);
    ShowScrollBar(result, SB_VERT, 0);
    f.y += 84 + 12;
    f.close();
    f.y -= GAP - 4;
    f.note(
        tr("Crie o seu com um .exe ou um script PowerShell: veja o LEIA-ME.txt e o exemplo \"Curiosidades\" na pasta de plugins."),
        MARGIN + 4,
        CONTENT - 2 * MARGIN - 8,
    );

    // --- Galeria
    f.start(Page::Gallery as usize);
    f.section(
        tr("Mascotes da comunidade"),
        Some(tr("Feitos pela comunidade: só desenho e falas, nada que rode no seu PC. Conferidos (SHA-256) antes de instalar.")),
    );
    f.list(IDC_GALLERY, 220, false, &[(tr("Nome"), 0), (tr("Autor"), 160)]);
    let top = f.row(60);
    f.status(IDC_GALLERY_INFO, SS_NOPREFIX, (LABEL_X, top + 10, INNER_W, 40));
    let top = f.row(ROW_TALL + 4);
    let y = top + (ROW_TALL + 4 - BUTTON) / 2;
    let left = f.buttons(&[(tr("Atualizar lista"), IDC_GALLERY_RELOAD), (tr("Instalar"), IDC_GALLERY_INSTALL)], RIGHT, y, BUTTON);
    f.status(IDC_GALLERY_STATUS, SS_NOPREFIX, (LABEL_X, top + 8, left - LABEL_X - 16, 40));
    f.close();

    // --- Desempenho
    let pc = Page::Performance as usize;
    f.start(pc);
    f.section(
        tr("Humor do mascote"),
        Some(tr("Com o processador ou a memória lá em cima por um tempo, ele fica agitado: sua e anda de um lado para o outro.")),
    );
    f.combo(tr("Ficar agitado"), IDC_PC_MOOD);
    let top = f.row(ROW_TALL);
    let y = top + (ROW_TALL - 36) / 2;
    f.status(IDC_PC_NOW, ellipsis, (LABEL_X, y, INNER_W, 20));
    f.paint(Ink::Desc, tr("Ele olha só o total do PC, a cada 6 segundos: quase nada de processador."), (LABEL_X, y + 20, INNER_W, 16));
    f.close();
    f.section(tr("Aliviar o PC"), None);
    f.toggle(IDC_PC_HELPER, tr("Com o PC pesado, sugerir o que fechar"), None);
    f.row(0);
    f.y += 10;
    f.note(
        tr("Dê um petisco a ele (ou clique no balão) e ele mostra os 3 programas seus que mais pesam. Você escolhe: fechar, como clicar no X (o programa ainda pode perguntar se quer salvar), ou deixar com menos prioridade."),
        LABEL_X,
        INNER_W,
    );
    f.note(tr("Nada é fechado sem você escolher. Programas do Windows, de outros usuários ou sem janela nunca aparecem."), LABEL_X, INNER_W);
    f.close();

    // --- Sobre
    f.start(ABOUT);
    f.open();
    let top = f.y + 12;
    let portrait = RECT { left: LABEL_X, top, right: LABEL_X + 56, bottom: top + 56 };
    f.st.sheet.portraits.push((ABOUT, portrait));
    let xt = LABEL_X + 56 + 16;
    f.paint(Ink::Heading, "!StayAlone", (xt, top, RIGHT - xt, 24));
    let blurb = tr("Um mascote em pixel art que faz companhia na área de trabalho: lembra de beber água e fazer pausas, conversa com você e fica levinho, feito direto na API do Windows.");
    let (_, h) = f.measure(Ink::Body, blurb, Some(RIGHT - xt));
    f.paint(Ink::Body, blurb, (xt, top + 26, RIGHT - xt, h));
    f.y = (top + 26 + h).max(portrait.bottom) + 12;
    f.rows = 1;
    let version = crate::update::current();
    let date = release_date();
    for (name, value) in [(tr("Versão"), version), (tr("Lançada em"), date.as_str()), (tr("Autor"), "Atylla Azevedo"), (tr("Licença"), "MIT")] {
        let top = f.row(30);
        f.paint(Ink::Key, name, (LABEL_X, top, VALUE_X - LABEL_X - 16, 30));
        f.paint(Ink::Label, value, (VALUE_X, top, VALUE_W, 30));
    }
    let top = f.row(ROW + 8);
    let y = top + (ROW + 8 - BUTTON) / 2;
    let x = LABEL_X - 12;
    let projects_w = f.button_width(tr("Meus projetos"));
    f.control("BUTTON", tr("Meus projetos"), BUTTON_STYLE, 0, (x, y, projects_w, BUTTON), IDC_ABOUT_PROJECTS);
    let github_w = f.button_width(tr("Página no GitHub"));
    f.control("BUTTON", tr("Página no GitHub"), BUTTON_STYLE, 0, (x + projects_w + 4, y, github_w, BUTTON), IDC_ABOUT_GITHUB);
    f.buttons(&[(tr("Procurar atualização"), IDC_ABOUT_UPDATE)], RIGHT, y, BUTTON);
    f.status(IDC_ABOUT_STATUS, ellipsis, (LABEL_X, f.y - 2, INNER_W, 20));
    f.y += 20;
    f.close();
    // O que protege você, à vista (preenchido por `fill_security` quando a página abre).
    f.section(tr("Segurança e privacidade"), None);
    for (i, name) in SECURITY_ROWS.iter().enumerate() {
        let top = f.row(28);
        f.paint(Ink::Key, tr(name), (LABEL_X, top, VALUE_X - LABEL_X - 16, 28));
        f.status(IDC_SEC_FIRST + i as i32, ellipsis, (VALUE_X, top + 4, VALUE_W, 20));
    }
    f.close();
    f.y -= GAP - 4;
    f.note(tr("Sem telemetria: só as conexões acima usam a rede."), MARGIN + 4, CONTENT - 2 * MARGIN - 8);
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
    if st.page == ABOUT {
        fill_security(hwnd);
    }
    if st.page == Page::Gallery as usize && !st.gallery_loaded && st.busy.is_none() {
        load_gallery(hwnd);
    }
}

// --- segurança e privacidade ------------------------------------------------------

/// O domínio de um endereço, curto: "https://api.openai.com/v1" → "openai.com"
/// (endereço IP ou nome sem ponto, como localhost, fica inteiro, com a porta).
fn site_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let name = host.split(':').next().unwrap_or("");
    if name.bytes().all(|b| b.is_ascii_digit() || b == b'.') || !name.contains('.') {
        return host;
    }
    let dots: Vec<usize> = name.match_indices('.').map(|(i, _)| i).collect();
    if dots.len() < 2 { name } else { &name[dots[dots.len() - 2] + 1..] }
}

/// Commit de origem do build, curto: "58a94be" (ou "58a94be-dirty").
fn short_commit() -> String {
    let commit = env!("STAYALONE_COMMIT");
    let (hash, dirty) = commit.split_once('-').unwrap_or((commit, ""));
    let hash = &hash[..hash.len().min(7)];
    if dirty.is_empty() { hash.into() } else { format!("{hash}-{dirty}") }
}

/// As proteções marcadas no próprio `.exe` em execução (DllCharacteristics do
/// cabeçalho PE já mapeado na memória, sem ler o arquivo de novo).
fn protections() -> String {
    // SAFETY: o módulo principal fica mapeado a vida toda; os offsets são os do PE32+.
    let flags = unsafe {
        let base = GetModuleHandleW(null()) as *const u8;
        let pe = base.add(std::ptr::read_unaligned(base.add(0x3C) as *const u32) as usize);
        std::ptr::read_unaligned(pe.add(24 + 70) as *const u16)
    };
    // "ASLR" só com as duas marcas (endereço aleatório e de alta entropia).
    let names = [(0x0100, "DEP"), (0x0060, "ASLR"), (0x4000, "CFG")];
    let on: Vec<&str> = names.iter().filter(|(bits, _)| flags & bits == *bits).map(|&(_, n)| n).collect();
    let off: Vec<&str> = names.iter().filter(|(bits, _)| flags & bits != *bits).map(|&(_, n)| n).collect();
    if off.is_empty() { on.join(" · ") } else { fill(tr("{} (sem {})"), &[&on.join(" · "), &off.join(", ")]) }
}

/// Preenche o cartão com o estado atual (inclusive o que foi mudado e ainda não salvo).
unsafe fn fill_security(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let sha = st.exe_sha.get_or_insert_with(|| {
        std::env::current_exe().and_then(std::fs::read).map(|b| crate::sha256::hex(&b)).unwrap_or_default()
    });
    let sha = if sha.len() == 64 { format!("{}…{}", &sha[..16], &sha[48..]) } else { tr("não consegui ler").into() };
    let program = short_commit();

    let updates = if is_checked(hwnd, IDC_UPDATES) { tr("Só assinadas, com o seu OK") } else { tr("Desligadas") };

    let key_name = text_of(item(hwnd, IDC_KEYENV));
    let key = match secret::find(&key_name) {
        _ if key_name.trim().is_empty() => tr("Não precisa"),
        Some(KeySource::Vault) => tr("Guardada no Windows"),
        Some(KeySource::Environment) => tr("Em variável de ambiente"),
        None => tr("Nenhuma"),
    };

    let enabled = |id: &str| st.draft.plugins.iter().any(|e| e.id == id);
    let mut net = Vec::new();
    if is_checked(hwnd, IDC_UPDATES) {
        net.push("GitHub".to_string());
    }
    if enabled(plugins::NATIVE_ID) {
        let base = text_of(item(hwnd, IDC_BASE));
        net.push(fill(tr("IA ({})"), &[&site_of(&base)]));
    }
    net.push(tr("Galeria").to_string());

    let external: Vec<&Plugin> = st.plugins.iter().filter(|p| !p.is_native()).collect();
    let on = external.iter().filter(|p| enabled(&p.id)).count();
    let online = external.iter().filter(|p| enabled(&p.id) && p.internet).count();
    let with_folders = st.draft.plugins.iter().filter(|e| !e.folders.is_empty()).count();
    let plugins = if on == 0 {
        tr("Nenhum ligado").to_string()
    } else {
        let mut parts = vec![fill(tr("{} ligado(s), isolados"), &[&on])];
        if online > 0 {
            parts.push(fill(tr("{} com internet"), &[&online]));
        }
        if with_folders > 0 {
            parts.push(fill(tr("{} com pastas"), &[&with_folders]));
        }
        parts.join(" · ")
    };

    let values = [program, sha, protections(), updates.into(), key.into(), net.join(" · "), plugins];
    for (i, value) in values.iter().enumerate() {
        set_text(hwnd, IDC_SEC_FIRST + i as i32, value);
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
    set_checked_box(hwnd, IDC_PC_HELPER, st.draft.pc_helper);
    let moods: Vec<String> = Sensitivity::ALL.iter().map(|m| m.label().to_string()).collect();
    fill_combo(hwnd, IDC_PC_MOOD, &moods, Sensitivity::ALL.iter().position(|m| *m == st.draft.pc_mood).unwrap_or(2));
    let now = match resources::last_cpu() {
        Some(cpu) => fill(tr("Agora: processador em {}% · memória em {}%"), &[&cpu, &resources::ram_now()]),
        None => fill(tr("Agora: memória em {}%"), &[&resources::ram_now()]),
    };
    set_text(hwnd, IDC_PC_NOW, &now);
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
        let access = if p.is_native() {
            tr("Parte do próprio !StayAlone.").to_string()
        } else if !p.folders.is_empty() {
            let asked: Vec<String> = p.folders.iter().map(|(raw, write)| format!("{raw}{}", if *write { tr(" (grava)") } else { "" })).collect();
            let net = if p.internet { tr("Pede internet.") } else { tr("Sem internet.") };
            fill(tr("Isolado (sandbox), com acesso a: {}. {}"), &[&asked.join(", "), &net])
        } else if p.internet {
            tr("Isolado (sandbox): não alcança seus arquivos. Pede internet.").to_string()
        } else {
            tr("Isolado (sandbox): não alcança seus arquivos nem a internet.").to_string()
        };
        format!("{}\n{} {file}{when}\n{access}{warning}", p.about, tr("Arquivo:"))
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
        return Some(Enabled { id: plugin.id.clone(), ..Default::default() });
    }
    // A impressão digital sai antes da pergunta: a que você vê é a que fica aprovada.
    let Some(fingerprint) = plugin.fingerprint() else {
        info(hwnd, tr("Não consegui ler o arquivo do plugin."));
        return None;
    };
    let folders = match plugin.requested_folders() {
        Ok(folders) => folders,
        Err(e) => {
            info(hwnd, &fill(tr("O plugin \"{}\" pede uma pasta que o !StayAlone não libera: {}"), &[&plugin.name, &e]));
            return None;
        }
    };
    let labels: Vec<String> = folders.iter().map(Access::label).collect();
    let mut rows = vec![ask::Row { icon: ask::icon::LOCK, text: tr("Isolado do resto do seu PC"), warn: false }];
    rows.extend(labels.iter().map(|l| ask::Row { icon: ask::icon::FOLDER, text: l, warn: true }));
    rows.push(if plugin.internet {
        ask::Row { icon: ask::icon::GLOBE, text: tr("Usa a internet"), warn: true }
    } else {
        ask::Row { icon: ask::icon::GLOBE, text: tr("Sem internet"), warn: false }
    });
    let note = fill(tr("Só ligue se confiar em quem fez · SHA-256 {}…{}"), &[&&fingerprint[..8], &&fingerprint[56..]]);
    let question = ask::Question {
        title: &fill(tr("Ligar \"{}\"?"), &[&plugin.name]),
        subtitle: &plugin.about,
        rows: &rows,
        note: &note,
        yes: tr("Ligar"),
        no: tr("Agora não"),
    };
    if !ask::ask(hwnd, &question) {
        return None;
    }
    Some(Enabled { id: plugin.id.clone(), fingerprint, internet: plugin.internet, folders })
}

/// Mostra o resultado do teste; a barra de rolagem só aparece se o texto não couber.
unsafe fn set_result(hwnd: HWND, text: &str) {
    const VISIBLE_LINES: isize = 4;
    let control = item(hwnd, IDC_PLUGIN_RESULT);
    ShowScrollBar(control, SB_VERT, 0);
    SetWindowTextW(control, w(text).as_ptr());
    if SendMessageW(control, EM_GETLINECOUNT, 0, 0) > VISIBLE_LINES {
        ShowScrollBar(control, SB_VERT, 1);
    }
}

/// O que o plugin escreveu, inteiro (até um limite), linha a linha e sem caracteres de
/// controle, para a caixa do teste.
fn test_output(text: &str) -> String {
    use crate::win::clean_line;
    const MAX: usize = 4000;
    let lines: Vec<String> = text.lines().map(|l| clean_line(l, MAX)).filter(|l| !l.is_empty()).collect();
    let mut out = lines.join("\r\n");
    if out.chars().count() > MAX {
        out = out.chars().take(MAX).collect::<String>() + "…";
    }
    out
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
                set_result(hwnd, tr("Ligue o plugin para testar."));
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
            set_result(hwnd, &fill(tr("Testando \"{}\"..."), &[&plugin.name]));
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
            set_result(hwnd, tr("✓ Rodou, mas não tinha nada para falar desta vez."));
        }
        (Some(Busy::Plugin), Ok(text)) => set_result(hwnd, &fill(tr("✓ Respondeu: {}"), &[&test_output(&text)])),
        (Some(Busy::Plugin), Err(e)) => set_result(hwnd, &format!("✗ {}", test_output(&e))),
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
const TOGGLES: [i32; 8] =
    [IDC_AUTOSTART, IDC_UPDATES, IDC_MEETINGS, IDC_MEMORY, IDC_FOCUS_QUIET, IDC_HOTKEY, IDC_ACCESSORIES, IDC_PC_HELPER];

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
    st.draft.pc_helper = is_checked(hwnd, IDC_PC_HELPER);
    st.draft.pc_mood = Sensitivity::ALL[combo_index(hwnd, IDC_PC_MOOD).min(Sensitivity::ALL.len() - 1)];
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
    st.saved = true;
    mailbox::post(st.owner, WM_SETTINGS_APPLY, Draft { config: st.draft.clone(), autostart });
    DestroyWindow(hwnd);
}

// --- pintura: barra lateral, título da página e cartões ------------------------------

/// Item `i` da navegação, em pixels.
fn nav_rect(i: usize, dpi: u32) -> RECT {
    let top = NAV_TOP + i as i32 * NAV_STEP;
    RECT { left: scale(12, dpi), top: scale(top, dpi), right: scale(SIDEBAR - 12, dpi), bottom: scale(top + NAV_H, dpi) }
}

/// O item "Sobre", no rodapé da barra lateral.
fn about_rect(dpi: u32) -> RECT {
    RECT { left: scale(12, dpi), top: scale(HEIGHT - 16 - NAV_H, dpi), right: scale(SIDEBAR - 12, dpi), bottom: scale(HEIGHT - 16, dpi) }
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
    let center = DT_CENTER | DT_VCENTER | DT_SINGLELINE;
    let left = DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS;
    // Tudo num bitmap e depois de uma vez na tela: sem piscar.
    let mut c = Canvas::new(client.right.max(1), client.bottom.max(1));
    let (width, height) = (c.width, c.height);
    c.fill(0, 0, width, height, argb(theme::bg()));

    // Barra lateral: quem é o mascote (muda ao vivo, como no painel) e a navegação.
    let side = s(SIDEBAR);
    c.fill(0, 0, side, height, argb(theme::sidebar()));
    c.fill(side - 1, 0, 1, height, argb(theme::border()));
    let (tx, ty, tile) = (s(16), s(18), s(48));
    c.round_rect(tx, ty, tile, tile, s(theme::size::CARD_RADIUS), argb(theme::accent_soft()));
    c.sprite_fit(tx + (tile - s(32)) / 2, ty + (tile - s(32)) / 2, s(32), &st.preview.1);
    let nx = tx + tile + s(12);
    c.text(st.heading, &st.preview.0, RECT { left: nx, top: ty + s(4), right: side - s(12), bottom: ty + s(26) }, theme::text(), left);
    c.text(st.small, tr("Configurações"), RECT { left: nx, top: ty + s(26), right: side - s(12), bottom: ty + s(44) }, theme::muted(), left);
    let version = fill(tr("Sobre · v{}"), &[&crate::update::current()]);
    let items = PAGES.iter().map(|&(_, glyph, label)| (glyph, tr(label).to_string())).chain(std::iter::once((icon::INFO, version)));
    for (i, (glyph, label)) in items.enumerate() {
        let r = if i == ABOUT { about_rect(st.dpi) } else { nav_rect(i, st.dpi) };
        let (x, y, w, h) = (r.left, r.top, r.right - r.left, r.bottom - r.top);
        let selected = i == st.page;
        if selected {
            c.round_rect(x, y, w, h, s(theme::size::RADIUS), argb(theme::card()));
            c.round_rect(x, y + (h - s(16)) / 2, s(3), s(16), s(2), argb(theme::accent()));
        } else if st.nav_hover == Some(i) {
            c.round_rect(x, y, w, h, s(theme::size::RADIUS), argb(theme::hover()));
        }
        let (icon_ink, font) = if selected { (theme::accent(), st.bold) } else { (theme::muted(), st.font) };
        c.text(st.icons, &glyph.to_string(), RECT { left: x + s(12), top: y, right: x + s(36), bottom: y + h }, icon_ink, center);
        c.text(font, &label, RECT { left: x + s(44), top: y, right: x + w - s(8), bottom: y + h }, theme::text(), left);
    }

    // Cabeçalho (título da página; Salvar e Cancelar são botões à direita).
    let x0 = side;
    let title = RECT { left: x0 + s(MARGIN), top: s(16), right: x0 + s(CONTENT - MARGIN - 220), bottom: s(16 + BUTTON) };
    let page_title = if st.page == ABOUT { tr("Sobre") } else { tr(PAGES[st.page].2) };
    c.text(st.title, page_title, title, theme::text(), left);

    // Cartões, divisórias, caixas e textos da página, atrás dos controles.
    let page = st.page;
    let at = |r: &RECT| (x0 + s(r.left), s(r.top), s(r.right - r.left), s(r.bottom - r.top));
    let sheet = &st.sheet;
    for (_, r) in sheet.cards.iter().filter(|(p, _)| *p == page) {
        c.card(at(r), s(theme::size::CARD_RADIUS), argb(theme::card()), argb(theme::border()));
    }
    for (_, r) in sheet.lines.iter().filter(|(p, _)| *p == page) {
        let (x, y, w, _) = at(r);
        c.fill(x, y, w, 1, argb(theme::divider()));
    }
    for (_, r) in sheet.wells.iter().filter(|(p, _)| *p == page) {
        let (x, y, w, h) = at(r);
        c.round_rect(x, y, w, h, s(theme::size::RADIUS), argb(theme::soft()));
    }
    for (_, r) in sheet.portraits.iter().filter(|(p, _)| *p == page) {
        let (x, y, w, h) = at(r);
        c.round_rect(x, y, w, h, s(theme::size::CARD_RADIUS), argb(theme::accent_soft()));
        c.sprite_fit(x + (w - s(48)) / 2, y + (h - s(48)) / 2, s(48), &st.preview.1);
    }
    let wrap = DT_LEFT | DT_TOP | DT_WORDBREAK;
    for text in sheet.texts.iter().filter(|t| t.page == page) {
        let (x, y, w, h) = at(&text.rect);
        let rect = RECT { left: x, top: y, right: x + w, bottom: y + h };
        let (ink, flags) = match text.ink {
            Ink::Section | Ink::Label | Ink::Heading => (theme::text(), left),
            Ink::Key | Ink::Unit | Ink::Desc => (theme::muted(), left),
            Ink::Note | Ink::Body => (theme::muted(), wrap),
        };
        c.text(font_of(st, text.ink), &text.text, rect, ink, flags);
    }
    c.blit(dc, 0, 0);
    EndPaint(hwnd, &ps);
}

/// O papel de cada botão: uma ação principal (laranja) por área, ações leves só em
/// texto e as que apagam em vermelho.
fn button_kind(id: i32) -> ui::Kind {
    match id {
        IDOK | IDC_ADD | IDC_TEST | IDC_SAVE_MASCOT | IDC_AI_GO | IDC_PLUGIN_TEST | IDC_GALLERY_INSTALL | IDC_ABOUT_UPDATE => {
            ui::Kind::Primary
        }
        IDC_MODS | IDC_GETKEY | IDC_PLUGIN_FOLDER | IDC_ABOUT_PROJECTS | IDC_ABOUT_GITHUB => ui::Kind::Ghost,
        IDC_REMOVE | IDC_KEY_REMOVE | IDC_MEMORY_CLEAR => ui::Kind::Danger,
        _ => ui::Kind::Secondary,
    }
}

unsafe fn draw_button(hwnd: HWND, di: &DRAWITEMSTRUCT) {
    let Some(st) = state(hwnd) else { return };
    let id = di.CtlID as i32;
    let style = ui::ButtonStyle {
        kind: button_kind(id),
        // Os cantos mostram o fundo de onde o botão está: cartão ou fundo da janela.
        background: if matches!(id, IDOK | IDCANCEL | IDC_MODS) { theme::bg() } else { theme::card() },
        font: st.font,
        bold: st.bold,
        small: Some(st.small),
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
        // Textos que mudam ficam sobre os cartões; estados em cinza, o resto na cor do texto.
        // O resultado do teste de plugin (campo só de leitura) fica na caixa neutra.
        WM_CTLCOLORSTATIC => {
            let Some(st) = state(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
            let dc = wp as HDC;
            SetBkMode(dc, TRANSPARENT as _);
            let id = GetDlgCtrlID(lp as HWND);
            let ink = match id {
                IDC_KEY_STATUS | IDC_TEST_RESULT | IDC_MEMORY_STATUS | IDC_AI_STATUS | IDC_GALLERY_STATUS | IDC_ABOUT_STATUS => theme::muted(),
                _ => theme::text(),
            };
            SetTextColor(dc, theme::colorref(ink));
            if id == IDC_PLUGIN_RESULT {
                SetBkColor(dc, theme::colorref(theme::soft()));
                return st.soft_brush as LRESULT;
            }
            if id == IDC_AI_STATUS {
                SetBkColor(dc, theme::colorref(theme::bg()));
                return st.bg_brush as LRESULT;
            }
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
            if let Some(st) = state(hwnd).filter(|st| !st.saved) {
                plugins::sync_folders(&st.draft.plugins, &st.saved_plugins);
            }
            0
        }
        WM_NCDESTROY => {
            mailbox::discard(hwnd); // resposta da IA que chegou tarde demais
            let st = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
            if !st.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let st = Box::from_raw(st);
                for object in [st.font, st.bold, st.title, st.heading, st.small, st.icons, st.card_brush, st.soft_brush, st.bg_brush] {
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
    fn sites_are_short() {
        assert_eq!(site_of("https://generativelanguage.googleapis.com/v1beta/openai/"), "googleapis.com");
        assert_eq!(site_of("https://api.openai.com/v1"), "openai.com");
        assert_eq!(site_of("http://localhost:11434/v1"), "localhost:11434");
        assert_eq!(site_of("http://127.0.0.1:8080/v1"), "127.0.0.1:8080");
        assert_eq!(site_of("https://example.com"), "example.com");
        assert_eq!(site_of(""), "");
    }
}
