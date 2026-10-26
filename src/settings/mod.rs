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
mod history_page;
mod about;
mod gallery_page;
mod reminders;
mod chat_page;
mod plugins_page;
mod maker_page;
mod paint;

use editor::{canvas_proc, palette_proc, redraw_editor, CELL, SWATCH, SWATCH_GAP};
use about::*;
use gallery_page::*;
use history_page::*;
use reminders::*;
use chat_page::*;
use plugins_page::*;
use maker_page::*;
use paint::*;

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
    History,
    /// Fora da lista: aberta pelo item "Sobre" no rodapé da barra lateral.
    About,
}

/// Índice da página "Sobre" (depois das páginas da navegação).
const ABOUT: usize = Page::About as usize;
/// Links da página "Sobre".
const PROJECTS_URL: &str = "https://4tyllal.github.io/#projects";
const GITHUB_URL: &str = "https://github.com/4TyllaL/notStayAlone";

const PAGES: [(Page, char, &str); 8] = [
    (Page::General, icon::HOME, "Geral"),
    (Page::Reminders, icon::BELL, "Lembretes"),
    (Page::Chat, icon::CHAT, "Conversa"),
    (Page::Maker, icon::PALETTE, "Criar mascote"),
    (Page::Plugins, icon::PUZZLE, "Plugins"),
    (Page::Gallery, icon::SHOP, "Galeria"),
    (Page::Performance, icon::GAUGE, "Desempenho"),
    (Page::History, icon::CALENDAR, "Histórico"),
];
const SPEEDS: [&str; 4] = ["Devagar", "Normal", "Rápido", "Muito rápido"];

/// Medidas em pontos de 96 DPI. A área de conteúdo usa coordenadas próprias: x a
/// partir da barra lateral, y a partir do topo da janela.
const SIDEBAR: i32 = 200;
const CONTENT: i32 = 620;
const HEIGHT: i32 = 664;
/// Margem do conteúdo (dos lados e do título) e onde começa a primeira seção.
const MARGIN: i32 = 24;
const TOP: i32 = 60;
/// Folga interna dos cartões, dos lados e em cima/embaixo das linhas.
const PAD: i32 = 16;
const VPAD: i32 = 4;
/// Espaço entre um cartão e o título da seção seguinte.
const GAP: i32 = 12;
/// Altura de uma linha (rótulo + controle) e de uma linha com descrição.
const ROW: i32 = 36;
const ROW_TALL: i32 = 44;
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
const IDC_SUMMARY: i32 = 185;
const IDC_HISTORY: i32 = 186;
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
    /// Onde termina o conteúdo de cada página (em pontos de 96 DPI); o teste confere que cabe.
    bottoms: Vec<(usize, i32)>,
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

    /// Guarda onde a página atual terminou (com o espaço de baixo do último cartão).
    fn finish(&mut self) {
        if self.page != GLOBAL {
            self.st.sheet.bottoms.push((self.page, self.y));
        }
    }

    fn start(&mut self, page: usize) {
        self.finish();
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
    f.list(IDC_LIST, 176, true, &[(tr("Lembrete"), 0), (tr("Quando"), 96)]);
    // Editar: texto e intervalo numa linha, as ações embaixo (a principal na ponta).
    let top = f.row(ROW);
    let y = top + (ROW - CONTROL) / 2 + 4;
    let x = f.unit(tr("Texto"), LABEL_X, y) + 12;
    let text = f.field(edit, (x, y, 260), IDC_TEXT);
    SendMessageW(text, EM_SETCUEBANNER, 1, w(tr("Ex.: Conferir o e-mail")).as_ptr() as LPARAM);
    limit(text, MAX_REMINDER_TEXT);
    let x = f.unit(tr("quando"), x + 260 + 12, y) + 8;
    let minutes = f.field(edit, (x, y, 56), IDC_MINUTES);
    SetWindowTextW(minutes, w("30").as_ptr());
    limit(minutes, 5);
    f.unit(tr("min ou 14:30"), x + 64, y);
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
    let y = f.labeled(tr("Resumo do dia"), None);
    let summary = f.field(number, (VALUE_X, y, 56), IDC_SUMMARY);
    limit(summary, 2);
    f.unit(tr("h (fim do expediente)"), VALUE_X + 64, y);
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

    // --- Histórico
    f.start(Page::History as usize);
    f.section(tr("Seus últimos dias"), Some(tr("O que ele anotou de cada dia: água, pausas e quanto tempo vocês ficaram juntos. Fica só no seu PC.")));
    f.list(IDC_HISTORY, 400, false, &[(tr("Dia"), 0), (tr("Água"), 70), (tr("Pausas"), 210), (tr("Juntos"), 280)]);
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
    f.finish();
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
    if st.page == Page::History as usize {
        fill_history(hwnd);
    }
    if st.page == Page::Gallery as usize && !st.gallery_loaded && st.busy.is_none() {
        load_gallery(hwnd);
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
    let Some(summary) = number(hwnd, IDC_SUMMARY, 0..=23, tr("Resumo do dia")) else {
        return show_page(hwnd, Page::Reminders as usize);
    };
    if !save_chat(hwnd) {
        return;
    }
    st.draft.companion.summary_hour = summary;
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

    /// Abre a janela de verdade (escondida pelo destroy logo em seguida) e confere que o
    /// conteúdo de cada página cabe na altura da janela, nos dois idiomas.
    #[test]
    fn every_page_fits_the_window() {
        use crate::config::Language;
        unsafe {
            for language in [Language::Portuguese, Language::English] {
                crate::lang::set(language);
                open(null_mut(), &Config::load(), null_mut(), Page::General);
                let hwnd = OPEN.with(Cell::get);
                assert!(!hwnd.is_null(), "a janela de Configurações não abriu");
                let bottoms = state(hwnd).map(|st| st.sheet.bottoms.clone()).unwrap_or_default();
                DestroyWindow(hwnd);
                assert_eq!(bottoms.len(), 9, "esperava o fim de cada uma das 9 páginas");
                for (page, bottom) in bottoms {
                    assert!(bottom <= HEIGHT, "a página {page} termina em {bottom}, mas a janela tem {HEIGHT}");
                }
            }
        }
    }
}
