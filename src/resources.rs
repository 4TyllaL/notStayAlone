//! Uso do PC (processador e memória) e os programas que mais pesam.
//!
//! A leitura de rotina são duas chamadas baratas (`GetSystemTimes` e
//! `GlobalMemoryStatusEx`) a cada poucos segundos. A lista de programas só é
//! montada quando você pede ajuda, e nada é fechado sem você escolher: "fechar"
//! é o mesmo que clicar no X (o programa ainda pode perguntar se quer salvar) e
//! "deixar mais leve" só baixa a prioridade. Programas do Windows, de outros
//! usuários ou sem janela nunca aparecem.

use std::{
    mem::{size_of, zeroed},
    ptr::null_mut,
    sync::atomic::{AtomicU16, Ordering},
    time::Duration,
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, BOOL, FILETIME, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM},
    Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW},
    System::{
        Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX},
        RemoteDesktop::ProcessIdToSessionId,
        SystemInformation::{GetSystemWindowsDirectoryW, GlobalMemoryStatusEx, MEMORYSTATUSEX},
        Threading::{
            GetCurrentProcessId, GetProcessTimes, GetSystemTimes, OpenProcess, QueryFullProcessImageNameW, SetPriorityClass,
            BELOW_NORMAL_PRIORITY_CLASS, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
        },
    },
    UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowLongW, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
        GWL_EXSTYLE, GW_OWNER, WM_CLOSE, WS_EX_TOOLWINDOW,
    },
};

use crate::lang::{fill, tr};

/// Quão fácil ele fica agitado.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sensitivity {
    Off,
    Low,
    Normal,
    High,
}

impl Sensitivity {
    pub const ALL: [Sensitivity; 4] = [Sensitivity::Off, Sensitivity::Low, Sensitivity::Normal, Sensitivity::High];

    pub fn key(self) -> &'static str {
        match self {
            Sensitivity::Off => "off",
            Sensitivity::Low => "low",
            Sensitivity::Normal => "normal",
            Sensitivity::High => "high",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Sensitivity::Off => tr("Nunca"),
            Sensitivity::Low => tr("Só com o PC muito pesado"),
            Sensitivity::Normal => tr("Com o PC pesado"),
            Sensitivity::High => tr("Sensível (fica agitado fácil)"),
        }
    }

    /// Limites (% do processador, % da memória) a partir dos quais o PC está "pesado".
    fn limits(self) -> (u8, u8) {
        match self {
            Sensitivity::Off => (101, 101),
            Sensitivity::Low => (95, 95),
            Sensitivity::Normal => (85, 90),
            Sensitivity::High => (70, 80),
        }
    }
}

/// Uso do PC agora, em %.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Load {
    pub cpu: u8,
    pub ram: u8,
}

/// Última leitura (processador << 8 | memória), para a página de configurações.
static LAST: AtomicU16 = AtomicU16::new(u16::MAX);

/// Última leitura do processador feita pelo app (sem leitura: `None`).
pub fn last_cpu() -> Option<u8> {
    let v = LAST.load(Ordering::Relaxed);
    (v != u16::MAX).then_some((v >> 8) as u8)
}

/// Memória em uso agora, em %.
pub fn ram_now() -> u8 {
    unsafe {
        let mut m: MEMORYSTATUSEX = zeroed();
        m.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut m) == 0 {
            return 0;
        }
        m.dwMemoryLoad.min(100) as u8
    }
}

