//! Superfícies de desenho para janelas em camadas (BGRA com alpha pré-multiplicado).

use std::{
    mem::{size_of, zeroed},
    ptr::null_mut,
};

use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT, SIZE},
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA},
};

/// Bitmap 32 bpp top-down; `bits` aponta para os pixels BGRA.
pub unsafe fn create_dib(width: i32, height: i32) -> (HBITMAP, *mut u32) {
    let mut bi: BITMAPINFO = zeroed();
    bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = width;
    bi.bmiHeader.biHeight = -height;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB as _;
    let mut bits = null_mut();
    let bmp = CreateDIBSection(null_mut(), &bi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    (bmp, bits.cast())
}

pub struct Canvas {
    pub dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u32,
    pub width: i32,
    pub height: i32,
}

impl Canvas {
    pub unsafe fn new(width: i32, height: i32) -> Canvas {
        let dc = CreateCompatibleDC(null_mut());
        let (bmp, bits) = create_dib(width, height);
        // Sem bitmap não há como desenhar; melhor parar do que escrever em lugar nenhum.
        assert!(!bits.is_null(), "não consegui criar o bitmap {width}×{height}");
        let old = SelectObject(dc, bmp);
        Canvas { dc, bmp, old, bits, width, height }
    }

    pub fn pixels(&mut self) -> &mut [u32] {
        unsafe { std::slice::from_raw_parts_mut(self.bits, (self.width * self.height) as usize) }
    }

    /// Preenche um retângulo (recortado aos limites do canvas).
    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        let (cw, ch) = (self.width, self.height);
        let (x0, y0) = (x.max(0), y.max(0));
        let (x1, y1) = ((x + w).min(cw), (y + h).min(ch));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let pixels = self.pixels();
        for row in y0..y1 {
            let start = (row * cw) as usize;
            pixels[start + x0 as usize..start + x1 as usize].fill(color);
        }
    }

    /// Envia o conteúdo para a janela em camadas. Com `pos = None` só troca o
    /// desenho, sem mover — mover janelas faz o Windows reavaliar o mouse e
    /// reexibir o ponteiro escondido durante a digitação. O tamanho vai sempre:
    /// sem ele o Windows ignora o bitmap novo (o desenho só mudava ao andar).
    pub unsafe fn present(&self, hwnd: HWND, pos: Option<(i32, i32)>) {
        self.present_faded(hwnd, pos, 255);
    }

    /// Como `present`, com a janela toda `alpha`/255 visível (para aparecer suavemente).
    pub unsafe fn present_faded(&self, hwnd: HWND, pos: Option<(i32, i32)>, alpha: u8) {
        let size = SIZE { cx: self.width, cy: self.height };
        let dst = pos.map(|(x, y)| POINT { x, y });
        let src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: alpha,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let dst = dst.as_ref().map_or(std::ptr::null(), |d| d as *const POINT);
        UpdateLayeredWindow(hwnd, null_mut(), dst, &size, self.dc, &src, 0, &blend, ULW_ALPHA);
    }
}

/// Lado de um sprite quadrado a partir do número de pixels.
fn sprite_side(pixels: &[u32]) -> usize {
    (pixels.len() as f64).sqrt().round().max(1.0) as usize
}

/// Desenho com bordas suaves, para as partes da interface (painel e configurações).
/// Cores em 0xAARRGGBB; o alpha da cor é a opacidade do que é pintado por cima.
impl Canvas {
    /// Mistura `color` no pixel `i` com a cobertura `coverage` (0 a 1).
    fn blend(&mut self, i: usize, color: u32, coverage: f32) {
        let a = ((color >> 24) as f32 / 255.0) * coverage.clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let dst = self.pixels()[i];
        let mix = |shift: u32| {
            let src = ((color >> shift) & 0xFF) as f32 * a;
            let under = ((dst >> shift) & 0xFF) as f32 * (1.0 - a);
            ((src + under).round() as u32).min(255) << shift
        };
        let alpha = (((a * 255.0) + ((dst >> 24) as f32) * (1.0 - a)).round() as u32).min(255) << 24;
        self.pixels()[i] = alpha | mix(16) | mix(8) | mix(0);
    }

    /// Distância (com sinal) do ponto até a borda de um retângulo arredondado.
    fn round_distance(px: f32, py: f32, (x, y, w, h): (f32, f32, f32, f32), r: f32) -> f32 {
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let qx = (px - cx).abs() - (w / 2.0 - r);
        let qy = (py - cy).abs() - (h / 2.0 - r);
        let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
        outside + qx.max(qy).min(0.0) - r
    }

