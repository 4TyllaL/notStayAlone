//! Ícone da bandeja, desenhado a partir do sprite do mascote (dispensa .ico).

use std::mem::{size_of, zeroed};

use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{CreateBitmap, DeleteObject},
    UI::{
        Shell::*,
        WindowsAndMessaging::{CreateIconIndirect, DestroyIcon, GetSystemMetrics, HICON, ICONINFO, SM_CXSMICON, WM_APP},
    },
};

use crate::{gfx, sprite::{Art, Frame}};

/// Cliques no ícone chegam à janela do mascote com esta mensagem (lparam = evento do mouse).
pub const WM_TRAY: u32 = WM_APP + 1;

pub struct Tray {
    hwnd: HWND,
    icon: HICON,
}

impl Tray {
    pub unsafe fn new(hwnd: HWND, art: &Art) -> Tray {
        Tray { hwnd, icon: make_icon(art) }
    }

    /// Coloca o ícone na bandeja (também depois que o Explorer reinicia).
    pub unsafe fn show(&self, tip: &str) {
        let mut nid = self.data(NIF_ICON | NIF_MESSAGE | NIF_TIP, tip);
        nid.uCallbackMessage = WM_TRAY;
        Shell_NotifyIconW(NIM_ADD, &nid);
    }

    pub unsafe fn set_tip(&self, tip: &str) {
        Shell_NotifyIconW(NIM_MODIFY, &self.data(NIF_TIP, tip));
    }

    /// Troca o desenho (outro mascote).
    pub unsafe fn set_art(&mut self, art: &Art, tip: &str) {
        DestroyIcon(self.icon);
        self.icon = make_icon(art);
        Shell_NotifyIconW(NIM_MODIFY, &self.data(NIF_ICON | NIF_TIP, tip));
    }

    pub unsafe fn remove(&self) {
        Shell_NotifyIconW(NIM_DELETE, &self.data(0, ""));
    }

    unsafe fn data(&self, flags: NOTIFY_ICON_DATA_FLAGS, tip: &str) -> NOTIFYICONDATAW {
        let mut nid: NOTIFYICONDATAW = zeroed();
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.hwnd;
        nid.uID = 1;
        nid.uFlags = flags;
        nid.hIcon = self.icon;
        let wide: Vec<u16> = tip.encode_utf16().collect();
        let n = wide.len().min(nid.szTip.len() - 1);
        nid.szTip[..n].copy_from_slice(&wide[..n]);
        nid
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe { DestroyIcon(self.icon) };
    }
}

unsafe fn make_icon(art: &Art) -> HICON {
    let size = GetSystemMetrics(SM_CXSMICON).max(16);
    let (color, bits) = gfx::create_dib(size, size);
    if bits.is_null() {
        return std::ptr::null_mut();
    }
    let pixels = art.sheet.draw_fit(art.index(Frame::Idle), size as usize);
    std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits, pixels.len());
    let mask_bits = vec![0u8; (size as usize).div_ceil(16) * 2 * size as usize];
    let mask = CreateBitmap(size, size, 1, 1, mask_bits.as_ptr().cast());
    let info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
    let icon = CreateIconIndirect(&info);
    DeleteObject(mask);
    DeleteObject(color);
    icon
}
