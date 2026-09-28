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
        Controls::{EM_LIMITTEXT, EM_SETCUEBANNER, WM_MOUSELEAVE},
        Input::KeyboardAndMouse::{SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_ESCAPE, VK_RETURN},
        WindowsAndMessaging::*,
    },
};

use crate::{
    ai::json::quote,
    gfx::Canvas,
    mailbox,
    theme::{self, argb, icon},
    win::{self, clean_line, text_of, ui_font, w},
};

/// Texto digitado na caixinha (`String` no `mailbox`), enviado ao app.
pub const WM_CHAT_SEND: u32 = WM_APP + 6;
/// Resposta do plugin de conversa (`child::Reply` no `mailbox`).
pub const WM_CHAT_REPLY: u32 = WM_APP + 7;

/// Quantas mensagens anteriores vão junto (a "memória" da conversa).
pub const HISTORY: usize = 12;
/// Respostas maiores que isso são cortadas: o balão é pequeno.
const MAX_REPLY: usize = 350;

const WIDTH: i32 = 340;
const HEIGHT: i32 = 48;

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
         Brasil, com acentuação correta, em no máximo duas frases curtas (até uns 200 caracteres), com carinho, bom \
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
//
// Uma janela comum (campos de texto não aparecem em janelas em camadas), pintada
// no visual do painel: cantos arredondados e sombra do Windows, ícone de conversa
// à esquerda, campo sem borda e um botão redondo de enviar.

/// Id do campo de texto (para as notificações de mudança).
const IDC_EDIT: i32 = 1;

struct Input {
    owner: HWND,
    edit: HWND,
    dpi: u32,
    font: HFONT,
    icons: HFONT,
    /// Fundo do campo de texto (a cor do cartão).
    brush: HBRUSH,
    /// Mouse em cima do botão de enviar.
    hover: bool,
}

impl Input {
    fn s(&self, v: i32) -> i32 {
        v * self.dpi as i32 / 96
    }

    /// Botão de enviar, no canto direito.
    unsafe fn send_rect(&self, window: HWND) -> RECT {
        let mut client: RECT = zeroed();
        GetClientRect(window, &mut client);
        let size = self.s(32);
        let (x, y) = (client.right - self.s(8) - size, (client.bottom - size) / 2);
        RECT { left: x, top: y, right: x + size, bottom: y + size }
    }
}

unsafe fn input_of<'a>(window: HWND) -> Option<&'a mut Input> {
    (GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Input).as_mut()
}

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
        style: CS_DROPSHADOW,
        lpfnWndProc: Some(proc),
        hInstance: hinstance,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
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
        WS_POPUP | WS_CLIPCHILDREN,
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
    win::round_corners(window, theme::BORDER);
    // Campo sem borda entre o ícone (à esquerda) e o botão de enviar (à direita).
    let font = ui_font(s(15), FW_NORMAL);
    let edit_h = s(22);
    let edit = CreateWindowExW(
        0,
        w("EDIT").as_ptr(),
        null(),
        WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL as u32,
        s(52),
        (height - edit_h) / 2,
        width - s(52) - s(50),
        edit_h,
        window,
        IDC_EDIT as isize as HMENU,
        hinstance,
        null(),
    );
    SendMessageW(edit, WM_SETFONT, font as WPARAM, 1);
    SendMessageW(edit, EM_SETCUEBANNER, 1, w(placeholder).as_ptr() as LPARAM);
    SendMessageW(edit, EM_LIMITTEXT, 500, 0);
    let input = Box::new(Input {
        owner,
        edit,
        dpi,
        font,
        icons: theme::icon_font(s(16)),
        brush: CreateSolidBrush(theme::colorref(theme::CARD)),
        hover: false,
    });
    SetWindowLongPtrW(window, GWLP_USERDATA, Box::into_raw(input) as isize);
    INPUT.with(|i| i.set((window, edit)));
    ShowWindow(window, SW_SHOW);
    SetForegroundWindow(window);
    SetFocus(edit);
}

