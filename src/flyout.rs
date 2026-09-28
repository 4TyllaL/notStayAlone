//! Painel do mascote: abre no clique direito (no mascote ou no ícone da bandeja).
//! Janela em camadas desenhada à mão — cartão com sombra, atalhos grandes, troca
//! de mascote pelo desenho, seletor de tamanho e interruptores. Só existe enquanto
//! está aberto; ao escolher algo, manda a `Action` para o app pelo `mailbox`.

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
        Controls::WM_MOUSELEAVE,
        Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_ESCAPE},
        WindowsAndMessaging::*,
    },
};

use crate::{
    gfx::Canvas,
    mailbox, system,
    theme::{self, argb, icon},
    win::{ui_font, w},
};

/// Escolha feita no painel (`Action` no `mailbox`).
pub const WM_FLYOUT_ACTION: u32 = WM_APP + 10;

const WIDTH: i32 = 300;
/// Espaço em volta do cartão para a sombra.
const MARGIN: i32 = 14;
const PAD: i32 = 14;
const RADIUS: i32 = 10;
const FADE_TIMER: usize = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Feed,
    Ball,
    Chat,
    Pomodoro,
    Mascot(usize),
    /// Mais mascotes do que cabem: abre as configurações.
    MoreMascots,
    NewMascot,
    Size(usize),
    Silence,
    Hide,
    Reminders,
    Summary,
    Settings,
    /// Baixar e instalar a versão nova.
    Update,
    Quit,
}

/// Um mascote para escolher: nome e o sprite parado (16×16, 0xAARRGGBB).
pub struct Choice {
    pub name: String,
    pub pixels: Vec<u32>,
}