fn ticks(t: FILETIME) -> u64 {
    (t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64
}

/// (ocioso, total) somando todos os núcleos, em unidades de 100 ns.
unsafe fn system_times() -> Option<(u64, u64)> {
    let (mut idle, mut kernel, mut user) = (zeroed(), zeroed(), zeroed());
    // O tempo de kernel já inclui o ocioso.
    (GetSystemTimes(&mut idle, &mut kernel, &mut user) != 0).then(|| (ticks(idle), ticks(kernel) + ticks(user)))
}

/// Quantas leituras seguidas acima (ou abaixo) do limite para mudar de humor.
const STREAK: u8 = 3;
/// Folga para voltar ao normal (não fica trocando de humor na beirada do limite).
const CPU_MARGIN: u8 = 15;
const RAM_MARGIN: u8 = 5;

/// Acompanha o uso do PC e decide quando ele está pesado (com folga para não oscilar).
#[derive(Default)]
pub struct Meter {
    last: Option<(u64, u64)>,
    over: u8,
    under: u8,
    pub heavy: bool,
    /// O que mais pesou na última vez que passou do limite: processador (senão, memória).
    pub by_cpu: bool,
}

impl Meter {
    /// Lê o uso agora. A primeira leitura só serve de base para o processador.
    pub unsafe fn sample(&mut self) -> Option<Load> {
        let now = system_times()?;
        let before = self.last.replace(now)?;
        let (idle, total) = (now.0.saturating_sub(before.0), now.1.saturating_sub(before.1));
        let cpu = (100 * total.saturating_sub(idle)).checked_div(total).map_or(0, |c| c.min(100) as u8);
        let load = Load { cpu, ram: ram_now() };
        LAST.store((load.cpu as u16) << 8 | load.ram as u16, Ordering::Relaxed);
        Some(load)
    }

    /// Soma uma leitura. Retorna o novo humor quando ele muda.
    pub fn feed(&mut self, load: Load, sensitivity: Sensitivity) -> Option<bool> {
        let (cpu_limit, ram_limit) = sensitivity.limits();
        let over = load.cpu >= cpu_limit || load.ram >= ram_limit;
        let calm = load.cpu.saturating_add(CPU_MARGIN) < cpu_limit && load.ram.saturating_add(RAM_MARGIN) < ram_limit;
        (self.over, self.under) = (if over { self.over + 1 } else { 0 }, if calm { self.under + 1 } else { 0 });
        if over {
            self.by_cpu = load.cpu >= cpu_limit;
        }
        let heavy = if self.heavy { self.under < STREAK } else { self.over >= STREAK };
        (heavy != self.heavy).then(|| {
            self.heavy = heavy;
            heavy
        })
    }

    /// Desligado nas configurações: esquece tudo e volta ao normal.
    pub fn reset(&mut self) {
        *self = Meter::default();
        LAST.store(u16::MAX, Ordering::Relaxed);
    }
}

// --- programas que mais pesam ------------------------------------------------------

/// Um programa seu (todos os processos do mesmo .exe juntos, como as abas do navegador).
#[derive(Clone, Debug)]
pub struct Hog {
    /// Nome para mostrar ("Google Chrome"), da descrição do .exe ou do nome do arquivo.
    pub name: String,
    /// Caminho completo do .exe: as ações conferem de novo por ele (o número do processo pode ser reutilizado).
    path: String,
    /// Memória reservada só para ele (bytes).
    pub memory: u64,
    /// Parte do processador que ele usou na medição (%).
    pub cpu: u8,
}

impl Hog {
    /// "Google Chrome — 1,4 GB de memória · 23% do processador".
    pub fn label(&self) -> String {
        fill(tr("{} — {} de memória · {}% do processador"), &[&self.name, &bytes(self.memory), &self.cpu])
    }
}

/// "812 MB", "1,4 GB" (ponto no inglês).
pub fn bytes(n: u64) -> String {
    const MB: u64 = 1024 * 1024;
    let text = if n >= 1024 * MB { format!("{:.1} GB", n as f64 / (1024 * MB) as f64) } else { format!("{} MB", n / MB) };
    if crate::lang::is_english() { text } else { text.replace('.', ",") }
}

/// Programas abaixo disso não valem a sugestão.
const MIN_MEMORY: u64 = 150 * 1024 * 1024;
const MIN_CPU: u8 = 3;
/// Quantos aparecem na lista.
pub const TOP: usize = 3;

struct Proc {
    handle: HANDLE,
    pid: u32,
    path: String,
    times: u64,
}

impl Drop for Proc {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}

/// Caminho do .exe de um processo aberto.
unsafe fn image_path(handle: HANDLE) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    (QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0)
        .then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

unsafe fn process_times(handle: HANDLE) -> u64 {
    let (mut created, mut exited, mut kernel, mut user) = (zeroed(), zeroed(), zeroed(), zeroed());
    if GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) == 0 {
        return 0;
    }
    ticks(kernel) + ticks(user)
}

unsafe fn private_memory(handle: HANDLE) -> u64 {
    let mut m: PROCESS_MEMORY_COUNTERS_EX = zeroed();
    m.cb = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    if K32GetProcessMemoryInfo(handle, (&mut m as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>(), m.cb) == 0 {
        return 0;
    }
    m.PrivateUsage as u64
}

/// Pasta do Windows ("c:\windows\"), em minúsculas: nada dali entra na lista.
unsafe fn windows_dir() -> String {
    let mut buf = [0u16; 260];
    let len = GetSystemWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as u32) as usize;
    let dir = String::from_utf16_lossy(&buf[..len.min(buf.len())]).to_lowercase();
    if dir.is_empty() { r"c:\windows\".into() } else { format!("{}\\", dir.trim_end_matches('\\')) }
}

/// Processos da sua sessão que podem entrar na lista (abertos só para consulta).
/// `access` extra é pedido junto (para baixar a prioridade).
unsafe fn processes(access: u32) -> Vec<Proc> {
    let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let me = GetCurrentProcessId();
    let mut session = 0;
    ProcessIdToSessionId(me, &mut session);
    let windows = windows_dir();
    let own = std::env::current_exe().map(|p| p.display().to_string().to_lowercase()).unwrap_or_default();
    let mut out = Vec::new();
    let mut entry: PROCESSENTRY32W = zeroed();
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut ok = Process32FirstW(snapshot, &mut entry) != 0;
    while ok {
        let pid = entry.th32ProcessID;
        let mut their = u32::MAX;
        if pid != 0 && pid != me && ProcessIdToSessionId(pid, &mut their) != 0 && their == session {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | access, 0, pid);
            if !handle.is_null() {
                let proc = image_path(handle).map(|path| Proc { handle, pid, path, times: process_times(handle) });
                match proc {
                    // O próprio app (e os filhos dele) e tudo que é do Windows ficam de fora.
                    Some(p) if !p.path.to_lowercase().starts_with(&windows) && p.path.to_lowercase() != own => out.push(p),
                    Some(_) => {} // `Drop` fecha
                    None => {
                        CloseHandle(handle);
                    }
                }
            }
        }
        ok = Process32NextW(snapshot, &mut entry) != 0;
    }
    CloseHandle(snapshot);
    out
}

