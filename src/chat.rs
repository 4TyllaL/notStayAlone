//! Conversa com o mascote: uma caixinha de texto acima dele e o que vai para o
//! plugin de conversa (veja `plugins`), que responde em outro processo.
//!
//! O plugin recebe no stdin
//!   {"system": "...", "messages": [{"role": "user"|"assistant", "content": "..."}]}
//! e responde o texto no stdout (erro: mensagem no stderr e código != 0).

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
        Controls::{EM_LIMITTEXT, EM_SETCUEBANNER},
        Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE, VK_RETURN},
        WindowsAndMessaging::*,
    },
};

use crate::{
    ai::json::quote,
    mailbox,
    win::{clean_line, text_of, ui_font, w},
};

/// Texto digitado na caixinha (`String` no `mailbox`), enviado ao app.
pub const WM_CHAT_SEND: u32 = WM_APP + 6;
/// Resposta do plugin de conversa (`plugins::Reply` no `mailbox`).
pub const WM_CHAT_REPLY: u32 = WM_APP + 7;

/// Quantas mensagens anteriores vão junto (a "memória" da conversa).
pub const HISTORY: usize = 12;
/// Respostas maiores que isso são cortadas: o balão é pequeno.
const MAX_REPLY: usize = 350;

const WIDTH: i32 = 300;
const HEIGHT: i32 = 36;

thread_local! {
    /// (janela, campo de texto) da caixinha aberta.
    static INPUT: Cell<(HWND, HWND)> = const { Cell::new((null_mut(), null_mut())) };
}

/// Um trecho da conversa: (é você?, texto).
pub type Turn = (bool, String);

pub fn system_prompt(name: &str, about: &str, female: bool) -> String {
    let gender = if female { " Fale de si mesma sempre no feminino." } else { "" };
    format!(
        "Você é {name}, {about}. Você vive na área de trabalho do usuário como mascote \
         virtual e faz companhia durante o dia.{gender} Responda sempre em português do \
         Brasil, em no máximo duas frases curtas (até uns 200 caracteres), com carinho, bom \
         humor e o jeitinho de {name}. Não use emojis, markdown nem listas: sua fala aparece \
         num balãozinho pequeno."
    )
}

/// Pedido para a IA desenhar um mascote novo no formato do mascot.txt.
pub const DRAW_PROMPT: &str = "Você é um artista de pixel art. Desenhe um mascote fofo de 16x16 \
pixels, de frente e olhando um pouquinho para a direita, que vai ficar em pé em cima da barra \
de tarefas, a partir da descrição do usuário.
Responda SOMENTE neste formato, sem explicações e sem blocos de código:
name <nome curto do mascote>
article <o se for ele, a se for ela>
about <quem ele é em poucas palavras, ex.: um polvo roxo curioso>
color <letra> <RRGGBB>
(de 4 a 10 linhas color; use a letra k para o contorno escuro e a letra e para os olhos)
frame idle
<16 linhas com exatamente 16 caracteres cada: '.' é transparente, as letras são as cores>
Regras: contorno escuro em volta do corpo; os pés encostam na última linha; o personagem \
ocupa a maior parte da grade; olhos de 2 pixels de altura com a letra e; nada de texto na \
grade.";

/// Monta o JSON que o plugin recebe (`max_tokens` opcional pede mais espaço na resposta).
pub fn payload(system: &str, history: &[Turn], max_tokens: Option<u32>) -> String {
    let messages: Vec<String> = history
        .iter()
        .map(|(user, text)| {
            let role = if *user { "user" } else { "assistant" };
            format!("{{\"role\":\"{role}\",\"content\":{}}}", quote(text))
        })
        .collect();
    let extra = max_tokens.map_or(String::new(), |n| format!(",\"max_tokens\":{n}"));
    format!("{{\"system\":{},\"messages\":[{}]{extra}}}", quote(system), messages.join(","))
}

/// Corta a resposta num tamanho que cabe no balão (e tira caracteres de controle).
pub fn shorten(text: &str) -> String {
    let text = clean_line(text, usize::MAX);
    if text.chars().count() <= MAX_REPLY {
        return text;
    }
    let cut: String = text.chars().take(MAX_REPLY).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head).to_string();
    cut + "…"
}

// --- caixinha de texto --------------------------------------------------------