/// Tudo o que o painel mostra, montado pelo app na hora de abrir.
pub struct Model {
    pub name: String,
    pub pixels: Vec<u32>,
    pub hearts: u32,
    pub stats: String,
    pub mascots: Vec<Choice>,
    pub current: usize,
    pub sizes: [&'static str; 4],
    pub size: usize,
    pub ball_out: bool,
    pub pomodoro: bool,
    pub silenced: bool,
    pub hidden: bool,
    pub reminders_on: usize,
    /// Versão nova disponível (mostra a faixa "Atualizar").
    pub update: Option<String>,
}

/// Lado direito de uma linha.
#[derive(Clone, PartialEq, Debug)]
enum Right {
    Nothing,
    Chevron,
    Switch(bool),
    Note(String),
}

#[derive(Clone, PartialEq, Debug)]
enum Part {
    Panel,
    Header,
    Stats,
    /// Faixa de destaque (versão nova).
    Banner(String),
    Tile { glyph: char, label: &'static str, active: bool },
    Caption(&'static str),
    Mascot(usize),
    Glyph(char),
    Track,
    Segment { label: &'static str, selected: bool },
    Divider,
    Row { glyph: char, label: &'static str, right: Right, danger: bool },
}

#[derive(Clone)]
struct Item {
    rect: RECT,
    part: Part,
    action: Option<Action>,
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> RECT {
    RECT { left: x, top: y, right: x + w, bottom: y + h }
}

/// Onde fica cada parte do painel (em pixels, para o `dpi`), e a altura total.
fn layout(m: &Model, dpi: u32) -> (Vec<Item>, i32) {
    let s = |v: i32| v * dpi as i32 / 96;
    let (margin, pad, width) = (s(MARGIN), s(PAD), s(WIDTH));
    let (x0, inner) = (margin + pad, width - 2 * pad);
    let mut items = Vec::new();
    let mut add = |r: RECT, part: Part, action: Option<Action>| items.push(Item { rect: r, part, action });
    let mut y = margin + pad;

    add(rect(x0, y, inner, s(48)), Part::Header, None);
    y += s(54);
    add(rect(x0, y, inner, s(20)), Part::Stats, None);
    y += s(30);
    if let Some(version) = &m.update {
        add(rect(x0, y, inner, s(38)), Part::Banner(format!("Versão {version} disponível — atualizar")), Some(Action::Update));
        y += s(38) + s(12);
    }

    // Atalhos grandes.
    let tiles = [
        (icon::FOOD, "Petisco", false, Action::Feed),
        (icon::GAME, if m.ball_out { "Guardar" } else { "Bolinha" }, m.ball_out, Action::Ball),
        (icon::CHAT, "Conversar", false, Action::Chat),
        (icon::TIMER, if m.pomodoro { "Parar" } else { "Foco" }, m.pomodoro, Action::Pomodoro),
    ];
    let gap = s(8);
    let tile_w = (inner - 3 * gap) / 4;
    for (i, (glyph, label, active, action)) in tiles.into_iter().enumerate() {
        let r = rect(x0 + i as i32 * (tile_w + gap), y, tile_w, s(64));
        add(r, Part::Tile { glyph, label, active }, Some(action));
    }
    y += s(64) + s(16);

    // Trocar de mascote clicando no desenho.
    add(rect(x0, y, inner, s(18)), Part::Caption("Mascote"), None);
    y += s(24);
    let slot = s(46);
    let slot_gap = (inner - 5 * slot) / 4;
    let shown = if m.mascots.len() > 5 { 4 } else { m.mascots.len() };
    for i in 0..shown {
        add(rect(x0 + i as i32 * (slot + slot_gap), y, slot, slot), Part::Mascot(i), Some(Action::Mascot(i)));
    }
    let extra = match m.mascots.len() {
        n if n > 5 => Some((icon::MORE, Action::MoreMascots)),
        n if n < 5 => Some((icon::ADD, Action::NewMascot)),
        _ => None,
    };
    if let Some((glyph, action)) = extra {
        add(rect(x0 + shown as i32 * (slot + slot_gap), y, slot, slot), Part::Glyph(glyph), Some(action));
    }
    y += slot + s(16);

    // Tamanho: seletor segmentado.
    add(rect(x0, y, s(90), s(30)), Part::Caption("Tamanho"), None);
    let track_w = s(176);
    let track = rect(x0 + inner - track_w, y, track_w, s(30));
    add(track, Part::Track, None);
    let seg_w = (track_w - s(4)) / 4;
    for (i, label) in m.sizes.into_iter().enumerate() {
        let r = rect(track.left + s(2) + i as i32 * seg_w, y + s(2), seg_w, s(26));
        add(r, Part::Segment { label, selected: i == m.size }, Some(Action::Size(i)));
    }
    y += s(30) + s(12);

    add(rect(x0, y, inner, 1), Part::Divider, None);
    y += s(8);
    let row_h = s(36);
    let reminders = match m.reminders_on {
        0 => "nenhum ligado".to_string(),
        1 => "1 ligado".to_string(),
        n => format!("{n} ligados"),
    };
    let rows = [
        (icon::MUTE, "Silenciar por 1 hora", Right::Switch(m.silenced), Action::Silence),
        (icon::HIDE, "Esconder o mascote", Right::Switch(m.hidden), Action::Hide),
        (icon::BELL, "Lembretes", Right::Note(reminders), Action::Reminders),
        (icon::CALENDAR, "Resumo do dia", Right::Nothing, Action::Summary),
        (icon::SETTINGS, "Configurações", Right::Chevron, Action::Settings),
    ];
    for (glyph, label, right, action) in rows {
        add(rect(x0 - s(6), y, inner + s(12), row_h), Part::Row { glyph, label, right, danger: false }, Some(action));
        y += row_h;
    }
    y += s(4);
    add(rect(x0, y, inner, 1), Part::Divider, None);
    y += s(5);
    let quit = Part::Row { glyph: icon::POWER, label: "Sair", right: Right::Nothing, danger: true };
    add(rect(x0 - s(6), y, inner + s(12), row_h), quit, Some(Action::Quit));
    y += row_h + pad - s(4);

    let height = y + margin;
    items.insert(0, Item { rect: rect(margin, margin, width, y - margin), part: Part::Panel, action: None });
    (items, height)
}

struct Fonts {
    title: HFONT,
    body: HFONT,
    small: HFONT,
    icon: HFONT,
    icon_big: HFONT,
}

impl Fonts {
    unsafe fn new(dpi: u32) -> Fonts {
        let s = |v: i32| v * dpi as i32 / 96;
        Fonts {
            title: ui_font(s(16), FW_SEMIBOLD),
            body: ui_font(s(13), FW_NORMAL),
            small: ui_font(s(12), FW_NORMAL),
            icon: theme::icon_font(s(15)),
            icon_big: theme::icon_font(s(20)),
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        for font in [self.title, self.body, self.small, self.icon, self.icon_big] {
            unsafe { DeleteObject(font) };
        }
    }
}

struct Flyout {
    owner: HWND,
    model: Model,
    dpi: u32,
    items: Vec<Item>,
    hover: Option<usize>,
    hovered_mascot: Option<usize>,
    canvas: Canvas,
    fonts: Fonts,
    alpha: u8,
}

thread_local! {
    static OPEN: Cell<HWND> = const { Cell::new(null_mut()) };
}

/// Abre o painel perto de `anchor` (o cursor). Se já estiver aberto, fecha.
pub unsafe fn open(owner: HWND, model: Model, anchor: POINT) {
    let existing = OPEN.with(Cell::get);
    if !existing.is_null() {
        DestroyWindow(existing);
        return;
    }
    let hinstance = GetModuleHandleW(null());
    let class = w("StayAloneFlyout");
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: hinstance,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        lpszClassName: class.as_ptr(),
        ..zeroed()
    };
    RegisterClassExW(&wc);

    let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForSystem();
    let (items, height) = layout(&model, dpi);
    let width = (WIDTH + 2 * MARGIN) * dpi as i32 / 96;
    let work = system::bounds_at(anchor);
    let x = (anchor.x - width / 2).clamp(work.left as i32, (work.right as i32 - width).max(work.left as i32));
    let y = (anchor.y - height).clamp(work.top as i32, (work.floor as i32 - height).max(work.top as i32));
    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        class.as_ptr(),
        w("!StayAlone").as_ptr(),
        WS_POPUP,
        x,
        y,
        width,
        height,
        null_mut(),
        null_mut(),
        hinstance,
        null(),
    );
    if hwnd.is_null() {
        return;
    }
    let flyout = Box::new(Flyout {
        owner,
        model,
        dpi,
        items,
        hover: None,
        hovered_mascot: None,
        canvas: Canvas::new(width, height),
        fonts: Fonts::new(dpi),
        alpha: 0,
    });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(flyout) as isize);
    OPEN.with(|o| o.set(hwnd));
    if let Some(f) = flyout_of(hwnd) {
        f.paint();
        f.canvas.present_faded(hwnd, Some((x, y)), 0);
    }
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    SetTimer(hwnd, FADE_TIMER, 15, None); // aparece em ~75 ms
}

unsafe fn flyout_of<'a>(hwnd: HWND) -> Option<&'a mut Flyout> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Flyout).as_mut()
}

impl Flyout {
    fn hit(&self, x: i32, y: i32) -> Option<usize> {
        self.items.iter().position(|it| {
            it.action.is_some() && x >= it.rect.left && x < it.rect.right && y >= it.rect.top && y < it.rect.bottom
        })
    }

