//! Página Conversa.

use super::*;

// --- conversa -------------------------------------------------------------------

pub(super) unsafe fn show_chat_fields(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    set_text(hwnd, IDC_BASE, &st.chat.api_base);
    set_text(hwnd, IDC_MODEL, &st.chat.model);
    set_text(hwnd, IDC_KEYENV, &st.chat.key_env);
    show_key_status(hwnd);
}

pub(super) unsafe fn show_key_status(hwnd: HWND) {
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
pub(super) unsafe fn chat_field_error(hwnd: HWND, id: i32, text: &str) -> bool {
    show_page(hwnd, Page::Chat as usize);
    info(hwnd, text);
    SetFocus(item(hwnd, id));
    false
}

/// Lê os campos da aba Conversa, grava o chat.ini e a chave (se colada).
pub(super) unsafe fn save_chat(hwnd: HWND) -> bool {
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

pub(super) unsafe fn on_chat_command(hwnd: HWND, id: i32) {
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

pub(super) fn hello_payload() -> String {
    chat::payload(tr("Responda em uma frase curta, em português."), &[(true, tr("Diga oi!").into())], None)
}

