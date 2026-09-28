//! O que o app observa do sistema — só leituras baratas, sem hooks globais.
//! Nada aqui registra teclas: sabe-se apenas *quando* houve atividade.

use std::mem::{size_of, zeroed};

use windows_sys::Win32::{
    Foundation::{CloseHandle, POINT, SYSTEMTIME},
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    System::{
        Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
        SystemInformation::{GetLocalTime, GetTickCount},
        Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
    },
    UI::{
        Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
        Shell::{SHQueryUserNotificationState, QUERY_USER_NOTIFICATION_STATE, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN},
        WindowsAndMessaging::{GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId},
    },
};

use crate::mascot::Bounds;

pub unsafe fn cursor() -> POINT {
    let mut p = POINT { x: 0, y: 0 };
    GetCursorPos(&mut p);
    p
}

/// (AAAAMMDD, hora, dia da semana com 0 = domingo) no fuso local.
pub unsafe fn clock() -> (u32, u32, u32) {
    let mut t: SYSTEMTIME = zeroed();
    GetLocalTime(&mut t);
    (t.wYear as u32 * 10_000 + t.wMonth as u32 * 100 + t.wDay as u32, t.wHour as u32, t.wDayOfWeek as u32)
}

/// Programas de reunião/chamada de vídeo, pelo nome do executável.
const MEETING_APPS: [&str; 9] = [
    "teams.exe", "ms-teams.exe", "zoom.exe", "webex.exe", "webexmta.exe", "ciscocollabhost.exe", "skype.exe",
    "g2mcomm.exe", "bluejeans.exe",
];

/// O programa em primeiro plano é de reunião? Olha só o nome do .exe —
/// nunca o título da janela nem o conteúdo da tela.
pub unsafe fn meeting_in_front() -> bool {
    foreground_exe().is_some_and(|name| MEETING_APPS.contains(&name.as_str()))
}

/// Nome (em minúsculas) do executável da janela em primeiro plano.
unsafe fn foreground_exe() -> Option<String> {
    let window = GetForegroundWindow();
    if window.is_null() {
        return None;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(window, &mut pid);
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if process.is_null() {
        return None;
    }
    let mut buf = [0u16; 520];
    let mut len = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
    CloseHandle(process);
    let path = ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))?;
    path.rsplit('\\').next().map(str::to_lowercase)
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