    unsafe fn paint(&mut self) {
        self.canvas.pixels().fill(0);
        self.hovered_mascot = self.hover.and_then(|i| match self.items[i].part {
            Part::Mascot(m) => Some(m),
            _ => None,
        });
        let items = std::mem::take(&mut self.items);
        for (i, item) in items.iter().enumerate() {
            self.paint_item(item, self.hover == Some(i));
        }
        self.items = items;
    }

    unsafe fn paint_item(&mut self, item: &Item, hover: bool) {
        let r = item.rect;
        let (x, y, w, h) = (r.left, r.top, r.right - r.left, r.bottom - r.top);
        let dpi = self.dpi as i32;
        let s = |v: i32| v * dpi / 96;
        let center = DT_CENTER | DT_VCENTER | DT_SINGLELINE;
        let left = DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS;
        let f = &self.fonts;
        let (title, body, small, icon_font, icon_big) = (f.title, f.body, f.small, f.icon, f.icon_big);
        let c = &mut self.canvas;
        match &item.part {
            Part::Panel => {
                c.shadow((x, y + s(3), w, h), s(RADIUS), s(MARGIN) - s(2), 0.28);
                c.card((x, y, w, h), s(RADIUS), argb(theme::card()), argb(theme::border()));
            }
            Part::Header => {
                c.round_rect(x, y, s(48), s(48), s(10), argb(theme::accent_soft()));
                let off = (s(48) - s(32)) / 2;
                c.sprite_fit(x + off, y + off, s(32), &self.model.pixels);
                let tx = x + s(60);
                c.text(title, &self.model.name, rect(tx, y + s(2), w - s(60), s(24)), theme::text(), left);
                // Um coração por vez: a fonte de ícones não tem o caractere de espaço.
                for i in 0..5 {
                    let (glyph, ink) =
                        if i < self.model.hearts { (icon::HEART_FULL, theme::heart()) } else { (icon::HEART_EMPTY, theme::disabled()) };
                    let hx = tx + i as i32 * s(19);
                    c.text(icon_font, &glyph.to_string(), rect(hx, y + s(28), s(18), s(18)), ink, left);
                }
            }
            Part::Stats => c.text(small, &self.model.stats, r, theme::muted(), left),
            Part::Banner(text) => {
                let fill = if hover { theme::accent_dark() } else { theme::accent() };
                c.round_rect(x, y, w, h, s(10), argb(fill));
                c.text(icon_font, &icon::DOWNLOAD.to_string(), rect(x + s(8), y, s(24), h), theme::on_accent(), center);
                c.text(body, text, rect(x + s(38), y, w - s(46), h), theme::on_accent(), left);
            }
            Part::Tile { glyph, label, active } => {
                let bg = match (active, hover) {
                    (true, false) => theme::accent(),
                    (true, true) => theme::accent_dark(),
                    (false, true) => theme::hover(),
                    (false, false) => theme::soft(),
                };
                c.round_rect(x, y, w, h, s(10), argb(bg));
                let (ink, accent) = if *active { (theme::on_accent(), theme::on_accent()) } else { (theme::text(), theme::accent()) };
                c.text(icon_big, &glyph.to_string(), rect(x, y + s(8), w, s(26)), accent, center);
                c.text(small, label, rect(x + s(2), y + s(38), w - s(4), s(18)), ink, center);
            }
            Part::Caption(text) => {
                // Passando o mouse num mascote, a legenda mostra o nome dele.
                let name = self.hovered_mascot.and_then(|i| self.model.mascots.get(i)).filter(|_| *text == "Mascote");
                let caption = name.map_or(text.to_string(), |m| format!("Mascote · {}", m.name));
                c.text(small, &caption, r, theme::muted(), left)
            }
            Part::Mascot(i) => {
                let current = *i == self.model.current;
                let bg = if hover { theme::hover() } else { theme::soft() };
                if current {
                    c.round_rect(x, y, w, h, s(10), argb(theme::accent()));
                    c.round_rect(x + s(2), y + s(2), w - s(4), h - s(4), s(8), argb(theme::accent_soft()));
                } else {
                    c.round_rect(x, y, w, h, s(10), argb(bg));
                }
                if let Some(choice) = self.model.mascots.get(*i) {
                    c.sprite_fit(x + (w - s(32)) / 2, y + (h - s(32)) / 2, s(32), &choice.pixels);
                }
            }
            Part::Glyph(glyph) => {
                c.round_rect(x, y, w, h, s(10), argb(if hover { theme::hover() } else { theme::soft() }));
                c.text(icon_font, &glyph.to_string(), r, theme::muted(), center);
            }
            Part::Track => c.round_rect(x, y, w, h, s(8), argb(theme::soft())),
            Part::Segment { label, selected } => {
                if *selected {
                    c.card((x, y, w, h), s(6), argb(theme::card()), argb(theme::border()));
                } else if hover {
                    c.round_rect(x, y, w, h, s(6), argb(theme::hover()));
                }
                let ink = if *selected { theme::accent() } else { theme::muted() };
                c.text(if *selected { title } else { body }, label, r, ink, center);
            }
            Part::Divider => c.fill(x, y, w, h.max(1), argb(theme::border())),
            Part::Row { glyph, label, right, danger } => {
                if hover {
                    c.round_rect(x, y, w, h, s(8), argb(if *danger { theme::danger_soft() } else { theme::soft() }));
                }
                let ink = if *danger { theme::danger() } else { theme::text() };
                let icon_ink = if *danger { theme::danger() } else { theme::muted() };
                c.text(icon_font, &glyph.to_string(), rect(x + s(6), y, s(24), h), icon_ink, center);
                c.text(body, label, rect(x + s(38), y, w - s(100), h), ink, left);
                let right_edge = x + w - s(8);
                match right {
                    Right::Nothing => {}
                    Right::Chevron => {
                        c.text(icon_font, &icon::CHEVRON.to_string(), rect(right_edge - s(16), y, s(16), h), theme::muted(), center)
                    }
                    Right::Note(note) => {
                        let flags = DT_RIGHT | DT_VCENTER | DT_SINGLELINE;
                        c.text(small, note, rect(right_edge - s(120), y, s(100), h), theme::muted(), flags);
                        c.text(icon_font, &icon::CHEVRON.to_string(), rect(right_edge - s(16), y, s(16), h), theme::muted(), center);
                    }
                    Right::Switch(on) => {
                        let (sw, sh) = (s(36), s(20));
                        let (sx, sy) = (right_edge - sw, y + (h - sh) / 2);
                        c.round_rect(sx, sy, sw, sh, sh / 2, argb(if *on { theme::accent() } else { theme::switch_off() }));
                        let knob = sh - s(6);
                        let kx = if *on { sx + sw - knob - s(3) } else { sx + s(3) };
                        c.round_rect(kx, sy + s(3), knob, knob, knob / 2, argb(theme::knob()));
                    }
                }
            }
        }
    }