/// Retângulo da caixinha aberta (para o balão aparecer acima dela).
pub fn input_rect() -> Option<RECT> {
    let (window, _) = INPUT.with(Cell::get);
    if window.is_null() {
        return None;
    }
    let mut r: RECT = unsafe { zeroed() };
    unsafe { GetWindowRect(window, &mut r) };
    Some(r)
}

/// Abre a caixinha centralizada em `center_x`, com a base em `bottom`.
pub unsafe fn open(owner: HWND, placeholder: &str, center_x: i32, bottom: i32, work: RECT, dpi: u32) {
    let (existing, edit) = INPUT.with(Cell::get);
    if !existing.is_null() {
        SetForegroundWindow(existing);
        SetFocus(edit);
        return;
    }
    let s = |v: i32| v * dpi as i32 / 96;
    let hinstance = GetModuleHandleW(null());
    let class = w("StayAloneChatInput");
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(proc),
        hInstance: hinstance,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: GetSysColorBrush(COLOR_WINDOW),
        lpszClassName: class.as_ptr(),
        ..zeroed()
    };
    RegisterClassExW(&wc);

    let (width, height) = (s(WIDTH), s(HEIGHT));
    let x = (center_x - width / 2).clamp(work.left, (work.right - width).max(work.left));
    let y = (bottom - height).max(work.top);
    let window = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        class.as_ptr(),
        w("Conversar").as_ptr(),
        WS_POPUP | WS_BORDER,
        x,
        y,
        width,
        height,
        null_mut(),
        null_mut(),
        hinstance,
        null(),
    );
    if window.is_null() {
        return;
    }
    SetWindowLongPtrW(window, GWLP_USERDATA, owner as isize);
    let edit = CreateWindowExW(
        0,
        w("EDIT").as_ptr(),
        null(),
        WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL as u32,
        s(8),
        s(8),
        width - s(16) - 2,
        s(20),
        window,
        null_mut(),
        hinstance,
        null(),
    );
    let font = ui_font(s(15), FW_NORMAL);
    SendMessageW(edit, WM_SETFONT, font as WPARAM, 1);
    SetPropW(window, w("font").as_ptr(), font);
    SendMessageW(edit, EM_SETCUEBANNER, 1, w(placeholder).as_ptr() as LPARAM);
    SendMessageW(edit, EM_LIMITTEXT, 500, 0);
    INPUT.with(|i| i.set((window, edit)));
    ShowWindow(window, SW_SHOW);
    SetForegroundWindow(window);
    SetFocus(edit);
}

/// Enter envia, Esc fecha. Chamado no laço de mensagens antes de despachar.
pub fn pre_translate(msg: &MSG) -> bool {
    let (window, edit) = INPUT.with(Cell::get);
    if window.is_null() || msg.hwnd != edit || msg.message != WM_KEYDOWN {
        return false;
    }
    unsafe {
        match msg.wParam as u16 {
            VK_RETURN => {
                let text = text_of(edit);
                if !text.is_empty() {
                    SetWindowTextW(edit, w("").as_ptr());
                    let owner = GetWindowLongPtrW(window, GWLP_USERDATA) as HWND;
                    mailbox::post(owner, WM_CHAT_SEND, text);
                }
                true
            }
            VK_ESCAPE => {
                DestroyWindow(window);
                true
            }
            _ => false,
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        // Clicou fora: some.
        WM_ACTIVATE if (wp & 0xFFFF) as u32 == WA_INACTIVE => {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
            0
        }
        WM_DESTROY => {
            let font = RemovePropW(hwnd, w("font").as_ptr());
            if !font.is_null() {
                DeleteObject(font);
            }
            INPUT.with(|i| i.set((null_mut(), null_mut())));
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_escapes_text() {
        let p = payload("Seja o \"Lance\"", &[(true, "oi\nde novo".into()), (false, "au!".into())], None);
        assert_eq!(
            p,
            r#"{"system":"Seja o \"Lance\"","messages":[{"role":"user","content":"oi\nde novo"},{"role":"assistant","content":"au!"}]}"#
        );
    }

    #[test]
    fn long_replies_are_cut_at_a_word() {
        let long = "palavra ".repeat(100);
        let cut = shorten(&long);
        assert!(cut.ends_with('…'));
        assert!(cut.chars().count() <= MAX_REPLY + 1);
        assert!(!cut.contains("  "));
        assert_eq!(shorten("  curta\n  "), "curta");
    }
}
