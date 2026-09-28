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
//! Plugins de terceiros vão além: rodam também num AppContainer (`sandbox`), sem acesso
//! aos seus arquivos nem à rede. As tarefas do próprio app (`--ia`, atualização, galeria)
//! ficam só no job, porque precisam da rede e do Gerenciador de Credenciais.

use std::{
    io::{Read, Write},
    mem::{size_of, zeroed},
    os::windows::{io::AsRawHandle, process::CommandExt},
    process::{Child, Command, Stdio},
    ptr::null,
    thread::JoinHandle,
    time::Duration,
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, HWND, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32},
        JobObjects::*,
        Threading::{
            GetExitCodeProcess, OpenThread, ResumeThread, TerminateProcess, WaitForSingleObject, CREATE_SUSPENDED,
            THREAD_SUSPEND_RESUME,
        },
    },
};

use crate::{
    lang::{fill, tr},
    mailbox,
    sandbox::{self, Sandboxed},
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
    unsafe fn start(&self, child: &Process) -> bool {
        AssignProcessToJobObject(self.0, child.handle()) != 0
            && match child {
                Process::Plain(c) => resume(c.id()),
                Process::Sandboxed(s) => ResumeThread(s.thread) != u32::MAX,
            }
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

/// O que rodar: um programa comum ou um plugin no AppContainer.
pub enum Program {
    Plain(Command),
    Sandboxed(sandbox::Spec),
}

impl From<Command> for Program {
    fn from(command: Command) -> Program {
        Program::Plain(command)
    }
}

/// Um filho criado (suspenso), de um jeito ou de outro.
enum Process {
    Plain(Child),
    Sandboxed(Sandboxed),
}

impl Process {
    fn handle(&self) -> HANDLE {
        match self {
            Process::Plain(c) => c.as_raw_handle() as HANDLE,
            Process::Sandboxed(s) => s.process,
        }
    }

    /// Espera terminar e devolve o código de saída; passado o `timeout`, encerra (`None`).
    fn wait(&self, timeout: Duration) -> Option<u32> {
        unsafe {
            if WaitForSingleObject(self.handle(), timeout.as_millis() as u32) != 0 {
                self.kill();
                return None;
            }
            let mut code = 0;
            (GetExitCodeProcess(self.handle(), &mut code) != 0).then_some(code)
        }
    }

    fn kill(&self) {
        unsafe {
            TerminateProcess(self.handle(), 1);
            WaitForSingleObject(self.handle(), 5000);
        }
    }
}

type Pipes = (Option<Box<dyn Write>>, Option<Box<dyn Read + Send>>, Option<Box<dyn Read + Send>>);

/// Cria o filho suspenso e devolve as pontas dos pipes do lado do app.
fn create(program: Program) -> Result<(Process, Pipes), String> {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    match program {
        Program::Plain(mut command) => {
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)
                .spawn()
                .map_err(|e| fill(tr("não consegui abrir o programa ({})"), &[&e]))?;
            let pipes: Pipes = (
                child.stdin.take().map(|p| Box::new(p) as _),
                child.stdout.take().map(|p| Box::new(p) as _),
                child.stderr.take().map(|p| Box::new(p) as _),
            );
            Ok((Process::Plain(child), pipes))
        }
        Program::Sandboxed(spec) => {
            let mut s = sandbox::spawn(&spec).map_err(|e| format!("{} ({e})", tr("não consegui preparar o isolamento do programa")))?;
            let pipes: Pipes = (
                s.stdin.take().map(|p| Box::new(p) as _),
                s.stdout.take().map(|p| Box::new(p) as _),
                s.stderr.take().map(|p| Box::new(p) as _),
            );
            Ok((Process::Sandboxed(s), pipes))
        }
    }
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

/// Roda `program`, manda `input` no stdin e devolve o stdout (até `max_output` bytes).
pub fn run(program: impl Into<Program>, input: &str, max_output: u64) -> Reply {
    let job = unsafe { Confined::new() }.ok_or(tr("não consegui preparar o isolamento do programa"))?;
    let (child, (stdin, stdout, stderr)) = create(program.into())?;
    // Sem o job, não roda: encerra ainda suspenso.
    if !unsafe { job.start(&child) } {
        child.kill();
        return Err(tr("não consegui preparar o isolamento do programa").into());
    }
    // Lê as saídas em paralelo e com limite: o filho nunca trava escrevendo
    // e não consegue encher a memória do app.
    let stdout = drain(stdout, max_output);
    let stderr = drain(stderr, max_output);
    if let Some(mut stdin) = stdin {
        let _ = stdin.write_all(input.as_bytes()); // se falhar, o filho reclama no stderr
    } // fechar o stdin avisa o filho de que a entrada acabou
    let status = child.wait(TIMEOUT);
    let (out, err) = (stdout.join().unwrap_or_default(), stderr.join().unwrap_or_default());
    match status {
        None => Err(tr("demorou demais e foi encerrado.").into()),
        Some(0) => Ok(String::from_utf8_lossy(&out).trim().trim_start_matches('\u{feff}').to_string()),
        Some(_) => {
            let err = String::from_utf8_lossy(&err).trim().to_string();
            Err(if err.is_empty() { tr("o programa falhou").into() } else { err })
        }
    }
}

fn drain(pipe: Option<Box<dyn Read + Send>>, max: u64) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(pipe) = pipe {
            let _ = pipe.take(max).read_to_end(&mut bytes);
        }
        bytes
    })
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
