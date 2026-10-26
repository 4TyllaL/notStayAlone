//! Página Lembretes.

use super::*;

// --- lembretes ----------------------------------------------------------------

pub(super) unsafe fn fill_list(hwnd: HWND, select: Option<usize>) {
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

pub(super) unsafe fn set_state(list: HWND, index: usize, mask: u32, state: u32) {
    let mut row: LVITEMW = zeroed();
    row.stateMask = mask;
    row.state = state;
    SendMessageW(list, LVM_SETITEMSTATE, index, &row as *const _ as LPARAM);
}

pub(super) unsafe fn selected(hwnd: HWND) -> Option<usize> {
    let i = SendMessageW(item(hwnd, IDC_LIST), LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as LPARAM);
    (i >= 0).then_some(i as usize)
}

/// Mostra o lembrete selecionado nos campos de edição.
pub(super) unsafe fn show_selection(hwnd: HWND, index: Option<usize>) {
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
pub(super) unsafe fn number(hwnd: HWND, id: i32, range: std::ops::RangeInclusive<u32>, what: &str) -> Option<u32> {
    let value = text_of(item(hwnd, id)).parse::<u32>().ok().filter(|v| range.contains(v));
    if value.is_none() {
        info(hwnd, &fill(tr("{}: use um número entre {} e {}."), &[&what, range.start(), range.end()]));
        SetFocus(item(hwnd, id));
    }
    value
}

pub(super) unsafe fn on_reminder_command(hwnd: HWND, id: i32) {
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

pub(super) unsafe fn on_list_change(hwnd: HWND, nm: &NMLISTVIEW) {
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

