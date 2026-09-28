//! Processos filhos: plugins e as tarefas de rede do próprio app (`--ia`,
//! atualização, galeria). Rodam numa thread, sem janela, com tempo e saída
//! limitados, e a resposta volta para uma janela pelo `mailbox`.
//!
//! Cada filho nasce suspenso, entra num Job Object (`Confined`) e só então roda. O job:
//! - não deixa abrir outros programas (um processo só);
//! - não deixa ler/escrever a área de transferência, mexer em configurações do sistema,
//!   na tela, na área de trabalho, desligar o PC nem usar janelas de fora do job (então
//!   não dá para forjar mensagens para o mascote);
//! - limita a memória (`MAX_MEMORY`) e encerra tudo quando o app fecha.
//!
//! Não é uma sandbox: arquivos e rede continuam com as permissões da sua conta.

use std::{
    io::{Read, Write},
    mem::{size_of, zeroed},
    os::windows::{io::AsRawHandle, process::CommandExt},
    process::{Child, Command, ExitStatus, Stdio},
    ptr::null,
    thread::JoinHandle,
    time::{Duration, Instant},
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32},
        JobObjects::*,
        Threading::{OpenThread, ResumeThread, CREATE_SUSPENDED, THREAD_SUSPEND_RESUME},
    },
};

use crate::{
    lang::{fill, tr},
    mailbox,
};

/// Resposta de um processo filho: o texto do stdout, ou a mensagem de erro.
pub type Reply = Result<String, String>;

/// Tempo máximo de um processo filho.
pub const TIMEOUT: Duration = Duration::from_secs(120);
/// Memória máxima de um processo filho.
const MAX_MEMORY: usize = 512 * 1024 * 1024;

/// Job Object com as regras acima. Fechar o job encerra o que ainda estiver nele.
struct Confined(HANDLE);

impl Confined {
    unsafe fn new() -> Option<Confined> {
        let job = CreateJobObjectW(null(), null());
        if job.is_null() {
            return None;
        }
        let job = Confined(job);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
        limits.ProcessMemoryLimit = MAX_MEMORY;
        let ui = JOBOBJECT_BASIC_UI_RESTRICTIONS {
            UIRestrictionsClass: JOB_OBJECT_UILIMIT_HANDLES
                | JOB_OBJECT_UILIMIT_READCLIPBOARD
                | JOB_OBJECT_UILIMIT_WRITECLIPBOARD
                | JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS
                | JOB_OBJECT_UILIMIT_DISPLAYSETTINGS
                | JOB_OBJECT_UILIMIT_GLOBALATOMS
                | JOB_OBJECT_UILIMIT_DESKTOP
                | JOB_OBJECT_UILIMIT_EXITWINDOWS,
        };
        let set = |class, info: *const core::ffi::c_void, size: usize| SetInformationJobObject(job.0, class, info, size as u32) != 0;
        let ok = set(JobObjectExtendedLimitInformation, &limits as *const _ as _, size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            && set(JobObjectBasicUIRestrictions, &ui as *const _ as _, size_of::<JOBOBJECT_BASIC_UI_RESTRICTIONS>());
        ok.then_some(job)
    }

    /// Põe o filho (criado suspenso) no job e o deixa rodar.
    unsafe fn start(&self, child: &Child) -> bool {
        AssignProcessToJobObject(self.0, child.as_raw_handle() as HANDLE) != 0 && resume(child.id())
    }
}

impl Drop for Confined {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// Solta as threads de um processo criado com CREATE_SUSPENDED (ele só tem a principal).
unsafe fn resume(pid: u32) -> bool {
    let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
    if snapshot == INVALID_HANDLE_VALUE {
        return false;
    }
    let mut entry: THREADENTRY32 = zeroed();
    entry.dwSize = size_of::<THREADENTRY32>() as u32;
    let mut resumed = false;
    let mut more = Thread32First(snapshot, &mut entry) != 0;
    while more {
        if entry.th32OwnerProcessID == pid {
            let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
            if !thread.is_null() {
                resumed |= ResumeThread(thread) != u32::MAX;
                CloseHandle(thread);
            }
        }
        more = Thread32Next(snapshot, &mut entry) != 0;
    }
    CloseHandle(snapshot);
    resumed
}

/// O próprio .exe num modo interno (ex.: `--ia`).
pub fn this_app(args: &[&str]) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap_or_default());
    command.args(args);
    command
}

