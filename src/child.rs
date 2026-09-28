//! Processos filhos: plugins e as tarefas de rede do próprio app (`--ia`,
//! atualização, galeria). Rodam numa thread, sem janela, com tempo e saída
//! limitados, e a resposta volta para uma janela pelo `mailbox`.

use std::{
    io::{Read, Write},
    os::windows::process::CommandExt,
    process::{Child, Command, ExitStatus, Stdio},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use windows_sys::Win32::Foundation::HWND;

use crate::{
    lang::{fill, tr},
    mailbox,
};

/// Resposta de um processo filho: o texto do stdout, ou a mensagem de erro.
pub type Reply = Result<String, String>;

/// Tempo máximo de um processo filho.
pub const TIMEOUT: Duration = Duration::from_secs(120);

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
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| fill(tr("não consegui abrir o programa ({})"), &[&e]))?;
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
