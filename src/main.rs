#![windows_subsystem = "windows"]
#![allow(non_snake_case)] // nome do binário: dontStayAlone.exe

mod ai;
mod bubble;
mod chat;
mod child;
mod companion;
mod config;
mod flyout;
mod gfx;
mod mailbox;
mod maker;
mod mascot;
mod net;
mod pack;
mod phrases;
mod plugins;
mod prop;
mod rng;
mod secret;
mod settings;
mod sha256;
mod sprite;
mod system;
mod theme;
mod tray;
mod ui;
mod update;
mod welcome;
mod win;

use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::GetModuleHandleW, Threading::CreateMutexW},
    UI::{
        HiDpi::{GetDpiForSystem, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
        Input::KeyboardAndMouse::{ReleaseCapture, SetCapture},
        WindowsAndMessaging::*,
    },
};

use bubble::{Bubble, WM_BUBBLE_CLICK};
use chat::{WM_CHAT_REPLY, WM_CHAT_SEND};
use companion::{Companion, Event, Now, TypingSensor};
use flyout::{Action, Choice, WM_FLYOUT_ACTION};
use config::{Config, Size};
use gfx::Canvas;
use mascot::{Bounds, Mascot, SLEEPY_DAY, SLEEPY_NIGHT};
use pack::PackInfo;
use phrases::{Phrases, Topic};
use child::Reply;
use plugins::{Kind as PluginKind, Plugin};
use prop::{Kind, Prop};
use rng::Rng;
use settings::{Draft, Page, WM_MASCOT_SAVED, WM_SETTINGS_APPLY};
use sprite::{Art, Frame, Sheet, PIXELS, SPRITE};
use system::{bounds_at, clock, cursor, fullscreen_app_running, idle_secs, last_input, ms_since, power};
use tray::{Tray, WM_TRAY};
use welcome::WM_WELCOME_DONE;
use win::{message, w};

const TIMER_ANIM: usize = 1;
const TIMER_WATCH: usize = 2;
const TIMER_BUBBLE: usize = 3;
const TIMER_PROP: usize = 4;
const TIMER_SENSE: usize = 5;
const WATCH_MS: u32 = 2000;
const SENSE_MS: u32 = 500;
const PROP_MS: u32 = 16;
/// Bateria: checa a cada 15 rodadas do watch (30 s).
const POWER_EVERY: u32 = 15;
const SAVE_EVERY_SECS: u64 = 300;
/// Por quanto tempo depois da última tecla ele fica quieto no lugar.
const TYPING_QUIET_MS: u32 = 2500;
/// Mouse na janela do objeto, repassado para a do mascote (wparam = mensagem original).
const WM_PROP_MOUSE: u32 = WM_APP + 3;
/// Comando vindo de outra instância (`dontStayAlone.exe --bolinha`); wparam = id do menu.
const WM_REMOTE: u32 = WM_APP + 4;
/// Um plugin de avisos respondeu (`(id, Reply)` no `mailbox`).
const WM_PLUGIN_SAY: u32 = WM_APP + 9;
/// Primeiro aviso de cada plugin: um minuto depois de abrir (ou de ligar o plugin).
const NOTICE_FIRST_SECS: u64 = 60;
/// Resposta do filho que procura versões novas (`Reply` no `mailbox`).
const WM_UPDATE_CHECKED: u32 = WM_APP + 11;
/// Versão nova baixada e conferida (`Reply` com o caminho, no `mailbox`).
const WM_UPDATE_DOWNLOADED: u32 = WM_APP + 12;
/// Primeira procura por versão nova: dois minutos depois de abrir; depois, uma vez por dia.
const UPDATE_FIRST_SECS: u64 = 120;
const UPDATE_EVERY_SECS: u64 = 24 * 3600;

/// Argumentos aceitos na linha de comando (úteis em atalhos do Windows).
/// Outra instância repassa o índice nesta lista com `WM_REMOTE`.
const COMMANDS: [(&str, Action); 6] = [
    ("--petisco", Action::Feed),
    ("--bolinha", Action::Ball),
    ("--resumo", Action::Summary),
    ("--esconder", Action::Hide),
    ("--configurar", Action::Settings),
    ("--conversar", Action::Chat),
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |i: usize| args.get(i).map_or("", String::as_str);
    // Modos internos: o app abre a si mesmo para as tarefas de rede (sem janela).
    match arg(1) {
        ai::ARG => std::process::exit(ai::serve()),
        update::ARG_CHECK => std::process::exit(update::serve_check()),
        update::ARG_DOWNLOAD => std::process::exit(update::serve_download(arg(2))),
        _ => {}
    }
    // Recém-atualizado: espera a versão antiga fechar antes de ocupar o lugar dela.
    let just_updated = arg(1) == update::ARG_AFTER;
    if just_updated {
        update::finish(arg(2));
    } else {
        update::cleanup();
    }
    let command = COMMANDS.iter().position(|(a, _)| *a == arg(1));
    unsafe {
        let _instance = CreateMutexW(null(), 0, w("Local\\StayAlone.Instance").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // Já está aberto: só repassa o comando (útil para atalhos do Windows).
            let running = FindWindowW(w("StayAloneMascot").as_ptr(), null());
            if let (Some(cmd), false) = (command, running.is_null()) {
                PostMessageW(running, WM_REMOTE, cmd, 0);
            }
            return;
        }
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinstance = GetModuleHandleW(null());
        let Some(hwnd) = create_window(hinstance, "StayAloneMascot", Some(wndproc), null_mut()) else { return };
        let Some(prop_hwnd) = create_window(hinstance, "StayAloneProp", Some(prop_proc), hwnd) else { return };
        SetWindowLongPtrW(prop_hwnd, GWLP_USERDATA, hwnd as isize);

        let first_run = !config::exists();
        let config = Config::load();
        let base_phrases = load_asset("phrases.txt", phrases::EMBEDDED, Phrases::parse);
        let props = Sheet::parse(sprite::PROPS).expect("props embutidos válidos");
        let (pack, art, phrases) = match load_mascot(&config.mascot, &base_phrases) {
            Ok(loaded) => loaded,
            Err(e) => {
                message(&format!("Não consegui carregar o mascote '{}', usando o padrão.\n\n{e}", config.mascot));
                load_mascot(pack::DEFAULT, &base_phrases).expect("mascote padrão válido")
            }
        };

        let bubble = Bubble::new(hwnd, hinstance);
        let app = App::new(hwnd, prop_hwnd, art, props, base_phrases, phrases, bubble, config);
        let app = Box::into_raw(Box::new(app));
        (*app).config.mascot = pack;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, app as isize);
        if just_updated {
            (*app).pending = Some((Topic::App, Some(format!("Atualizei! Agora estou na versão {}.", update::current()))));
        }
        (*app).start();
        if first_run {
            welcome::open(hwnd, &(*app).config.mascot, (*app).app_icon);
        }
        if let Some(cmd) = command {
            PostMessageW(hwnd, WM_REMOTE, cmd, 0);
        }

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if chat::pre_translate(&msg) || settings::is_dialog_message(&msg) || welcome::is_dialog_message(&msg) {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        drop(Box::from_raw(app));
    }
}

