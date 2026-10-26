//! Galeria de mascotes da comunidade.

use super::*;

// --- galeria ----------------------------------------------------------------------

pub(super) unsafe fn load_gallery(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    if st.busy.is_some() {
        return;
    }
    st.busy = Some(Busy::GalleryList);
    set_text(hwnd, IDC_GALLERY_STATUS, tr("Buscando a galeria..."));
    gallery::list(hwnd, WM_CHAT_REPLY);
}

pub(super) unsafe fn fill_gallery(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let list = item(hwnd, IDC_GALLERY);
    SendMessageW(list, LVM_DELETEALLITEMS, 0, 0);
    for (i, e) in st.gallery.iter().enumerate() {
        insert_row(list, i, &[&e.name, &e.author]);
    }
    show_gallery_item(hwnd);
}

pub(super) fn selected_row(hwnd: HWND, id: i32) -> Option<usize> {
    let i = unsafe { SendMessageW(item(hwnd, id), LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as LPARAM) };
    (i >= 0).then_some(i as usize)
}

pub(super) unsafe fn show_gallery_item(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let entry = selected_row(hwnd, IDC_GALLERY).and_then(|i| st.gallery.get(i));
    set_text(hwnd, IDC_GALLERY_INFO, entry.map_or(tr("Selecione um item para ver o que ele faz."), |e| e.about.as_str()));
    EnableWindow(item(hwnd, IDC_GALLERY_INSTALL), (entry.is_some() && st.busy.is_none()) as BOOL);
}

pub(super) unsafe fn on_gallery_command(hwnd: HWND, id: i32) {
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

pub(super) unsafe fn fill_combo(hwnd: HWND, id: i32, labels: &[String], selected: usize) {
    let combo = item(hwnd, id);
    SendMessageW(combo, CB_RESETCONTENT, 0, 0);
    for label in labels {
        SendMessageW(combo, CB_ADDSTRING, 0, w(label).as_ptr() as LPARAM);
    }
    SendMessageW(combo, CB_SETCURSEL, selected, 0);
}

pub(super) unsafe fn combo_index(hwnd: HWND, id: i32) -> usize {
    SendMessageW(item(hwnd, id), CB_GETCURSEL, 0, 0).max(0) as usize
}

pub(super) unsafe fn populate(hwnd: HWND) {
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
pub(super) unsafe fn fill_mascot_combos(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let names: Vec<String> = st.packs.iter().map(|p| p.name.clone()).collect();
    let current = st.packs.iter().position(|p| p.id == st.draft.mascot).unwrap_or(0);
    fill_combo(hwnd, IDC_MASCOT, &names, current);
    let templates: Vec<String> = std::iter::once(tr("Desenho em branco").to_string()).chain(names).collect();
    fill_combo(hwnd, IDC_TEMPLATE, &templates, 0);
}

