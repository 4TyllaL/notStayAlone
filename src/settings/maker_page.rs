//! Página Criar mascote.

use super::*;

// --- criar mascote --------------------------------------------------------------

/// Coloca um desenho no editor (e nos campos de nome/personalidade).
pub(super) unsafe fn load_drawing(hwnd: HWND, drawing: Drawing, keep_name: bool) {
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

pub(super) unsafe fn on_maker_command(hwnd: HWND, id: i32, code: u32) {
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

pub(super) unsafe fn info_err(hwnd: HWND, e: &str) {
    info(hwnd, &fill(tr("Não consegui abrir esse mascote.\n\n{}"), &[&e]));
}

pub(super) unsafe fn save_mascot(hwnd: HWND) {
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

pub(super) unsafe fn on_reply(hwnd: HWND, reply: Reply) {
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

