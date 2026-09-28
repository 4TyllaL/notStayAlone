//! O que o app observa do sistema — só leituras baratas, sem hooks globais.
//! Nada aqui registra teclas: sabe-se apenas *quando* houve atividade.

use std::mem::{size_of, zeroed};

use windows_sys::Win32::{
    Foundation::{POINT, SYSTEMTIME},
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    System::{
        Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
        SystemInformation::{GetLocalTime, GetTickCount},
    },
    UI::{
        Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
        Shell::{SHQueryUserNotificationState, QUERY_USER_NOTIFICATION_STATE, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN},
        WindowsAndMessaging::GetCursorPos,
    },
};

use crate::mascot::Bounds;

pub unsafe fn cursor() -> POINT {
    let mut p = POINT { x: 0, y: 0 };
    GetCursorPos(&mut p);
    p
}

/// (AAAAMMDD, hora) no fuso local.
pub unsafe fn clock() -> (u32, u32) {
    let mut t: SYSTEMTIME = zeroed();
    GetLocalTime(&mut t);
    (t.wYear as u32 * 10_000 + t.wMonth as u32 * 100 + t.wDay as u32, t.wHour as u32)
}

/// Momento (GetTickCount) do último uso de teclado/mouse — não diz *o que* foi feito.
pub unsafe fn last_input() -> u32 {
    let mut info = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    GetLastInputInfo(&mut info);
    info.dwTime
}

/// Milissegundos desde `tick` (GetTickCount), à prova da volta do contador.
pub unsafe fn ms_since(tick: u32) -> u32 {
    GetTickCount().wrapping_sub(tick)
}

pub unsafe fn idle_secs() -> u64 {
    (ms_since(last_input()) / 1000) as u64
}

/// (na bateria?, porcentagem) — `None` em desktops sem bateria.
pub unsafe fn power() -> Option<(bool, u8)> {
    let mut s: SYSTEM_POWER_STATUS = zeroed();
    if GetSystemPowerStatus(&mut s) == 0 || s.BatteryFlag & 128 != 0 || s.BatteryFlag == 255 || s.BatteryLifePercent > 100 {
        return None;
    }
    Some((s.ACLineStatus == 0, s.BatteryLifePercent))
}

/// Área de trabalho (sem a barra de tarefas) do monitor mais perto de `p`.
pub unsafe fn bounds_at(p: POINT) -> Bounds {
    let mut mi: MONITORINFO = zeroed();
    mi.cbSize = size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST), &mut mi);
    let r = mi.rcWork;
    Bounds { left: r.left as f32, top: r.top as f32, right: r.right as f32, floor: r.bottom as f32 }
}

/// Jogo, vídeo em tela cheia ou apresentação: hora de sair da frente.
pub unsafe fn fullscreen_app_running() -> bool {
    let mut state: QUERY_USER_NOTIFICATION_STATE = 0;
    SHQueryUserNotificationState(&mut state) >= 0
        && matches!(state, QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE)
}
