//! Pintura: barra lateral, título da página e cartões.

use super::*;

// --- pintura: barra lateral, título da página e cartões ------------------------------

/// Item `i` da navegação, em pixels.
pub(super) fn nav_rect(i: usize, dpi: u32) -> RECT {
    let top = NAV_TOP + i as i32 * NAV_STEP;
    RECT { left: scale(12, dpi), top: scale(top, dpi), right: scale(SIDEBAR - 12, dpi), bottom: scale(top + NAV_H, dpi) }
}

/// O item "Sobre", no rodapé da barra lateral.
pub(super) fn about_rect(dpi: u32) -> RECT {
    RECT { left: scale(12, dpi), top: scale(HEIGHT - 16 - NAV_H, dpi), right: scale(SIDEBAR - 12, dpi), bottom: scale(HEIGHT - 16, dpi) }
}

pub(super) fn nav_hit(lp: LPARAM, dpi: u32) -> Option<usize> {
    let (x, y) = ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
    let inside = |r: RECT| x >= r.left && x < r.right && y >= r.top && y < r.bottom;
    (0..PAGES.len()).find(|&i| inside(nav_rect(i, dpi))).or(inside(about_rect(dpi)).then_some(ABOUT))
}

/// Data de lançamento (gravada na compilação), no formato do idioma.
pub(super) fn release_date() -> String {
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

pub(super) unsafe fn paint(hwnd: HWND) {
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
pub(super) fn button_kind(id: i32) -> ui::Kind {
    match id {
        IDOK | IDC_ADD | IDC_TEST | IDC_SAVE_MASCOT | IDC_AI_GO | IDC_PLUGIN_TEST | IDC_GALLERY_INSTALL | IDC_ABOUT_UPDATE => {
            ui::Kind::Primary
        }
        IDC_MODS | IDC_GETKEY | IDC_PLUGIN_FOLDER | IDC_ABOUT_PROJECTS | IDC_ABOUT_GITHUB => ui::Kind::Ghost,
        IDC_REMOVE | IDC_KEY_REMOVE | IDC_MEMORY_CLEAR => ui::Kind::Danger,
        _ => ui::Kind::Secondary,
    }
}

pub(super) unsafe fn draw_button(hwnd: HWND, di: &DRAWITEMSTRUCT) {
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
pub(super) fn preview_of(info: Option<&PackInfo>) -> (String, Vec<u32>) {
    let Some(pack) = info.and_then(|i| pack::load(i).ok()) else { return (String::new(), vec![0; PIXELS]) };
    (pack.art.name.clone(), pack.art.pixels(Frame::Idle))
}

pub(super) unsafe fn refresh_preview(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let chosen = st.packs.get(combo_index(hwnd, IDC_MASCOT));
    st.preview = preview_of(chosen);
    let side = RECT { left: 0, top: 0, right: scale(SIDEBAR, st.dpi), bottom: scale(NAV_TOP, st.dpi) };
    InvalidateRect(hwnd, &side, 0);
}

pub(super) unsafe fn set_nav_hover(hwnd: HWND, hover: Option<usize>) {
    let Some(st) = state(hwnd) else { return };
    if st.nav_hover != hover {
        st.nav_hover = hover;
        let side = RECT { left: 0, top: 0, right: scale(SIDEBAR, st.dpi), bottom: scale(HEIGHT, st.dpi) };
        InvalidateRect(hwnd, &side, 0);
    }
}

pub(super) unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
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

