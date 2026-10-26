//! Página Plugins.

use super::*;

// --- plugins ----------------------------------------------------------------------

pub(super) unsafe fn fill_plugins(hwnd: HWND, select: Option<usize>) {
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

pub(super) fn selected_plugin(hwnd: HWND) -> Option<usize> {
    let i = unsafe { SendMessageW(item(hwnd, IDC_PLUGINS), LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as LPARAM) };
    (i >= 0).then_some(i as usize)
}

/// Descrição do plugin selecionado.
pub(super) unsafe fn show_plugin(hwnd: HWND, index: Option<usize>) {
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

pub(super) unsafe fn on_plugins_change(hwnd: HWND, nm: &NMLISTVIEW) {
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
pub(super) unsafe fn toggle_plugin(hwnd: HWND, i: usize, on: bool) {
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
pub(super) unsafe fn approve(hwnd: HWND, plugin: &Plugin) -> Option<Enabled> {
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
pub(super) unsafe fn set_result(hwnd: HWND, text: &str) {
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
pub(super) fn test_output(text: &str) -> String {
    use crate::win::clean_line;
    const MAX: usize = 4000;
    let lines: Vec<String> = text.lines().map(|l| clean_line(l, MAX)).filter(|l| !l.is_empty()).collect();
    let mut out = lines.join("\r\n");
    if out.chars().count() > MAX {
        out = out.chars().take(MAX).collect::<String>() + "…";
    }
    out
}

pub(super) unsafe fn on_plugin_command(hwnd: HWND, id: i32) {
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