/// Janela em camadas, transparente, sem botão na barra de tarefas.
unsafe fn create_window(hinstance: HINSTANCE, class: &str, proc: WNDPROC, owner: HWND) -> Option<HWND> {
    let class = w(class);
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: proc,
        hInstance: hinstance,
        hCursor: LoadCursorW(null_mut(), IDC_HAND),
        lpszClassName: class.as_ptr(),
        ..zeroed()
    };
    RegisterClassExW(&wc);
    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
        class.as_ptr(),
        w("!StayAlone").as_ptr(),
        WS_POPUP,
        0,
        0,
        1,
        1,
        owner,
        null_mut(),
        hinstance,
        null(),
    );
    (!hwnd.is_null()).then_some(hwnd)
}

/// Usa o arquivo `name` ao lado do .exe se existir e for válido; senão, o embutido.
fn load_asset<T>(name: &str, embedded: &str, parse: fn(&str) -> Result<T, String>) -> T {
    let custom = std::env::current_exe().ok().and_then(|exe| config::read_file(&exe.with_file_name(name)));
    if let Some(src) = custom {
        match parse(&src) {
            Ok(value) => return value,
            Err(e) => message(&format!("{name} inválido, usando o padrão.\n\n{e}")),
        }
    }
    parse(embedded).unwrap_or_else(|e| panic!("{name} embutido inválido: {e}"))
}

/// Carrega um mascote pelo id: (id, arte, falas padrão + falas próprias).
fn load_mascot(id: &str, base: &Phrases) -> Result<(String, Art, Phrases), String> {
    let info = pack::list().into_iter().find(|p| p.id == id).ok_or("não encontrado")?;
    load_pack(&info, base)
}

fn load_pack(info: &PackInfo, base: &Phrases) -> Result<(String, Art, Phrases), String> {
    let pack = pack::load(info)?;
    let mut phrases = base.clone();
    // Falas padrão → no feminino (se for "ela") → falas próprias do mascote.
    if pack.art.female() {
        phrases.overlay(&Phrases::parse_partial(pack::FEMININE).expect("falas femininas válidas"));
    }
    if let Some(src) = &pack.phrases {
        let own = Phrases::parse_partial(src).map_err(|e| format!("phrases.txt: {e}"))?;
        phrases.overlay(&own);
    }
    Ok((pack.id, pack.art, phrases))
}