/// Janela "de programa": visível, com título, sem dono e fora da barra de ferramentas.
unsafe fn app_window(hwnd: HWND) -> bool {
    IsWindowVisible(hwnd) != 0
        && GetWindow(hwnd, GW_OWNER).is_null()
        && GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW == 0
        && GetWindowTextLengthW(hwnd) > 0
}

unsafe extern "system" fn collect_window(hwnd: HWND, lp: LPARAM) -> BOOL {
    let list = &mut *(lp as *mut Vec<(HWND, u32)>);
    if app_window(hwnd) {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        list.push((hwnd, pid));
    }
    1
}

/// Janelas de programa abertas agora, com o processo de cada uma.
unsafe fn app_windows() -> Vec<(HWND, u32)> {
    let mut list: Vec<(HWND, u32)> = Vec::new();
    EnumWindows(Some(collect_window), &mut list as *mut _ as LPARAM);
    list
}

/// Descrição do .exe ("Google Chrome"); sem ela, o nome do arquivo.
unsafe fn display_name(path: &str) -> String {
    let file = path.rsplit('\\').next().unwrap_or(path);
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s).to_string();
    let wide = crate::win::w(path);
    let size = GetFileVersionInfoSizeW(wide.as_ptr(), null_mut());
    if size == 0 || size > 1 << 20 {
        return stem;
    }
    let mut data = vec![0u8; size as usize];
    if GetFileVersionInfoW(wide.as_ptr(), 0, size, data.as_mut_ptr().cast()) == 0 {
        return stem;
    }
    // Primeira tradução da tabela (idioma + página de código).
    let (mut ptr, mut len) = (null_mut(), 0u32);
    let translation = crate::win::w(r"\VarFileInfo\Translation");
    let lang = if VerQueryValueW(data.as_ptr().cast(), translation.as_ptr(), &mut ptr, &mut len) != 0 && len >= 4 {
        let pair = std::slice::from_raw_parts(ptr as *const u16, 2);
        format!("{:04x}{:04x}", pair[0], pair[1])
    } else {
        "040904b0".into()
    };
    let query = crate::win::w(&format!(r"\StringFileInfo\{lang}\FileDescription"));
    if VerQueryValueW(data.as_ptr().cast(), query.as_ptr(), &mut ptr, &mut len) == 0 || len <= 1 {
        return stem;
    }
    let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    let name = crate::win::clean_line(&String::from_utf16_lossy(&text[..end]), 40);
    if name.is_empty() { stem } else { name }
}

/// Mede por um segundo e devolve os programas seus que mais pesam, do mais pesado
/// para o mais leve (por processador se `by_cpu`, senão por memória). Bloqueia:
/// roda numa thread.
pub fn measure(by_cpu: bool) -> Vec<Hog> {
    unsafe {
        let with_window: Vec<u32> = app_windows().into_iter().map(|(_, pid)| pid).collect();
        let procs = processes(0);
        let start = system_times();
        std::thread::sleep(Duration::from_secs(1));
        let end = system_times();
        let elapsed = match (start, end) {
            (Some(a), Some(b)) => b.1.saturating_sub(a.1).max(1),
            _ => 1,
        };
        // Agrupa por .exe; só entram os que têm pelo menos uma janela aberta.
        let mut groups: Vec<(String, u64, u64, bool)> = Vec::new(); // (caminho, memória, tempo de CPU, tem janela)
        for p in &procs {
            let used = process_times(p.handle).saturating_sub(p.times);
            let key = p.path.to_lowercase();
            let windowed = with_window.contains(&p.pid);
            let memory = private_memory(p.handle);
            match groups.iter_mut().find(|g| g.0.to_lowercase() == key) {
                Some(g) => {
                    g.1 += memory;
                    g.2 += used;
                    g.3 |= windowed;
                }
                None => groups.push((p.path.clone(), memory, used, windowed)),
            }
        }
        let mut hogs: Vec<Hog> = groups
            .into_iter()
            .filter(|g| g.3)
            .map(|(path, memory, used, _)| {
                let cpu = (100 * used / elapsed).min(100) as u8;
                Hog { name: String::new(), path, memory, cpu }
            })
            .filter(|h| h.memory >= MIN_MEMORY || h.cpu >= MIN_CPU)
            .collect();
        if by_cpu {
            hogs.sort_by(|a, b| b.cpu.cmp(&a.cpu).then(b.memory.cmp(&a.memory)));
        } else {
            hogs.sort_by(|a, b| b.memory.cmp(&a.memory).then(b.cpu.cmp(&a.cpu)));
        }
        hogs.truncate(TOP);
        for h in &mut hogs {
            h.name = display_name(&h.path);
        }
        hogs
    }
}