/// Manda o que foi digitado para o app e limpa o campo.
unsafe fn submit(window: HWND) {
    let Some(input) = input_of(window) else { return };
    let text = text_of(input.edit);
    if !text.is_empty() {
        SetWindowTextW(input.edit, w("").as_ptr());
        mailbox::post(input.owner, WM_CHAT_SEND, text);
    }
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
                submit(window);
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

unsafe fn paint(window: HWND) {
    let Some(input) = input_of(window) else { return };
    let mut ps: PAINTSTRUCT = zeroed();
    let dc = BeginPaint(window, &mut ps);
    let mut client: RECT = zeroed();
    GetClientRect(window, &mut client);
    let mut c = Canvas::new(client.right.max(1), client.bottom.max(1));
    c.fill(0, 0, c.width, c.height, argb(theme::CARD));
    let center = DT_CENTER | DT_VCENTER | DT_SINGLELINE;

    // Ícone de conversa num círculo suave.
    let bubble = input.s(30);
    let (bx, by) = (input.s(12), (client.bottom - bubble) / 2);
    c.round_rect(bx, by, bubble, bubble, bubble / 2, argb(theme::ACCENT_SOFT));
    let icon_rect = RECT { left: bx, top: by, right: bx + bubble, bottom: by + bubble };
    c.text(input.icons, &icon::CHAT.to_string(), icon_rect, theme::ACCENT, center);

    // Botão de enviar: laranja quando há texto, apagado quando o campo está vazio.
    let send = input.send_rect(window);
    let size = send.right - send.left;
    let empty = GetWindowTextLengthW(input.edit) == 0;
    let (fill, ink) = match (empty, input.hover) {
        (true, _) => (theme::SOFT, theme::DISABLED),
        (false, true) => (theme::ACCENT_DARK, theme::CARD),
        (false, false) => (theme::ACCENT, theme::CARD),
    };
    c.round_rect(send.left, send.top, size, size, size / 2, argb(fill));
    c.text(input.icons, &icon::SEND.to_string(), send, ink, center);

    c.blit(dc, 0, 0);
    EndPaint(window, &ps);
}

unsafe fn over_send(window: HWND, lp: LPARAM) -> bool {
    let Some(input) = input_of(window) else { return false };
    let (x, y) = ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
    let r = input.send_rect(window);
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

unsafe fn set_hover(window: HWND, hover: bool) {
    let Some(input) = input_of(window) else { return };
    if input.hover != hover {
        input.hover = hover;
        let r = input.send_rect(window);
        InvalidateRect(window, &r, 0);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint(hwnd);
            0
        }
        // O campo com o fundo do cartão.
        WM_CTLCOLOREDIT => {
            let Some(input) = input_of(hwnd) else { return DefWindowProcW(hwnd, msg, wp, lp) };
            SetBkColor(wp as HDC, theme::colorref(theme::CARD));
            SetTextColor(wp as HDC, theme::colorref(theme::TEXT));
            input.brush as LRESULT
        }
        // Digitou ou apagou: o botão de enviar acende ou apaga.
        WM_COMMAND if (wp & 0xFFFF) as i32 == IDC_EDIT && ((wp >> 16) & 0xFFFF) as u32 == EN_CHANGE => {
            if let Some(input) = input_of(hwnd) {
                let r = input.send_rect(hwnd);
                InvalidateRect(hwnd, &r, 0);
            }
            0
        }
        WM_MOUSEMOVE => {
            set_hover(hwnd, over_send(hwnd, lp));
            let mut track = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
            TrackMouseEvent(&mut track);
            0
        }
        WM_MOUSELEAVE => {
            set_hover(hwnd, false);
            0
        }
        WM_SETCURSOR if input_of(hwnd).is_some_and(|i| i.hover) => {
            SetCursor(LoadCursorW(null_mut(), IDC_HAND));
            1
        }
        WM_LBUTTONUP => {
            if over_send(hwnd, lp) {
                submit(hwnd);
            }
            if let Some(input) = input_of(hwnd) {
                SetFocus(input.edit);
            }
            0
        }
        // Clicou fora: some.
        WM_ACTIVATE if (wp & 0xFFFF) as u32 == WA_INACTIVE => {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
            0
        }
        WM_DESTROY => {
            INPUT.with(|i| i.set((null_mut(), null_mut())));
            0
        }
        WM_NCDESTROY => {
            let input = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Input;
            if !input.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let input = Box::from_raw(input);
                for object in [input.font, input.icons, input.brush] {
                    DeleteObject(object);
                }
            }
            DefWindowProcW(hwnd, msg, wp, lp)
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