unsafe fn pixel_scale(size: Size) -> i32 {
    ((size.factor() * GetDpiForSystem() as i32 + 48) / 96).max(1)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Grab {
    Mascot,
    Toy,
}

struct Press {
    what: Grab,
    start: POINT,
    grab: POINT,
    dragging: bool,
    last: (f32, f32, Instant),
    vel: (f32, f32),
}

/// Plugin de avisos ligado e quando ele roda de novo.
struct Notice {
    plugin: Plugin,
    next: u64,
    running: bool,
}

/// Bolinha ou petisco na tela.
struct Toy {
    prop: Prop,
    bounds: Bounds,
    canvas: Canvas,
    /// (índice do frame, veio da arte do mascote?)
    rendered: Option<(usize, bool)>,
    shown_at: (i32, i32),
    kicks: u32,
    until: u64,
}

struct App {
    hwnd: HWND,
    prop_hwnd: HWND,
    art: Art,
    props: Sheet,
    base_phrases: Phrases,
    phrases: Phrases,
    bubble: Bubble,
    config: Config,
    companion: Companion,
    mascot: Mascot,
    toy: Option<Toy>,
    typing: TypingSensor,
    rng: Rng,
    bounds: Bounds,
    scale: i32,
    dpi: u32,
    canvas: Canvas,
    tray: Tray,
    /// Ícone do .exe (recurso 1), usado na janela de configurações.
    app_icon: HICON,
    rendered: Option<(Frame, bool)>,
    shown_at: (i32, i32),
    user_hidden: bool,
    busy_hidden: bool,
    timers: [u32; 6],
    started: Instant,
    last_tick: Instant,
    last_prop_tick: Instant,
    last_save: u64,
    watch_count: u32,
    press: Option<Press>,
    cursor_seen: (i32, i32),
    /// `last_input()` da última vez que o cursor se mexeu.
    mouse_input: u32,
    /// Fala que chegou numa hora ruim (caindo, tela cheia); sai assim que der.
    pending: Option<(Topic, Option<String>)>,
    hearts: u32,
    taskbar_created: u32,
    /// Conversa com a IA: histórico recente e se há resposta a caminho.
    chat_history: Vec<chat::Turn>,
    chat_busy: bool,
    /// Plugin que responde à conversa (nenhum = conversa desligada).
    chat_plugin: Option<Plugin>,
    notices: Vec<Notice>,
    /// Versão nova encontrada no GitHub (o painel oferece atualizar).
    update: Option<update::Release>,
    /// Procurando ou baixando versão nova agora.
    update_busy: bool,
    update_checked: Option<u64>,
}

impl App {
    #[allow(clippy::too_many_arguments)]
    unsafe fn new(
        hwnd: HWND,
        prop_hwnd: HWND,
        art: Art,
        props: Sheet,
        base_phrases: Phrases,
        phrases: Phrases,
        bubble: Bubble,
        config: Config,
    ) -> App {
        let scale = pixel_scale(config.size);
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        let mut mascot = Mascot::new(scale as f32, config.speed as f32, seed);
        let bounds = bounds_at(POINT { x: 0, y: 0 }); // monitor principal
        mascot.drop_in(&bounds);

        let (day, hour) = clock();
        let mut companion = Companion::new(config.companion.clone(), day);
        if let Some(state) = config::read_state() {
            companion.load_string(&state);
        }

        App {
            hwnd,
            prop_hwnd,
            tray: Tray::new(hwnd, &art),
            app_icon: LoadIconW(GetModuleHandleW(null()), 1 as _),
            art,
            props,
            base_phrases,
            phrases,
            bubble,
            config,
            companion,
            mascot,
            toy: None,
            typing: TypingSensor::default(),
            rng: Rng::new(seed.rotate_left(17)),
            bounds,
            scale,
            dpi: GetDpiForSystem(),
            canvas: Canvas::new(SPRITE as i32 * scale, SPRITE as i32 * scale),
            rendered: None,
            shown_at: (i32::MIN, i32::MIN),
            user_hidden: false,
            busy_hidden: false,
            timers: [0; 6],
            started: Instant::now(),
            last_tick: Instant::now(),
            last_prop_tick: Instant::now(),
            last_save: 0,
            watch_count: 0,
            press: None,
            cursor_seen: (i32::MIN, 0),
            mouse_input: 0,
            pending: Some((Companion::greeting(hour), None)), // diz oi assim que pousar
            hearts: 0,
            taskbar_created: 0,
            chat_history: Vec::new(),
            chat_busy: false,
            chat_plugin: None,
            notices: Vec::new(),
            update: None,
            update_busy: false,
            update_checked: None,
        }
    }

    unsafe fn start(&mut self) {
        self.taskbar_created = RegisterWindowMessageW(w("TaskbarCreated").as_ptr());
        self.tray.show(&self.tip());
        if config::autostart_enabled() {
            config::set_autostart(true); // atualiza o caminho caso o .exe tenha mudado de lugar
        }
        self.load_plugins();
        SetTimer(self.hwnd, TIMER_WATCH, WATCH_MS, None);
        self.busy_hidden = fullscreen_app_running();
        self.update_visibility();
    }

    fn secs(&self) -> u64 {
        self.started.elapsed().as_secs()
    }

    fn visible(&self) -> bool {
        !self.user_hidden && !self.busy_hidden
    }

    /// Só redesenha quando o frame muda; se só a posição mudou, apenas move a janela.
    unsafe fn present(&mut self) {
        let frame = (self.mascot.frame(), self.mascot.facing_left);
        let pos = (self.mascot.x.round() as i32, self.mascot.y.round() as i32);
        if self.rendered != Some(frame) {
            let scale = self.scale as usize;
            self.art.draw(frame.0, frame.1, scale, self.canvas.pixels());
            self.canvas.present(self.hwnd, (pos != self.shown_at).then_some(pos));
            self.rendered = Some(frame);
        } else if pos != self.shown_at {
            SetWindowPos(self.hwnd, null_mut(), pos.0, pos.1, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
        self.shown_at = pos;
    }

    unsafe fn present_toy(&mut self) {
        let Some(toy) = self.toy.as_mut() else { return };
        let sprite = match (toy.prop.kind, self.art.food) {
            (Kind::Food, Some(food)) => (food, true),
            (Kind::Food, None) => (self.props.find("food").unwrap_or(0), false),
            (Kind::Ball, _) => (self.props.find(["ball1", "ball2"][toy.prop.roll_frame()]).unwrap_or(0), false),
        };
        let pos = (toy.prop.x.round() as i32, toy.prop.y.round() as i32);
        let moved = pos != toy.shown_at;
        if toy.rendered != Some(sprite) {
            let sheet = if sprite.1 { &self.art.sheet } else { &self.props };
            sheet.draw(sprite.0, false, self.scale as usize, toy.canvas.pixels());
            toy.rendered = Some(sprite);
            toy.canvas.present(self.prop_hwnd, moved.then_some(pos));
        } else if moved {
            SetWindowPos(self.prop_hwnd, null_mut(), pos.0, pos.1, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
        toy.shown_at = pos;
    }

    /// Você está usando o teclado agora? (atividade recente sem o cursor se mexer)
    unsafe fn typing_now(&mut self) -> bool {
        let input = last_input();
        let p = cursor();
        if (p.x, p.y) != self.cursor_seen {
            self.cursor_seen = (p.x, p.y);
            self.mouse_input = input;
        }
        input != self.mouse_input && ms_since(input) < TYPING_QUIET_MS
    }

    /// Algo (fora as nossas próprias janelas) está acima do mascote?
    unsafe fn covered(&self) -> bool {
        let mut above = GetWindow(self.hwnd, GW_HWNDPREV);
        while !above.is_null() {
            let ours = above == self.prop_hwnd || GetWindow(above, GW_OWNER) == self.hwnd;
            if !ours && IsWindowVisible(above) != 0 {
                return true;
            }
            above = GetWindow(above, GW_HWNDPREV);
        }
        false
    }

    /// Liga/desliga cada timer conforme a necessidade: nada roda à toa.
    unsafe fn sync_timer(&mut self) {
        let visible = self.visible();
        let toy_moving = self.toy.as_ref().is_some_and(|t| !t.prop.held && !t.prop.resting(&t.bounds));
        let wanted = [
            (TIMER_ANIM, if visible { self.mascot.interval_ms() } else { 0 }),
            (TIMER_PROP, if visible && toy_moving { PROP_MS } else { 0 }),
            (TIMER_SENSE, if visible && !self.companion.away() { SENSE_MS } else { 0 }),
        ];
        for (id, ms) in wanted {
            if self.timers[id] == ms {
                continue;
            }
            if ms == 0 {
                KillTimer(self.hwnd, id);
            } else {
                if self.timers[id] == 0 && id == TIMER_PROP {
                    self.last_prop_tick = Instant::now();
                }
                SetTimer(self.hwnd, id, ms, None);
            }
            self.timers[id] = ms;
        }
    }

    unsafe fn on_tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32().min(0.05);
        self.last_tick = now;
        if let Some(toy) = &self.toy {
            self.mascot.move_target(toy.prop.center_x());
        }
        let typing = self.typing_now();
        self.mascot.set_still(typing);
        self.mascot.update(dt, &self.bounds);
        if self.mascot.take_arrived() {
            self.on_arrive();
        }
        self.present();
        self.sync_timer();
    }

    unsafe fn on_prop_tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_prop_tick).as_secs_f32().min(0.05);
        self.last_prop_tick = now;
        if let Some(toy) = self.toy.as_mut() {
            toy.prop.update(dt, &toy.bounds);
        }
        self.present_toy();
        self.sync_timer();
    }

    unsafe fn on_sense(&mut self) {
        let p = cursor();
        if self.typing.sample(last_input(), (p.x, p.y)) {
            self.mascot.cheer();
            if let Some(topic) = self.companion.on_typing(self.secs()) {
                self.say(topic);
            }
            self.present();
            self.sync_timer();
        }
    }

    unsafe fn on_watch(&mut self) {
        self.watch_count += 1;
        let busy = fullscreen_app_running();
        if busy != self.busy_hidden {
            self.busy_hidden = busy;
            self.update_visibility();
        }
        let typing = self.typing_now();
        if self.visible() && self.press.is_none() && !typing {
            // A barra de tarefas também é "topmost"; volta para a frente só se algo passou por cima
            // (mexer na ordem das janelas faz o Windows reexibir o ponteiro escondido).
            if self.covered() {
                SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
            if self.bubble.topic.is_none() {
                self.on_display_change();
            }
        }

        let (day, hour) = clock();
        let now = Now {
            secs: self.secs(),
            day,
            hour,
            idle_secs: idle_secs(),
            hidden: self.user_hidden,
            busy: self.busy_hidden,
        };
        for event in self.companion.update(&now) {
            match event {
                Event::Say(topic) => self.say(topic),
                Event::Remind(i) => self.remind(i),
                Event::Doze => {
                    self.hide_bubble();
                    self.remove_toy();
                    self.mascot.doze();
                }
                Event::WakeUp => self.mascot.wake_happy(),
            }
        }
        if self.watch_count % POWER_EVERY == 1 {
            if let Some(topic) = power().and_then(|(battery, pct)| self.companion.on_power(battery, pct)) {
                self.say(topic);
            }
            self.mascot.set_tired(self.companion.low_battery);
        }
        if let Some((topic, text)) = self.pending.take() {
            self.speak(topic, text);
        }
        self.check_toy_done(now.secs);
        self.run_notices(now.secs, hour);
        self.check_updates(now.secs);

        let night = !(6..22).contains(&hour);
        let cheer = match self.companion.affection {
            a if a >= 70.0 => 8,
            a if a >= 40.0 => 3,
            _ => 0,
        };
        self.mascot.set_mood(if night || self.companion.low_battery { SLEEPY_NIGHT } else { SLEEPY_DAY }, cheer);
        if self.companion.hearts() != self.hearts {
            self.hearts = self.companion.hearts();
            self.tray.set_tip(&self.tip());
        }
        if now.secs >= self.last_save + SAVE_EVERY_SECS {
            self.save_state();
        }
        self.present();
        self.sync_timer();
    }

    unsafe fn on_display_change(&mut self) {
        self.refresh_bounds();
        self.mascot.settle(&self.bounds);
        self.present();
        self.sync_timer();
    }

    unsafe fn refresh_bounds(&mut self) {
        let half = self.mascot.size() / 2.0;
        let center = POINT { x: (self.mascot.x + half) as i32, y: (self.mascot.y + half) as i32 };
        self.bounds = bounds_at(center);
    }

    unsafe fn update_visibility(&mut self) {
        if self.visible() {
            self.last_tick = Instant::now();
            self.rendered = None;
            self.present();
            ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            if self.toy.is_some() {
                self.present_toy();
                ShowWindow(self.prop_hwnd, SW_SHOWNOACTIVATE);
            }
        } else {
            self.hide_bubble();
            ShowWindow(self.hwnd, SW_HIDE);
            ShowWindow(self.prop_hwnd, SW_HIDE);
        }
        self.sync_timer();
    }

    unsafe fn toggle_hidden(&mut self) {
        self.user_hidden = !self.user_hidden;
        self.update_visibility();
    }

    unsafe fn set_size(&mut self, size: Size) {
        self.hide_bubble();
        self.remove_toy();
        self.config.size = size;
        self.config.save();
        self.scale = pixel_scale(size);
        let px = SPRITE as i32 * self.scale;
        self.canvas = Canvas::new(px, px);
        self.mascot.resize(self.scale as f32, &self.bounds);
        self.rendered = None;
        self.present();
        self.sync_timer();
    }

    unsafe fn set_mascot(&mut self, info: &PackInfo) {
        let (id, art, phrases) = match load_pack(info, &self.base_phrases) {
            Ok(loaded) => loaded,
            Err(e) => return message(&format!("Não consegui carregar '{}'.\n\n{e}", info.name)),
        };
        self.hide_bubble();
        self.art = art;
        self.phrases = phrases;
        self.config.mascot = id;
        self.config.save();
        self.tray.set_art(&self.art, &self.tip());
        self.rendered = None;
        if let Some(toy) = self.toy.as_mut() {
            toy.rendered = None;
        }
        self.present();
        self.present_toy();
        self.chat_history.clear(); // outro mascote, outra conversa
        self.say(Topic::Hello);
    }

    // --- conversa -----------------------------------------------------------

    unsafe fn open_chat(&mut self) {
        if self.user_hidden {
            self.toggle_hidden();
        }
        let size = self.mascot.size();
        let center = (self.mascot.x + size / 2.0) as i32;
        let work = RECT {
            left: self.bounds.left as i32,
            top: self.bounds.top as i32,
            right: self.bounds.right as i32,
            bottom: self.bounds.floor as i32,
        };
        let placeholder = format!("Diga algo para {} {}...", self.art.article(), self.art.name);
        self.hide_bubble();
        chat::open(self.hwnd, &placeholder, center, self.mascot.y as i32 - 4, work, self.dpi);
    }

    /// Você mandou uma mensagem na caixinha.
    unsafe fn on_chat_send(&mut self, text: String) {
        if self.chat_busy {
            return; // uma pergunta de cada vez
        }
        let Some(plugin) = self.chat_plugin.clone() else {
            let tip = "Para conversar comigo, ligue um plugin de conversa em Configurações → Plugins.";
            return self.speak(Topic::Chat, Some(tip.into()));
        };
        self.chat_history.push((true, text));
        let excess = self.chat_history.len().saturating_sub(chat::HISTORY);
        self.chat_history.drain(..excess);
        let about = self.art.sheet.about.as_deref().unwrap_or("um mascote fofinho");
        let system = chat::system_prompt(&self.art.name, about, self.art.female());
        let payload = chat::payload(&system, &self.chat_history, None);
        self.chat_busy = true;
        self.speak(Topic::Chat, Some("...".into())); // pensando
        let approved = plugin.approved(&self.config.plugins);
        plugins::request(self.hwnd, WM_CHAT_REPLY, &plugin, approved, payload, |reply| reply);
    }

    unsafe fn on_chat_reply(&mut self, reply: Reply) {
        self.chat_busy = false;
        match reply {
            Ok(text) => {
                let text = chat::shorten(&text);
                self.chat_history.push((false, text.clone()));
                self.speak(Topic::Chat, Some(text));
            }
            Err(e) => {
                self.chat_history.pop(); // a pergunta ficou sem resposta
                self.speak(Topic::Chat, Some(chat::shorten(&format!("Hmm, não consegui responder: {e}"))));
            }
        }
    }

    // --- plugins ------------------------------------------------------------

    /// Lê quais plugins estão ligados (ao abrir e depois de mudar as configurações).
    unsafe fn load_plugins(&mut self) {
        let enabled = &self.config.plugins;
        let on = |p: &Plugin| enabled.iter().any(|e| e.id == p.id) && p.program.is_file();
        let all: Vec<Plugin> = plugins::list().into_iter().filter(on).collect();
        self.chat_plugin = all.iter().find(|p| p.kind == PluginKind::Chat).cloned();
        let first = self.secs() + NOTICE_FIRST_SECS;
        let old = std::mem::take(&mut self.notices);
        self.notices = all
            .into_iter()
            .filter(|p| p.kind == PluginKind::Notice)
            .map(|plugin| {
                // Quem já estava ligado mantém o próprio relógio.
                let kept = old.iter().find(|n| n.plugin == plugin);
                Notice { next: kept.map_or(first, |n| n.next), running: kept.is_some_and(|n| n.running), plugin }
            })
            .collect();
    }

    /// Roda os plugins de avisos que chegaram na hora — só com você por perto e sem silêncio.
    unsafe fn run_notices(&mut self, secs: u64, hour: u32) {
        let quiet = !self.visible() || self.companion.away() || self.companion.silenced(secs);
        if quiet || self.notices.iter().all(|n| n.running || secs < n.next) {
            return;
        }
        let input = plugins::notice_input(&self.art.name, self.art.female(), hour);
        for n in self.notices.iter_mut().filter(|n| !n.running && secs >= n.next) {
            n.next = secs + n.plugin.every as u64 * 60;
            n.running = true;
            let id = n.plugin.id.clone();
            let approved = n.plugin.approved(&self.config.plugins);
            plugins::request(self.hwnd, WM_PLUGIN_SAY, &n.plugin, approved, input.clone(), move |reply| (id, reply));
        }
    }

    unsafe fn on_plugin_say(&mut self, id: String, reply: Reply) {
        let Some(i) = self.notices.iter().position(|n| n.plugin.id == id) else { return };
        self.notices[i].running = false;
        match reply {
            Ok(text) => {
                let text = win::clean_line(&text, plugins::MAX_NOTICE);
                if !text.is_empty() {
                    self.speak(Topic::Plugin, Some(text));
                }
            }
            // Arquivo trocado depois de aprovado: pausa e avisa (uma vez).
            Err(e) if e == plugins::CHANGED => {
                let notice = self.notices.remove(i);
                self.speak(Topic::Plugin, Some(format!("Plugin \"{}\": {e}", notice.plugin.name)));
            }
            Err(_) => {} // falhas de um aviso não interrompem você; o Testar da aba Plugins mostra o erro
        }
    }

    // --- atualização --------------------------------------------------------

    /// Terminou (ou fechou) as boas-vindas.
    unsafe fn on_welcome(&mut self, choices: welcome::Choices) {
        for (reminder, on) in self.config.companion.reminders.iter_mut().zip(choices.reminders) {
            reminder.on = on;
        }
        self.companion.set_settings(self.config.companion.clone());
        if choices.autostart != config::autostart_enabled() {
            config::set_autostart(choices.autostart);
        }
        self.config.save();
        match pack::list().into_iter().find(|p| p.id == choices.mascot) {
            Some(info) if info.id != self.config.mascot => self.set_mascot(&info), // já diz "oi"
            _ => self.say(Topic::Hello),
        }
    }

    unsafe fn check_updates(&mut self, secs: u64) {
        let due = match self.update_checked {
            None => secs >= UPDATE_FIRST_SECS,
            Some(last) => secs >= last + UPDATE_EVERY_SECS,
        };
        if self.config.updates && !self.update_busy && due {
            self.update_checked = Some(secs);
            self.update_busy = true;
            update::check(self.hwnd, WM_UPDATE_CHECKED);
        }
    }

    unsafe fn on_update_checked(&mut self, reply: Reply) {
        self.update_busy = false;
        // Sem internet ou GitHub fora do ar: tenta de novo amanhã, sem incomodar.
        let Some(release) = reply.ok().and_then(|line| update::Release::from_line(&line)) else { return };
        if update::is_newer(&release.version, update::current()) && self.update.as_ref() != Some(&release) {
            let text = format!("Tem versão nova de mim (v{})! Abra o painel para atualizar.", release.version);
            self.update = Some(release);
            self.speak(Topic::App, Some(text));
        }
    }

    unsafe fn start_update(&mut self) {
        let Some(release) = self.update.clone().filter(|_| !self.update_busy) else { return };
        self.update_busy = true;
        self.speak(Topic::App, Some(format!("Baixando a versão {}...", release.version)));
        update::fetch(self.hwnd, WM_UPDATE_DOWNLOADED, &release);
    }

    unsafe fn on_update_downloaded(&mut self, reply: Reply) {
        self.update_busy = false;
        let installed = reply.and_then(|path| {
            self.save_state();
            update::install(&path)
        });
        match installed {
            Ok(()) => {
                DestroyWindow(self.hwnd); // a versão nova já está abrindo
            }
            Err(e) => self.speak(Topic::App, Some(chat::shorten(&format!("Não consegui atualizar: {e}")))),
        }
    }

    /// OK na janela de configurações.
    unsafe fn apply_settings(&mut self, draft: &Draft) {
        let new = &draft.config;
        if new.mascot != self.config.mascot {
            if let Some(info) = pack::list().into_iter().find(|p| p.id == new.mascot) {
                self.set_mascot(&info);
            }
        }
        if new.size != self.config.size {
            self.set_size(new.size);
        }
        self.config.speed = new.speed;
        self.mascot.set_speed(new.speed as f32);
        self.config.companion = new.companion.clone();
        self.config.updates = new.updates;
        self.config.birthday = new.birthday;
        self.config.quiet_in_meetings = new.quiet_in_meetings;
        self.config.memory = new.memory;
        self.config.theme = new.theme;
        self.config.language = new.language;
        self.companion.set_settings(new.companion.clone());
        if draft.autostart != config::autostart_enabled() {
            config::set_autostart(draft.autostart);
        }
        if new.plugins != self.config.plugins {
            self.config.plugins = new.plugins.clone();
            self.load_plugins();
        }
        self.config.save();
    }

    unsafe fn save_state(&mut self) {
        self.last_save = self.secs();
        config::write_state(&self.companion.save_string());
    }

    // --- brincadeiras -----------------------------------------------------

    unsafe fn spawn_toy(&mut self, kind: Kind) {
        self.remove_toy();
        let (size, b, scale) = (self.mascot.size(), self.bounds, self.scale as f32);
        let center = self.mascot.x + size / 2.0;
        // Aparece do lado com mais espaço, um pouco acima do chão.
        let dir = if center - b.left > b.right - center { -1.0 } else { 1.0 };
        let x = (center + dir * size * 3.0 - size / 2.0).clamp(b.left, (b.right - size).max(b.left));
        let mut prop = Prop::new(kind, x, b.floor - size * 4.0, scale);
        let k = scale / 4.0;
        if kind == Kind::Ball {
            prop.throw(dir * 350.0 * k, -300.0 * k);
        }
        let px = SPRITE as i32 * self.scale;
        let until = self.secs() + if kind == Kind::Ball { 90 } else { 60 };
        let canvas = Canvas::new(px, px);
        self.toy = Some(Toy { prop, bounds: b, canvas, rendered: None, shown_at: (i32::MIN, 0), kicks: 0, until });
        self.present_toy();
        ShowWindow(self.prop_hwnd, SW_SHOWNOACTIVATE);
        self.hide_bubble();
        self.mascot.set_target(Some(x + size / 2.0));
        self.present();
        self.sync_timer();
    }

    unsafe fn remove_toy(&mut self) {
        if self.toy.take().is_some() {
            ShowWindow(self.prop_hwnd, SW_HIDE);
            self.mascot.set_target(None);
            if matches!(self.press, Some(Press { what: Grab::Toy, .. })) {
                self.press = None;
                ReleaseCapture();
            }
            self.sync_timer();
        }
    }

    /// O mascote alcançou o objeto.
    unsafe fn on_arrive(&mut self) {
        let Some(toy) = self.toy.as_ref() else { return };
        if toy.prop.held {
            self.mascot.hop(); // pulando para pegar da sua mão
            return;
        }
        let size = self.mascot.size();
        let kind = toy.prop.kind;
        let on_floor = toy.prop.on_floor(&toy.bounds);
        let low = toy.prop.y + size >= toy.bounds.floor - size * 0.6;
        let side = (toy.prop.center_x() - (self.mascot.x + size / 2.0)).signum();
        match kind {
            Kind::Food if on_floor => {
                self.remove_toy();
                self.mascot.eat();
                self.companion.on_fed(self.secs());
                self.say(Topic::Yummy);
            }
            Kind::Ball if low => {
                let dir = if side == 0.0 { 1.0 } else { side };
                let k = self.scale as f32 / 4.0;
                let (vx, vy) = (self.rng.range(500, 950) as f32, self.rng.range(350, 750) as f32);
                if let Some(toy) = self.toy.as_mut() {
                    toy.prop.throw(dir * vx * k, -vy * k);
                    toy.kicks += 1;
                }
                self.mascot.hop();
            }
            _ => {}
        }
    }

    unsafe fn check_toy_done(&mut self, secs: u64) {
        let Some(toy) = &self.toy else { return };
        let tired = toy.prop.kind == Kind::Ball && toy.kicks >= 10 && toy.prop.resting(&toy.bounds);
        if toy.prop.held || (secs < toy.until && !tired) {
            return;
        }
        let ball = toy.prop.kind == Kind::Ball;
        self.remove_toy();
        if ball {
            self.companion.on_played();
            self.mascot.wake_happy();
            self.say(Topic::BallDone);
        }
    }

    // --- fala -------------------------------------------------------------

    unsafe fn say(&mut self, topic: Topic) {
        self.speak(topic, None);
    }

    /// Lembrete `i` da configuração: os embutidos usam as falas do mascote, os seus o seu texto.
    unsafe fn remind(&mut self, i: usize) {
        let Some(r) = self.config.companion.reminders.get(i) else { return };
        match r.kind.topic() {
            Some(topic) => self.say(topic),
            None => self.speak(Topic::Reminder, Some(r.kind.label().to_string())),
        }
    }

    /// Mostra o balão com uma fala sorteada do tópico (ou com `text`, se vier).
    unsafe fn speak(&mut self, topic: Topic, text: Option<String>) {
        if self.user_hidden {
            return;
        }
        // Não interrompe: espera pousar, sair da tela cheia e você dar uma pausa na digitação.
        if self.busy_hidden || !self.mascot.grounded() || self.press.is_some() || (topic != Topic::Chat && self.typing_now()) {
            self.pending = Some((topic, text));
            return;
        }
        let text = match text {
            Some(text) => text,
            None => {
                let line = self.phrases.pick(topic, &mut self.rng);
                companion::fill(line, &self.companion.stats, &self.art.name)
            }
        };
        self.mascot.talk();
        self.present();
        let size = self.mascot.size();
        // A ponta do balão encosta no topo da cabeça.
        let mut anchor = ((self.mascot.x + size / 2.0) as i32, (self.mascot.y + self.scale as f32 * 2.0) as i32);
        // Com a caixinha de conversa aberta, o balão sobe para ficar acima dela.
        if let Some(input) = chat::input_rect() {
            anchor.1 = anchor.1.min(input.top - 2);
        }
        let ms = self.bubble.show(&text, topic, anchor, &self.bounds, self.dpi);
        SetTimer(self.hwnd, TIMER_BUBBLE, ms, None);
        self.sync_timer();
    }

    unsafe fn hide_bubble(&mut self) {
        KillTimer(self.hwnd, TIMER_BUBBLE);
        self.bubble.hide();
        self.mascot.quiet();
    }

    unsafe fn on_bubble_click(&mut self) {
        let topic = self.bubble.topic;
        self.hide_bubble();
        if let Some(reply) = topic.and_then(|t| self.companion.on_ack(t)) {
            self.say(reply);
        }
    }

    // --- mouse (mascote e objeto) -------------------------------------------

    unsafe fn on_press(&mut self, what: Grab) {
        let origin = match (what, &self.toy) {
            (Grab::Toy, Some(toy)) => (toy.prop.x, toy.prop.y),
            (Grab::Toy, None) => return,
            (Grab::Mascot, _) => (self.mascot.x, self.mascot.y),
        };
        let p = cursor();
        SetCapture(if what == Grab::Toy { self.prop_hwnd } else { self.hwnd });
        self.press = Some(Press {
            what,
            start: p,
            grab: POINT { x: p.x - origin.0.round() as i32, y: p.y - origin.1.round() as i32 },
            dragging: false,
            last: (p.x as f32, p.y as f32, Instant::now()),
            vel: (0.0, 0.0),
        });
    }

    unsafe fn on_drag(&mut self) {
        let Some(press) = self.press.as_mut() else { return };
        let p = cursor();
        if !press.dragging {
            if (p.x - press.start.x).abs() < 5 && (p.y - press.start.y).abs() < 5 {
                return;
            }
            press.dragging = true;
            if press.what == Grab::Mascot {
                self.mascot.grab();
                KillTimer(self.hwnd, TIMER_BUBBLE);
                self.bubble.hide();
                self.mascot.quiet();
            }
        }
        let now = Instant::now();
        let dt = now.duration_since(press.last.2).as_secs_f32();
        if dt > 0.001 {
            let vx = (p.x as f32 - press.last.0) / dt;
            let vy = (p.y as f32 - press.last.1) / dt;
            press.vel = (press.vel.0 * 0.4 + vx * 0.6, press.vel.1 * 0.4 + vy * 0.6);
            press.last = (p.x as f32, p.y as f32, now);
        }
        let pos = ((p.x - press.grab.x) as f32, (p.y - press.grab.y) as f32);
        match press.what {
            Grab::Mascot => {
                (self.mascot.x, self.mascot.y) = pos;
                self.present();
            }
            Grab::Toy => {
                if let Some(toy) = self.toy.as_mut() {
                    toy.prop.held = true;
                    (toy.prop.x, toy.prop.y) = pos;
                }
                self.present_toy();
            }
        }
        self.sync_timer();
    }

    /// Retorna `true` se havia um clique em andamento (e a captura deve ser liberada).
    unsafe fn on_release(&mut self) -> bool {
        let Some(press) = self.press.take() else { return false };
        // Se o mouse parou antes de soltar, não arremessa.
        let still = press.last.2.elapsed().as_millis() > 80;
        let vel = if still { (0.0, 0.0) } else { press.vel };
        match (press.what, press.dragging) {
            (Grab::Mascot, true) => self.drop_mascot(vel.0, vel.1),
            (Grab::Toy, true) => self.drop_toy(vel.0, vel.1),
            (Grab::Mascot, false) => {
                self.mascot.pet();
                let roll = self.rng.range(0, 100);
                match self.companion.on_pet(self.secs(), roll) {
                    Some(topic) => self.say(topic),
                    None => {
                        self.hide_bubble();
                        self.present();
                        self.sync_timer();
                    }
                }
            }
            (Grab::Toy, false) => {
                // Clique na bolinha: um tapinha para cima.
                if let Some(toy) = self.toy.as_mut().filter(|t| t.prop.kind == Kind::Ball) {
                    let k = self.scale as f32 / 4.0;
                    let side = self.rng.range(0, 400) as f32 - 200.0;
                    toy.prop.throw(side * k, -700.0 * k);
                }
                self.sync_timer();
            }
        }
        true
    }

    unsafe fn on_capture_lost(&mut self) {
        if let Some(press) = self.press.take() {
            match (press.what, press.dragging) {
                (Grab::Mascot, true) => self.drop_mascot(0.0, 0.0),
                (Grab::Toy, true) => self.drop_toy(0.0, 0.0),
                _ => {}
            }
        }
    }

    unsafe fn drop_mascot(&mut self, vx: f32, vy: f32) {
        self.refresh_bounds();
        self.mascot.release(vx, vy);
        self.last_tick = Instant::now();
        self.present();
        self.sync_timer();
    }

    unsafe fn drop_toy(&mut self, vx: f32, vy: f32) {
        if let Some(toy) = self.toy.as_mut() {
            let half = toy.canvas.width / 2;
            toy.bounds = bounds_at(POINT { x: toy.prop.x as i32 + half, y: toy.prop.y as i32 + half });
            toy.prop.throw(vx, vy);
        }
        self.sync_timer();
    }

    /// Dica do ícone da bandeja: nome e corações de afeto.
    fn tip(&self) -> String {
        let full = self.companion.hearts() as usize;
        format!("{}  {}{}", self.art.name, "♥".repeat(full), "♡".repeat(5 - full))
    }

    unsafe fn shutdown(&mut self) {
        for id in [TIMER_ANIM, TIMER_WATCH, TIMER_BUBBLE, TIMER_PROP, TIMER_SENSE] {
            KillTimer(self.hwnd, id);
        }
        self.save_state();
        self.tray.remove();
    }
}

unsafe fn app_of(hwnd: HWND) -> Option<&'static mut App> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App).as_mut()
}

/// Sprite parado de um mascote (16×16, 0xAARRGGBB), para o painel.
fn idle_pixels(art: &Art) -> Vec<u32> {
    let mut pixels = vec![0; PIXELS];
    art.draw(Frame::Idle, false, 1, &mut pixels);
    pixels
}

/// Abre o painel do mascote (clique direito nele ou no ícone da bandeja).
unsafe fn open_flyout(hwnd: HWND) {
    let Some(app) = app_of(hwnd) else { return };
    let packs = pack::list();
    let mascots = packs
        .iter()
        .map(|info| {
            let pixels = if info.id == app.config.mascot {
                idle_pixels(&app.art)
            } else {
                pack::load(info).map_or_else(|_| vec![0; PIXELS], |p| idle_pixels(&p.art))
            };
            Choice { name: info.name.clone(), pixels }
        })
        .collect();
    let secs = app.secs();
    let model = flyout::Model {
        name: app.art.name.clone(),
        pixels: idle_pixels(&app.art),
        hearts: app.companion.hearts(),
        stats: companion::fill("Hoje: {juntos} juntos · {pausas}", &app.companion.stats, &app.art.name),
        mascots,
        current: packs.iter().position(|p| p.id == app.config.mascot).unwrap_or(usize::MAX),
        sizes: Size::ALL.map(Size::short),
        size: Size::ALL.iter().position(|s| *s == app.config.size).unwrap_or(0),
        ball_out: app.toy.as_ref().is_some_and(|t| t.prop.kind == Kind::Ball),
        pomodoro: app.companion.pomodoro_active(),
        silenced: app.companion.silenced(secs),
        hidden: app.user_hidden,
        reminders_on: app.config.companion.reminders.iter().filter(|r| r.on).count(),
        update: app.update.as_ref().map(|r| r.version.clone()),
    };
    flyout::open(hwnd, model, cursor());
}

/// Executa uma escolha do painel (ou da linha de comando, via `WM_REMOTE`).
unsafe fn run_action(hwnd: HWND, action: Action) {
    let Some(app) = app_of(hwnd) else { return };
    let secs = app.secs();
    let silenced = app.companion.silenced(secs);
    let ball_out = app.toy.as_ref().is_some_and(|t| t.prop.kind == Kind::Ball);
    match action {
        Action::Hide => app.toggle_hidden(),
        Action::Summary => {
            app.companion.stats.summary_shown = true;
            app.say(Topic::Summary);
        }
        Action::Pomodoro if app.companion.pomodoro_active() => app.companion.stop_pomodoro(),
        Action::Pomodoro => {
            app.companion.start_pomodoro(secs);
            app.say(Topic::PomodoroStart);
        }
        Action::Silence if silenced => app.companion.unsilence(),
        Action::Silence => {
            app.companion.silence(secs);
            app.say(Topic::Silence);
        }
        Action::Feed if app.companion.hungry(secs) => app.spawn_toy(Kind::Food),
        Action::Feed => app.say(Topic::Full),
        Action::Ball if ball_out => app.remove_toy(),
        Action::Ball => app.spawn_toy(Kind::Ball),
        // Sem plugin de conversa ligado: leva direto para onde se liga um.
        Action::Chat if app.chat_plugin.is_none() => settings::open(hwnd, &app.config, app.app_icon, Page::Plugins),
        Action::Chat => app.open_chat(),
        Action::Mascot(i) => {
            if let Some(info) = pack::list().get(i) {
                app.set_mascot(info);
            }
        }
        Action::Size(i) => {
            if let Some(&size) = Size::ALL.get(i) {
                app.set_size(size);
            }
        }
        Action::Settings | Action::MoreMascots => settings::open(hwnd, &app.config, app.app_icon, Page::General),
        Action::Reminders => settings::open(hwnd, &app.config, app.app_icon, Page::Reminders),
        Action::NewMascot => settings::open(hwnd, &app.config, app.app_icon, Page::Maker),
        Action::Update => app.start_update(),
        Action::Quit => {
            DestroyWindow(hwnd);
        }
    }
}

/// A janela do objeto só repassa o mouse para a janela do mascote.
unsafe extern "system" fn prop_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP | WM_CAPTURECHANGED => {
            let owner = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as HWND;
            SendMessageW(owner, WM_PROP_MOUSE, msg as WPARAM, 0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // Estes podem abrir janelas que bombeiam mensagens: não seguram referência ao App.
    match msg {
        WM_RBUTTONUP => {
            open_flyout(hwnd);
            return 0;
        }
        // Clique (esquerdo ou direito) no ícone da bandeja abre o painel.
        WM_TRAY if matches!(lp as u32, WM_LBUTTONUP | WM_RBUTTONUP | WM_CONTEXTMENU) => {
            SetForegroundWindow(hwnd);
            open_flyout(hwnd);
            return 0;
        }
        WM_REMOTE => {
            if let Some(&(_, action)) = COMMANDS.get(wp) {
                run_action(hwnd, action);
            }
            return 0;
        }
        WM_FLYOUT_ACTION => {
            if let Some(action) = mailbox::take::<Action>(hwnd, msg) {
                run_action(hwnd, action);
            }
            return 0;
        }
        _ => {}
    }

    let Some(app) = app_of(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
    match msg {
        WM_TIMER => match wp {
            TIMER_ANIM => app.on_tick(),
            TIMER_WATCH => app.on_watch(),
            TIMER_BUBBLE => app.hide_bubble(),
            TIMER_PROP => app.on_prop_tick(),
            TIMER_SENSE => app.on_sense(),
            _ => {}
        },
        WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
        WM_LBUTTONDOWN => app.on_press(Grab::Mascot),
        WM_MOUSEMOVE => app.on_drag(),
        WM_LBUTTONUP => {
            if app.on_release() {
                ReleaseCapture();
            }
        }
        WM_CAPTURECHANGED => app.on_capture_lost(),
        WM_PROP_MOUSE => match wp as u32 {
            WM_LBUTTONDOWN => app.on_press(Grab::Toy),
            WM_MOUSEMOVE => app.on_drag(),
            WM_LBUTTONUP => {
                if app.on_release() {
                    ReleaseCapture();
                }
            }
            WM_CAPTURECHANGED => app.on_capture_lost(),
            _ => {}
        },
        WM_BUBBLE_CLICK => app.on_bubble_click(),
        // Dados vêm pelo `mailbox`: mensagem forjada por outro programa acha a caixa vazia.
        WM_SETTINGS_APPLY => {
            if let Some(draft) = mailbox::take::<Draft>(hwnd, msg) {
                app.apply_settings(&draft);
            }
        }
        WM_MASCOT_SAVED => {
            let saved = mailbox::take::<String>(hwnd, msg).and_then(|id| pack::list().into_iter().find(|p| p.id == id));
            if let Some(info) = saved {
                app.set_mascot(&info);
            }
        }
        WM_CHAT_SEND => {
            if let Some(text) = mailbox::take::<String>(hwnd, msg) {
                app.on_chat_send(text);
            }
        }
        WM_WELCOME_DONE => {
            if let Some(choices) = mailbox::take::<welcome::Choices>(hwnd, msg) {
                app.on_welcome(choices);
            }
        }
        WM_UPDATE_CHECKED => {
            if let Some(reply) = mailbox::take::<Reply>(hwnd, msg) {
                app.on_update_checked(reply);
            }
        }
        WM_UPDATE_DOWNLOADED => {
            if let Some(reply) = mailbox::take::<Reply>(hwnd, msg) {
                app.on_update_downloaded(reply);
            }
        }
        WM_PLUGIN_SAY => {
            if let Some((id, reply)) = mailbox::take::<(String, Reply)>(hwnd, msg) {
                app.on_plugin_say(id, reply);
            }
        }
        WM_CHAT_REPLY => {
            if let Some(reply) = mailbox::take::<Reply>(hwnd, msg) {
                app.on_chat_reply(reply);
            }
        }
        WM_DISPLAYCHANGE => {
            app.hide_bubble();
            app.on_display_change();
        }
        WM_SETTINGCHANGE if wp as u32 == SPI_SETWORKAREA => app.on_display_change(),
        WM_ENDSESSION if wp != 0 => app.save_state(), // Windows desligando
        WM_DESTROY => {
            app.shutdown();
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            PostQuitMessage(0);
        }
        m if m != 0 && m == app.taskbar_created => app.tray.show(&app.tip()), // Explorer reiniciou
        _ => return DefWindowProcW(hwnd, msg, wp, lp),
    }
    0
}