    /// Retângulo com cantos arredondados de raio `r`, borda antisserrilhada.
    pub fn round_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, color: u32) {
        let shape = (x as f32, y as f32, w as f32, h as f32);
        let r = (r as f32).min(w as f32 / 2.0).min(h as f32 / 2.0);
        for py in y.max(0)..(y + h).min(self.height) {
            for px in x.max(0)..(x + w).min(self.width) {
                let d = Self::round_distance(px as f32 + 0.5, py as f32 + 0.5, shape, r);
                if d < 0.5 {
                    self.blend((py * self.width + px) as usize, color, 0.5 - d);
                }
            }
        }
    }

    /// Retângulo arredondado com borda de 1 px (`border`) e miolo `fill`.
    pub fn card(&mut self, (x, y, w, h): (i32, i32, i32, i32), r: i32, fill: u32, border: u32) {
        self.round_rect(x, y, w, h, r, border);
        self.round_rect(x + 1, y + 1, w - 2, h - 2, (r - 1).max(0), fill);
    }

    /// Sombra suave em volta de um retângulo arredondado.
    pub fn shadow(&mut self, (x, y, w, h): (i32, i32, i32, i32), r: i32, blur: i32, opacity: f32) {
        let shape = (x as f32, y as f32, w as f32, h as f32);
        for py in (y - blur).max(0)..(y + h + blur).min(self.height) {
            for px in (x - blur).max(0)..(x + w + blur).min(self.width) {
                let d = Self::round_distance(px as f32 + 0.5, py as f32 + 0.5, shape, r as f32);
                let t = (1.0 - d.max(0.0) / blur as f32).clamp(0.0, 1.0);
                self.blend((py * self.width + px) as usize, 0xFF00_0000, opacity * t * t);
            }
        }
    }

    /// Texto GDI. O alpha dos pixels é preservado (o GDI o apagaria), então só escreva
    /// sobre áreas já pintadas.
    pub unsafe fn text(&mut self, font: HFONT, text: &str, rect: RECT, rgb: u32, flags: DRAW_TEXT_FORMAT) {
        // Só a área do texto precisa ter o alpha guardado e restaurado.
        let (x0, x1) = (rect.left.clamp(0, self.width) as usize, rect.right.clamp(0, self.width) as usize);
        let (y0, y1) = (rect.top.clamp(0, self.height) as usize, rect.bottom.clamp(0, self.height) as usize);
        let width = self.width as usize;
        let area = |y: usize| y * width + x0..y * width + x1;
        let alphas: Vec<u32> = (y0..y1).flat_map(|y| self.pixels()[area(y)].iter().map(|p| p & 0xFF00_0000).collect::<Vec<_>>()).collect();
        let old = SelectObject(self.dc, font);
        SetBkMode(self.dc, TRANSPARENT as _);
        SetTextColor(self.dc, crate::theme::colorref(rgb));
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let mut rect = rect;
        DrawTextW(self.dc, wide.as_mut_ptr(), wide.len() as i32, &mut rect, flags | DT_NOPREFIX);
        SelectObject(self.dc, old);
        GdiFlush();
        let mut saved = alphas.into_iter();
        for y in y0..y1 {
            for p in &mut self.pixels()[area(y)] {
                *p = (*p & 0x00FF_FFFF) | saved.next().unwrap_or(0xFF00_0000);
            }
        }
    }

    /// Desenha um sprite quadrado (16×16 ou 32×32; pixels 0xAARRGGBB, 0 = transparente)
    /// ampliado `scale` vezes.
    pub fn sprite(&mut self, x: i32, y: i32, scale: i32, pixels: &[u32]) {
        let side = sprite_side(pixels);
        for (i, &c) in pixels.iter().enumerate() {
            if c >> 24 != 0 {
                let (sx, sy) = ((i % side) as i32, (i / side) as i32);
                self.fill(x + sx * scale, y + sy * scale, scale, scale, c);
            }
        }
    }

    /// Sprite centralizado num quadrado de `box_px`, na maior escala inteira que cabe.
    pub fn sprite_fit(&mut self, x: i32, y: i32, box_px: i32, pixels: &[u32]) {
        let side = sprite_side(pixels) as i32;
        let scale = (box_px / side).max(1);
        let off = (box_px - side * scale) / 2;
        self.sprite(x + off, y + off, scale, pixels);
    }

    /// Copia o canvas para um DC comum (janelas que não são em camadas).
    pub unsafe fn blit(&self, dc: HDC, x: i32, y: i32) {
        BitBlt(dc, x, y, self.width, self.height, self.dc, 0, 0, SRCCOPY);
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bmp);
            DeleteDC(self.dc);
        }
    }
}
