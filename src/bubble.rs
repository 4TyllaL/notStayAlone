//! Balão de fala: uma segunda janela em camadas, desenhada só quando aparece
//! (o bitmap é descartado logo depois — não fica nada em memória).

use std::ptr::{null, null_mut};

use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::*,
};

use crate::{
    gfx::Canvas,
    mascot::Bounds,
    phrases::Topic,
    win::{ui_font, w},
};

/// Enviada à janela do mascote quando o balão é clicado.
pub const WM_BUBBLE_CLICK: u32 = WM_APP + 2;

const OUTLINE: u32 = 0xFF2B_1E2F;
const PAPER: u32 = 0xFFFF_FAF0;
const INK: COLORREF = 0x002F_1E2B; // mesmo tom do contorno, em 0x00BBGGRR

pub struct Bubble {
    hwnd: HWND,
    /// Tópico do balão visível (None = escondido).
    pub topic: Option<Topic>,
}

impl Bubble {
    pub unsafe fn new(owner: HWND, hinstance: HINSTANCE) -> Bubble {
        let class = w("StayAloneBubble");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(bubble_proc),
            hInstance: hinstance,
            hCursor: LoadCursorW(null_mut(), IDC_HAND),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassExW(&wc);
        // Janela "possuída" pelo mascote: fica sempre acima dele.
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            class.as_ptr(),
            null(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            owner,
            null_mut(),
            hinstance,
            null(),
        );
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, owner as isize);
        Bubble { hwnd, topic: None }
    }

    /// Mostra `text` com a ponta apontando para `anchor`. Retorna quanto tempo (ms) deixar visível.
    pub unsafe fn show(&mut self, text: &str, topic: Topic, anchor: (i32, i32), b: &Bounds, dpi: u32) -> u32 {
        let s = |v: i32| v * dpi as i32 / 96;
        let u = (dpi as i32 / 48).max(2); // um "pixel" do contorno
        let pad = s(8);
        let mut wide = w(text);

        let font = ui_font(s(14), FW_SEMIBOLD);
        let flags = DT_WORDBREAK | DT_NOPREFIX | DT_CENTER;

        // Mede o texto com largura máxima fixa.
        let measure = CreateCompatibleDC(null_mut());
        let old = SelectObject(measure, font);
        // Conversas têm respostas maiores: balão mais largo.
        let max_width = if matches!(topic, Topic::Chat | Topic::Plugin) { s(260) } else { s(200) };
        let mut text_rect = RECT { left: 0, top: 0, right: max_width, bottom: 0 };
        DrawTextW(measure, wide.as_mut_ptr(), -1, &mut text_rect, flags | DT_CALCRECT);
        SelectObject(measure, old);
        DeleteDC(measure);

        let (tw, th) = (text_rect.right, text_rect.bottom);
        let width = tw + 2 * (pad + u);
        let body = th + 2 * (pad + u);
        let height = body + 2 * u; // + rabinho

        let left = (anchor.0 - width / 2).clamp(b.left as i32, (b.right as i32 - width).max(b.left as i32));
        let top = (anchor.1 - height).max(b.top as i32);
        let tail = (anchor.0 - left).clamp(3 * u, width - 3 * u);

        let mut canvas = Canvas::new(width, height);
        canvas.pixels().fill(0);
        // Corpo com cantos arredondados em degrau, no estilo do sprite.
        canvas.fill(u, u, width - 2 * u, body - 2 * u, PAPER);
        canvas.fill(u, 0, width - 2 * u, u, OUTLINE);
        canvas.fill(u, body - u, width - 2 * u, u, OUTLINE);
        canvas.fill(0, u, u, body - 2 * u, OUTLINE);
        canvas.fill(width - u, u, u, body - 2 * u, OUTLINE);
        for (cx, cy) in [(u, u), (width - 2 * u, u), (u, body - 2 * u), (width - 2 * u, body - 2 * u)] {
            canvas.fill(cx, cy, u, u, OUTLINE);
        }
        // Rabinho em escada apontando para baixo.
        for k in 0..3 {
            let half = (2 - k) * u;
            let y = body - u + k * u;
            canvas.fill(tail - half, y, 2 * half, u, PAPER);
            canvas.fill(tail - half - u, y, u, u, OUTLINE);
            canvas.fill(tail + half, y, u, u, OUTLINE);
        }

        let old = SelectObject(canvas.dc, font);
        SetBkMode(canvas.dc, TRANSPARENT as _);
        SetTextColor(canvas.dc, INK);
        let mut rect = RECT { left: u + pad, top: u + pad, right: u + pad + tw, bottom: u + pad + th };
        DrawTextW(canvas.dc, wide.as_mut_ptr(), -1, &mut rect, flags);
        SelectObject(canvas.dc, old);
        DeleteObject(font);
        GdiFlush();
        // O GDI zera o alpha onde escreve; o miolo do balão é todo opaco.
        let cw = canvas.width;
        let pixels = canvas.pixels();
        for y in u..body - u {
            for p in &mut pixels[(y * cw + u) as usize..(y * cw + cw - u) as usize] {
                *p |= 0xFF00_0000;
            }
        }

        canvas.present(self.hwnd, Some((left, top)));
        ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        self.topic = Some(topic);

        if matches!(topic, Topic::Water | Topic::Stretch | Topic::Eyes | Topic::Reminder) {
            15_000
        } else if topic == Topic::Chat {
            // Tempo de leitura: resposta longa fica mais tempo (o "..." de espera fica até a resposta chegar).
            if text == "..." { 60_000 } else { (4_000 + 70 * text.chars().count() as u32).min(20_000) }
        } else {
            (3_000 + 60 * text.chars().count() as u32).min(10_000)
        }
    }

    pub unsafe fn hide(&mut self) {
        if self.topic.take().is_some() {
            ShowWindow(self.hwnd, SW_HIDE);
        }
    }
}

unsafe extern "system" fn bubble_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_LBUTTONUP => {
            let owner = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as HWND;
            PostMessageW(owner, WM_BUBBLE_CLICK, 0, 0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
