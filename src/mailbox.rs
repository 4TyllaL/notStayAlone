//! Correio interno: leva dados entre as janelas e threads do app sem pôr
//! ponteiros nas mensagens do Windows.
//!
//! Qualquer programa da sessão pode postar mensagens para as nossas janelas.
//! Se o conteúdo viajasse no `lparam` como ponteiro, uma mensagem forjada faria
//! o app ler (e liberar) memória arbitrária. Aqui a mensagem só avisa "chegou
//! correspondência"; o conteúdo fica guardado dentro do processo, e uma mensagem
//! forjada encontra a caixa vazia e é ignorada.

use std::{
    any::Any,
    collections::VecDeque,
    sync::{Mutex, MutexGuard},
};

use windows_sys::Win32::{Foundation::HWND, UI::WindowsAndMessaging::PostMessageW};

struct Letter {
    to: isize,
    msg: u32,
    value: Box<dyn Any + Send>,
}

static MAILBOX: Mutex<VecDeque<Letter>> = Mutex::new(VecDeque::new());

fn mailbox() -> MutexGuard<'static, VecDeque<Letter>> {
    MAILBOX.lock().unwrap_or_else(|e| e.into_inner())
}

/// Guarda `value` para a janela `to` e a avisa com `msg`. Pode ser chamada de qualquer thread.
pub fn post<T: Send + 'static>(to: HWND, msg: u32, value: T) {
    let key = to as isize;
    mailbox().push_back(Letter { to: key, msg, value: Box::new(value) });
    if unsafe { PostMessageW(to, msg, 0, 0) } == 0 {
        // A janela já fechou: ninguém vai buscar.
        let mut letters = mailbox();
        if let Some(i) = letters.iter().rposition(|l| l.to == key && l.msg == msg) {
            letters.remove(i);
        }
    }
}

/// Retira a correspondência mais antiga de `msg` para `to` (`None` = mensagem forjada ou repetida).
pub fn take<T: 'static>(to: HWND, msg: u32) -> Option<T> {
    let mut letters = mailbox();
    let i = letters.iter().position(|l| l.to == to as isize && l.msg == msg)?;
    letters.remove(i)?.value.downcast::<T>().ok().map(|v| *v)
}

/// Descarta o que sobrou para uma janela que está fechando.
pub fn discard(to: HWND) {
    mailbox().retain(|l| l.to != to as isize);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_are_delivered_in_order_and_only_once() {
        let to = 0x1234 as HWND; // janela inexistente: `post` descartaria, então enfileira direto
        for text in ["a", "b"] {
            mailbox().push_back(Letter { to: to as isize, msg: 7, value: Box::new(text.to_string()) });
        }
        assert_eq!(take::<String>(to, 8), None);
        assert_eq!(take::<String>(to, 7).as_deref(), Some("a"));
        assert_eq!(take::<String>(to, 7).as_deref(), Some("b"));
        assert_eq!(take::<String>(to, 7), None);
    }

    #[test]
    fn posting_to_a_closed_window_leaves_nothing_behind() {
        let gone = 0x5678 as HWND;
        post(gone, 9, String::from("x"));
        assert_eq!(take::<String>(gone, 9), None);
    }
}