/// Processos que ainda são deste programa (conferidos pelo caminho do .exe).
unsafe fn processes_of(hog: &Hog, access: u32) -> Vec<Proc> {
    let path = hog.path.to_lowercase();
    processes(access).into_iter().filter(|p| p.path.to_lowercase() == path).collect()
}

/// Pede para o programa fechar, como clicar no X de cada janela dele (ele pode
/// perguntar se você quer salvar). Retorna quantas janelas receberam o pedido.
pub unsafe fn close(hog: &Hog) -> usize {
    let pids: Vec<u32> = processes_of(hog, 0).iter().map(|p| p.pid).collect();
    let mut asked = 0;
    for (hwnd, pid) in app_windows() {
        if pids.contains(&pid) && PostMessageW(hwnd, WM_CLOSE, 0, 0) != 0 {
            asked += 1;
        }
    }
    asked
}

/// Baixa a prioridade de todos os processos do programa (ele continua aberto,
/// só para de disputar o processador). Retorna quantos mudaram.
pub unsafe fn lighten(hog: &Hog) -> usize {
    processes_of(hog, PROCESS_SET_INFORMATION)
        .iter()
        .filter(|p| SetPriorityClass(p.handle, BELOW_NORMAL_PRIORITY_CLASS) != 0)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(cpu: u8, ram: u8) -> Load {
        Load { cpu, ram }
    }

    #[test]
    fn gets_heavy_only_after_a_streak() {
        let mut m = Meter::default();
        assert_eq!(m.feed(load(99, 40), Sensitivity::Normal), None);
        assert_eq!(m.feed(load(99, 40), Sensitivity::Normal), None);
        assert_eq!(m.feed(load(20, 40), Sensitivity::Normal), None); // um pico só não conta
        for _ in 0..STREAK - 1 {
            assert_eq!(m.feed(load(90, 40), Sensitivity::Normal), None);
        }
        assert_eq!(m.feed(load(90, 40), Sensitivity::Normal), Some(true));
        assert!(m.heavy && m.by_cpu);
    }

    #[test]
    fn calms_down_only_with_room_to_spare() {
        let mut m = Meter::default();
        for _ in 0..STREAK {
            m.feed(load(10, 95), Sensitivity::Normal);
        }
        assert!(m.heavy && !m.by_cpu);
        // Logo abaixo do limite: continua agitado.
        for _ in 0..10 {
            assert_eq!(m.feed(load(10, 88), Sensitivity::Normal), None);
        }
        for _ in 0..STREAK - 1 {
            assert_eq!(m.feed(load(10, 70), Sensitivity::Normal), None);
        }
        assert_eq!(m.feed(load(10, 70), Sensitivity::Normal), Some(false));
    }

    #[test]
    fn off_never_gets_heavy() {
        let mut m = Meter::default();
        for _ in 0..20 {
            assert_eq!(m.feed(load(100, 100), Sensitivity::Off), None);
        }
    }

    #[test]
    fn sensitivities_are_ordered() {
        let limits: Vec<_> = Sensitivity::ALL[1..].iter().map(|s| s.limits()).collect();
        assert!(limits.windows(2).all(|w| w[0].0 > w[1].0 && w[0].1 > w[1].1));
    }

    #[test]
    fn bytes_read_nicely() {
        assert_eq!(bytes(300 * 1024 * 1024).replace(',', "."), "300 MB");
        assert_eq!(bytes(1536 * 1024 * 1024).replace(',', "."), "1.5 GB");
    }

    /// Mede de verdade: nunca lista o próprio app nem nada da pasta do Windows.
    #[test]
    fn measure_skips_windows_and_itself() {
        let windows = unsafe { windows_dir() };
        for h in measure(false) {
            assert!(!h.path.to_lowercase().starts_with(&windows), "{}", h.path);
            assert!(!h.name.is_empty());
        }
    }
}