    unsafe fn redraw(&mut self, hwnd: HWND) {
        self.paint();
        self.canvas.present_faded(hwnd, None, self.alpha);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(f) = flyout_of(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
    match msg {
        WM_TIMER if wp == FADE_TIMER => {
            f.alpha = f.alpha.saturating_add(51);
            f.canvas.present_faded(hwnd, None, f.alpha);
            if f.alpha == 255 {
                KillTimer(hwnd, FADE_TIMER);
            }
            0
        }
        WM_MOUSEMOVE => {
            let hover = f.hit((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
            if hover != f.hover {
                f.hover = hover;
                f.redraw(hwnd);
                let mut track = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                TrackMouseEvent(&mut track);
            }
            0
        }
        WM_MOUSELEAVE => {
            if f.hover.take().is_some() {
                f.redraw(hwnd);
            }
            0
        }
        WM_LBUTTONUP => {
            let hit = f.hit((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
            if let Some(action) = hit.and_then(|i| f.items[i].action) {
                mailbox::post(f.owner, WM_FLYOUT_ACTION, action);
                DestroyWindow(hwnd);
            }
            0
        }
        WM_KEYDOWN if wp as u16 == VK_ESCAPE => {
            DestroyWindow(hwnd);
            0
        }
        // Clicou fora: fecha, como um menu.
        WM_ACTIVATE if (wp & 0xFFFF) as u32 == WA_INACTIVE => {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
            0
        }
        WM_DESTROY => {
            OPEN.with(|o| o.set(null_mut()));
            0
        }
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(f as *mut Flyout));
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(mascots: usize) -> Model {
        Model {
            name: "Jujubs".into(),
            pixels: vec![0; 256],
            hearts: 3,
            stats: String::new(),
            mascots: (0..mascots).map(|i| Choice { name: format!("m{i}"), pixels: vec![0; 256] }).collect(),
            current: 0,
            sizes: ["Mini", "P", "M", "G"],
            size: 2,
            ball_out: false,
            pomodoro: false,
            silenced: false,
            hidden: false,
            reminders_on: 2,
            update: None,
        }
    }

    fn actions(m: &Model) -> Vec<Action> {
        layout(m, 96).0.into_iter().filter_map(|i| i.action).collect()
    }

    #[test]
    fn clickable_areas_never_overlap_and_fit_the_panel() {
        for dpi in [96, 144, 192] {
            let (items, height) = layout(&model(4), dpi);
            let panel = items[0].rect;
            let clickable: Vec<&Item> = items.iter().filter(|i| i.action.is_some()).collect();
            for (n, a) in clickable.iter().enumerate() {
                assert!(a.rect.left >= panel.left && a.rect.right <= panel.right && a.rect.bottom <= height, "{:?}", a.part);
                for b in &clickable[n + 1..] {
                    let overlap = a.rect.left < b.rect.right && b.rect.left < a.rect.right && a.rect.top < b.rect.bottom && b.rect.top < a.rect.bottom;
                    assert!(!overlap, "{:?} e {:?} se sobrepõem", a.part, b.part);
                }
            }
        }
    }

    #[test]
    fn mascot_slots_adapt_to_how_many_there_are() {
        assert!(actions(&model(4)).contains(&Action::NewMascot));
        let five = actions(&model(5));
        assert!(!five.contains(&Action::NewMascot) && five.contains(&Action::Mascot(4)));
        let many = actions(&model(9));
        assert!(many.contains(&Action::MoreMascots) && !many.contains(&Action::Mascot(4)));
    }

    #[test]
    fn every_quick_action_is_there() {
        let all = actions(&model(4));
        for action in [Action::Feed, Action::Ball, Action::Chat, Action::Pomodoro, Action::Silence, Action::Quit] {
            assert!(all.contains(&action), "{action:?}");
        }
    }
}