/// Roda `job` numa thread e entrega `wrap(resposta)` à janela `to` com `msg`.
pub fn spawn<T: Send + 'static>(
    to: HWND,
    msg: u32,
    job: impl FnOnce() -> Reply + Send + 'static,
    wrap: impl FnOnce(Reply) -> T + Send + 'static,
) {
    let target = to as isize;
    std::thread::spawn(move || {
        let reply = job();
        mailbox::post(target as HWND, msg, wrap(reply));
    });
}

/// Roda `command`, manda `input` no stdin e devolve o stdout (até `max_output` bytes).
pub fn run(mut command: Command, input: &str, max_output: u64) -> Reply {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let job = unsafe { Confined::new() }.ok_or(tr("não consegui preparar o isolamento do programa"))?;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)
        .spawn()
        .map_err(|e| fill(tr("não consegui abrir o programa ({})"), &[&e]))?;
    // Sem o job, não roda: encerra ainda suspenso.
    if !unsafe { job.start(&child) } {
        let _ = child.kill();
        let _ = child.wait();
        return Err(tr("não consegui preparar o isolamento do programa").into());
    }
    // Lê as saídas em paralelo e com limite: o filho nunca trava escrevendo
    // e não consegue encher a memória do app.
    let stdout = drain(child.stdout.take(), max_output);
    let stderr = drain(child.stderr.take(), max_output);
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes()); // se falhar, o filho reclama no stderr
    }
    let status = wait(&mut child, TIMEOUT);
    let (out, err) = (stdout.join().unwrap_or_default(), stderr.join().unwrap_or_default());
    match status {
        None => Err(tr("demorou demais e foi encerrado.").into()),
        Some(s) if s.success() => Ok(String::from_utf8_lossy(&out).trim().trim_start_matches('\u{feff}').to_string()),
        Some(_) => {
            let err = String::from_utf8_lossy(&err).trim().to_string();
            Err(if err.is_empty() { tr("o programa falhou").into() } else { err })
        }
    }
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>, max: u64) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(pipe) = pipe {
            let _ = pipe.take(max).read_to_end(&mut bytes);
        }
        bytes
    })
}

/// Espera o filho terminar; passado o `timeout`, encerra o processo (`None`).
fn wait(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn powershell(script: &str) -> Command {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let mut c = Command::new(format!(r"{root}\System32\WindowsPowerShell\v1.0\powershell.exe"));
        c.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script]);
        c
    }

    #[test]
    fn children_run_confined() {
        // Roda normalmente e recebe o stdin (foi solto antes de ler).
        assert_eq!(run(powershell("$input | ForEach-Object { \"oi $_\" }"), "mascote", 4096), Ok("oi mascote".into()));
        // Não consegue abrir outro programa.
        let spawn = "try { Start-Process cmd.exe -ArgumentList '/c','exit' -Wait -ErrorAction Stop; 'abriu' } catch { 'bloqueado' }";
        assert_eq!(run(powershell(spawn), "", 4096), Ok("bloqueado".into()));
        // Não lê a área de transferência (só diz se conseguiu; nunca mostra o conteúdo).
        let clip = "try { if (Get-Clipboard -Raw -ErrorAction Stop) { 'leu' } else { 'nada' } } catch { 'bloqueado' }";
        assert_ne!(run(powershell(clip), "", 4096), Ok("leu".into()));
    }
}
