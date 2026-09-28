//! Pequenos utilitários Win32 usados por todas as janelas do app.

use std::ptr::null_mut;

use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW, MessageBoxW, MB_ICONWARNING},
};

/// Texto em UTF-16 terminado em zero, para as APIs `...W`.
pub fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Aviso simples (sem janela dona).
pub fn message(text: &str) {
    unsafe { MessageBoxW(null_mut(), w(text).as_ptr(), w("!StayAlone").as_ptr(), MB_ICONWARNING) };
}

/// Fonte Segoe UI com `height` pixels de altura (quem cria apaga com `DeleteObject`).
pub unsafe fn ui_font(height: i32, weight: u32) -> HFONT {
    CreateFontW(
        -height,
        0,
        0,
        0,
        weight as _,
        0,
        0,
        0,
        DEFAULT_CHARSET as _,
        OUT_DEFAULT_PRECIS as _,
        CLIP_DEFAULT_PRECIS as _,
        CLEARTYPE_QUALITY as _,
        (DEFAULT_PITCH | FF_DONTCARE) as _,
        w("Segoe UI").as_ptr(),
    )
}

/// Texto de um controle, sem espaços nas pontas.
pub fn text_of(control: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(control);
        let mut buf = vec![0u16; len as usize + 1];
        let got = GetWindowTextW(control, buf.as_mut_ptr(), buf.len() as i32);
        let text = String::from_utf16_lossy(&buf[..got as usize]).trim().to_string();
        buf.fill(0); // pode ser a chave da API: não deixa cópia solta na memória
        text
    }
}

/// Função de uma DLL do Windows carregada só de System32 (nunca da pasta do .exe,
/// onde alguém poderia ter deixado uma DLL falsa).
pub unsafe fn system_proc(dll: &str, name: &core::ffi::CStr) -> Option<unsafe extern "system" fn() -> isize> {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
    let lib = LoadLibraryExW(w(dll).as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
    if lib.is_null() {
        return None;
    }
    GetProcAddress(lib, name.as_ptr().cast())
}

/// Como `system_proc`, para funções exportadas só por número.
pub unsafe fn system_proc_ordinal(dll: &str, ordinal: u16) -> Option<unsafe extern "system" fn() -> isize> {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
    let lib = LoadLibraryExW(w(dll).as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
    if lib.is_null() {
        return None;
    }
    GetProcAddress(lib, ordinal as usize as *const u8)
}

/// Muda um atributo de janela do DWM (visual do Windows 11; antes disso não faz nada).
unsafe fn dwm_set(hwnd: HWND, attribute: u32, value: u32) {
    type SetAttribute = unsafe extern "system" fn(HWND, u32, *const core::ffi::c_void, u32) -> i32;
    if let Some(proc) = system_proc("dwmapi.dll", c"DwmSetWindowAttribute") {
        let set: SetAttribute = std::mem::transmute::<unsafe extern "system" fn() -> isize, SetAttribute>(proc);
        set(hwnd, attribute, (&value as *const u32).cast(), 4);
    }
}

/// Barra de título clara ou escura (junto com o tema do app).
pub unsafe fn dark_title(hwnd: HWND, dark: bool) {
    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    dwm_set(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, dark as u32);
}

/// Pinta a barra de título com a cor da janela.
pub unsafe fn caption_color(hwnd: HWND, rgb: u32) {
    const DWMWA_CAPTION_COLOR: u32 = 35;
    dwm_set(hwnd, DWMWA_CAPTION_COLOR, crate::theme::colorref(rgb));
}

/// Cantos arredondados e borda fina de cor `border_rgb` (janelas sem moldura).
pub unsafe fn round_corners(hwnd: HWND, border_rgb: u32) {
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWCP_ROUND: u32 = 2;
    dwm_set(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND);
    dwm_set(hwnd, DWMWA_BORDER_COLOR, crate::theme::colorref(border_rgb));
}

/// Uma string UTF-16 terminada em zero (vinda do Windows) é igual a `text`?
pub unsafe fn wide_eq(ptr: *const u16, text: &str) -> bool {
    let mut len = 0;
    while len < 256 && *ptr.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)) == text
}

/// Tira caracteres de controle e limita o tamanho — para textos vindos de
/// arquivos de mods ou da IA, que vão parar em menus, balões e prompts.
pub fn clean_line(s: &str, max_chars: usize) -> String {
    // Também os controles de direção do texto (usados para disfarçar nomes).
    let bidi = |c: char| matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
    let s: String = s.chars().map(|c| if c.is_control() || bidi(c) { ' ' } else { c }).collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_line_strips_controls_and_limits() {
        assert_eq!(clean_line("  Oi\u{0}\tmundo\u{202e}!\r\n ", 50), "Oi mundo !");
        assert_eq!(clean_line("abcdef", 3), "abc");
    }
}
