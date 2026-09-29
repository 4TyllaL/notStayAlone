//! Sandbox dos plugins de terceiros: cada um roda num **AppContainer** próprio
//! (`StayAlone.Plugin.<id>`), o mesmo isolamento dos apps da Microsoft Store:
//! - arquivos: só **lê** a pasta do plugin e lê/grava a pasta de dados dele
//!   (`STAYALONE_DADOS`); documentos, área de trabalho, %APPDATA% e o resto da sua conta
//!   ficam fora de alcance, menos as pastas que você aprovar (`allow_folder`), cada uma
//!   só para leitura ou para leitura e escrita;
//! - rede: nenhuma, a não ser que o `plugin.ini` peça (`internet = sim`) e você aprove;
//! - ambiente: variáveis mínimas (nada de chaves de API em variáveis de ambiente);
//! - herda só os três pipes (stdin, stdout, stderr), nenhum outro handle do app.
//!
//! O processo nasce suspenso: `child` o põe no Job Object antes de soltá-lo.

use std::{
    ffi::OsString,
    fs::File,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::FromRawHandle,
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};

use windows_sys::{
    core::PWSTR,
    Win32::{
        Foundation::*,
        Security::{
            Authorization::*, FreeSid, Isolation::*, ACL, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
            SECURITY_CAPABILITIES, SID_AND_ATTRIBUTES, SUB_CONTAINERS_AND_OBJECTS_INHERIT, DACL_SECURITY_INFORMATION,
        },
        System::{Com::CoTaskMemFree, Pipes::CreatePipe, Threading::*},
    },
};

/// Capacidade `internetClient` (conexões de saída).
const INTERNET_CLIENT: &str = "S-1-15-3-1";
const SE_GROUP_ENABLED: u32 = 0x4;
const FILE_READ_EXECUTE: u32 = 0x0012_00A9; // FILE_GENERIC_READ | FILE_GENERIC_EXECUTE
/// Ler, gravar, criar e apagar (o "Modificar" das propriedades da pasta), sem mudar permissões.
const FILE_MODIFY: u32 = 0x0013_01BF;
const ERROR_ALREADY_EXISTS_HR: i32 = 0x8007_00B7_u32 as i32;

/// O que rodar e com que permissões.
#[derive(Clone, Debug)]
pub struct Spec {
    /// Id do plugin (vira o nome do AppContainer).
    pub id: String,
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// Pasta do plugin: diretório atual, só leitura.
    pub folder: PathBuf,
    pub internet: bool,
}

/// Processo criado (suspenso) e as pontas dos pipes do lado do app.
pub struct Sandboxed {
    pub process: HANDLE,
    pub thread: HANDLE,
    pub stdin: Option<File>,
    pub stdout: Option<File>,
    pub stderr: Option<File>,
}

impl Drop for Sandboxed {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.process);
            CloseHandle(self.thread);
        }
    }
}

/// SID alocado pelo Windows, liberado do jeito certo.
struct Sid(PSID, fn(PSID));

impl Drop for Sid {
    fn drop(&mut self) {
        (self.1)(self.0)
    }
}

fn free_sid(sid: PSID) {
    unsafe { FreeSid(sid) };
}

fn local_free(sid: PSID) {
    unsafe { LocalFree(sid as HLOCAL) };
}

fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}

/// Criar o perfil e mexer nas permissões é ler-mudar-gravar: dois ao mesmo tempo (o botão
/// Testar enquanto o mesmo plugin roda sozinho) poderiam atropelar um ao outro.
static SETUP: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn setup_lock() -> std::sync::MutexGuard<'static, ()> {
    SETUP.lock().unwrap_or_else(|e| e.into_inner())
}

/// Nome do AppContainer do plugin (até 64 caracteres).
pub fn container_name(id: &str) -> String {
    let mut name = format!("StayAlone.Plugin.{id}");
    name.truncate(64);
    name
}

/// Cria o perfil do AppContainer (ou pega o que já existe) e devolve o SID dele.
unsafe fn container(id: &str) -> Result<Sid, String> {
    let name = wide(container_name(id));
    let display = wide(format!("!StayAlone: plugin {id}"));
    let mut sid: PSID = null_mut();
    let hr = CreateAppContainerProfile(name.as_ptr(), display.as_ptr(), display.as_ptr(), null(), 0, &mut sid);
    if hr == ERROR_ALREADY_EXISTS_HR {
        let hr = DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid);
        if hr < 0 {
            return Err(format!("AppContainer (0x{hr:08X})"));
        }
    } else if hr < 0 {
        return Err(format!("AppContainer (0x{hr:08X})"));
    }
    Ok(Sid(sid, free_sid))
}

/// A pasta de dados do AppContainer (`...\Packages\<nome>\AC`).
unsafe fn container_folder(sid: PSID) -> Result<PathBuf, String> {
    let mut text: PWSTR = null_mut();
    if ConvertSidToStringSidW(sid, &mut text) == 0 {
        return Err("SID".into());
    }
    let mut path: PWSTR = null_mut();
    let hr = GetAppContainerFolderPath(text, &mut path);
    LocalFree(text as HLOCAL);
    if hr < 0 {
        return Err(format!("pasta do AppContainer (0x{hr:08X})"));
    }
    let len = (0..).take_while(|&i| *path.add(i) != 0).count();
    let folder = PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(path, len)));
    CoTaskMemFree(path as _);
    Ok(folder)
}

/// Muda a permissão do AppContainer numa pasta (herdada por tudo dentro dela):
/// `GRANT_ACCESS` soma, `SET_ACCESS` troca a que ele tinha, `REVOKE_ACCESS` tira.
unsafe fn set_access(folder: &Path, sid: PSID, mask: u32, mode: ACCESS_MODE) -> Result<(), String> {
    let path = wide(folder);
    let (mut dacl, mut descriptor): (*mut ACL, PSECURITY_DESCRIPTOR) = (null_mut(), null_mut());
    let err = GetNamedSecurityInfoW(
        path.as_ptr(),
        SE_FILE_OBJECT,
        DACL_SECURITY_INFORMATION,
        null_mut(),
        null_mut(),
        &mut dacl,
        null_mut(),
        &mut descriptor,
    );
    if err != 0 {
        return Err(format!("permissão da pasta ({err})"));
    }
    let mut access: EXPLICIT_ACCESS_W = zeroed();
    access.grfAccessPermissions = mask;
    access.grfAccessMode = mode;
    access.grfInheritance = SUB_CONTAINERS_AND_OBJECTS_INHERIT;
    access.Trustee.TrusteeForm = TRUSTEE_IS_SID;
    access.Trustee.TrusteeType = TRUSTEE_IS_WELL_KNOWN_GROUP;
    access.Trustee.ptstrName = sid as PWSTR;
    let mut updated: *mut ACL = null_mut();
    let mut err = SetEntriesInAclW(1, &access, dacl, &mut updated);
    if err == 0 {
        err = SetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            updated,
            null(),
        );
        LocalFree(updated as HLOCAL);
    }
    LocalFree(descriptor as HLOCAL);
    if err != 0 {
        return Err(format!("permissão da pasta ({err})"));
    }
    Ok(())
}

/// Dá ao plugin `id` acesso a uma pasta que você aprovou (leitura, ou leitura e escrita).
/// Numa pasta grande pode demorar: o Windows aplica a permissão a cada arquivo.
pub fn allow_folder(id: &str, folder: &Path, write: bool) -> Result<(), String> {
    let _lock = setup_lock();
    unsafe {
        let sid = container(id)?;
        set_access(folder, sid.0, if write { FILE_MODIFY } else { FILE_READ_EXECUTE }, SET_ACCESS)
    }
}

/// Tira do plugin `id` o acesso a uma pasta (plugin desligado ou pasta não mais aprovada).
pub fn revoke_folder(id: &str, folder: &Path) -> Result<(), String> {
    let _lock = setup_lock();
    unsafe {
        let sid = container(id)?;
        set_access(folder, sid.0, 0, REVOKE_ACCESS)
    }
}

/// Variáveis de ambiente mínimas: as do Windows, e as pastas de usuário apontando para
/// a pasta de dados do plugin.
fn environment(data: &Path) -> Vec<u16> {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let mut vars: Vec<(String, OsString)> = [
        "SystemRoot",
        "windir",
        "SystemDrive",
        "ComSpec",
        "PATHEXT",
        "OS",
        "PROCESSOR_ARCHITECTURE",
        "NUMBER_OF_PROCESSORS",
    ]
    .iter()
    .filter_map(|k| std::env::var_os(k).map(|v| (k.to_string(), v)))
    .collect();
    vars.push(("PATH".into(), format!(r"{root}\System32;{root};{root}\System32\WindowsPowerShell\v1.0").into()));
    vars.push(("PSModulePath".into(), format!(r"{root}\System32\WindowsPowerShell\v1.0\Modules").into()));
    let temp = data.join("Temp");
    for (key, value) in [
        ("STAYALONE_DADOS", data.join("Dados")),
        ("TEMP", temp.clone()),
        ("TMP", temp),
        ("USERPROFILE", data.to_path_buf()),
        ("APPDATA", data.join("Roaming")),
        ("LOCALAPPDATA", data.join("Local")),
    ] {
        let _ = std::fs::create_dir_all(&value);
        vars.push((key.into(), value.into()));
    }
    vars.sort_by_key(|(k, _)| k.to_uppercase());
    let mut block = Vec::new();
    for (key, value) in vars {
        block.extend(key.encode_utf16());
        block.push(b'=' as u16);
        block.extend(value.encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

/// Aspas no estilo do Windows (CommandLineToArgvW) para um argumento.
fn quote(arg: &std::ffi::OsStr, out: &mut Vec<u16>) {
    let arg: Vec<u16> = arg.encode_wide().collect();
    let plain = !arg.is_empty() && !arg.iter().any(|&c| c == b' ' as u16 || c == b'\t' as u16 || c == b'"' as u16);
    if plain {
        out.extend(arg);
        return;
    }
    out.push(b'"' as u16);
    let mut backslashes = 0;
    for c in arg {
        if c == b'\\' as u16 {
            backslashes += 1;
            continue;
        }
        let n = if c == b'"' as u16 { backslashes * 2 + 1 } else { backslashes };
        out.extend(std::iter::repeat_n(b'\\' as u16, n));
        out.push(c);
        backslashes = 0;
    }
    out.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
    out.push(b'"' as u16);
}

/// Pipe com a ponta do filho herdável e a do app não.
unsafe fn pipe(child_reads: bool) -> Result<(HANDLE, HANDLE), String> {
    let sa = SECURITY_ATTRIBUTES { nLength: size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: null_mut(), bInheritHandle: 1 };
    let (mut read, mut write) = (null_mut(), null_mut());
    if CreatePipe(&mut read, &mut write, &sa, 0) == 0 {
        return Err("pipe".into());
    }
    let (child, ours) = if child_reads { (read, write) } else { (write, read) };
    SetHandleInformation(ours, HANDLE_FLAG_INHERIT, 0);
    Ok((child, ours))
}

/// Cria o processo do plugin no AppContainer, **suspenso**.
pub fn spawn(spec: &Spec) -> Result<Sandboxed, String> {
    unsafe {
        let setup = setup_lock();
        let sid = container(&spec.id)?;
        set_access(&spec.folder, sid.0, FILE_READ_EXECUTE, GRANT_ACCESS)?;
        let data = container_folder(sid.0)?;
        drop(setup);

        let internet = if spec.internet {
            let text = wide(INTERNET_CLIENT);
            let mut cap: PSID = null_mut();
            if ConvertStringSidToSidW(text.as_ptr(), &mut cap) == 0 {
                return Err("SID".into());
            }
            Some(Sid(cap, local_free))
        } else {
            None
        };
        let mut caps: Vec<SID_AND_ATTRIBUTES> =
            internet.iter().map(|c| SID_AND_ATTRIBUTES { Sid: c.0, Attributes: SE_GROUP_ENABLED }).collect();
        let security = SECURITY_CAPABILITIES {
            AppContainerSid: sid.0,
            Capabilities: if caps.is_empty() { null_mut() } else { caps.as_mut_ptr() },
            CapabilityCount: caps.len() as u32,
            Reserved: 0,
        };

        let (in_child, in_ours) = pipe(true)?;
        let (out_child, out_ours) = pipe(false)?;
        let (err_child, err_ours) = pipe(false)?;
        let inherited = [in_child, out_child, err_child];
        // As pontas do app viram File já aqui: fecham sozinhas se algo falhar.
        let (stdin, stdout, stderr) = (File::from_raw_handle(in_ours), File::from_raw_handle(out_ours), File::from_raw_handle(err_ours));
        let close_child_ends = || inherited.iter().for_each(|&h| {
            CloseHandle(h);
        });

        let mut size = 0;
        InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut size);
        let mut buffer = vec![0u64; size.div_ceil(8)];
        let list = buffer.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
        let ok = InitializeProcThreadAttributeList(list, 2, 0, &mut size) != 0
            && UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                &security as *const _ as _,
                size_of::<SECURITY_CAPABILITIES>(),
                null_mut(),
                null(),
            ) != 0
            && UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                inherited.as_ptr() as _,
                size_of::<[HANDLE; 3]>(),
                null_mut(),
                null(),
            ) != 0;
        if !ok {
            close_child_ends();
            return Err("atributos do processo".into());
        }

        let mut info: STARTUPINFOEXW = zeroed();
        info.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        info.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        (info.StartupInfo.hStdInput, info.StartupInfo.hStdOutput, info.StartupInfo.hStdError) = (in_child, out_child, err_child);
        info.lpAttributeList = list;

        let mut command_line = Vec::new();
        quote(spec.program.as_os_str(), &mut command_line);
        for arg in &spec.args {
            command_line.push(b' ' as u16);
            quote(arg, &mut command_line);
        }
        command_line.push(0);
        let program = wide(&spec.program);
        let folder = wide(&spec.folder);
        let env = environment(&data);
        let mut process: PROCESS_INFORMATION = zeroed();
        let created = CreateProcessW(
            program.as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            1,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
            env.as_ptr() as _,
            folder.as_ptr(),
            &info.StartupInfo,
            &mut process,
        );
        let error = GetLastError();
        DeleteProcThreadAttributeList(list);
        close_child_ends();
        if created == 0 {
            return Err(format!("CreateProcess ({error})"));
        }
        Ok(Sandboxed { process: process.hProcess, thread: process.hThread, stdin: Some(stdin), stdout: Some(stdout), stderr: Some(stderr) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::child::{self, Program};

    /// Um "plugin" PowerShell no sandbox, rodando da pasta `folder`.
    fn powershell(id: &str, folder: &Path, script: &str, internet: bool) -> Program {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let args = ["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script];
        Program::Sandboxed(Spec {
            id: id.into(),
            program: PathBuf::from(format!(r"{root}\System32\WindowsPowerShell\v1.0\powershell.exe")),
            args: args.iter().map(OsString::from).collect(),
            folder: folder.to_path_buf(),
            internet,
        })
    }

    #[test]
    fn plugins_only_reach_their_own_folders() {
        let folder = std::env::temp_dir().join("stayalone-sandbox-limites");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("dados.txt"), "da pasta").unwrap();
        // Um arquivo "seu", fora da pasta do plugin.
        let secret = std::env::temp_dir().join("stayalone-sandbox-segredo.txt");
        std::fs::write(&secret, "segredo").unwrap();
        let script = format!(
            "$r = @(); $r += \"oi $input\"; \
             $r += Get-Content '{}'; \
             try {{ Get-Content -ErrorAction Stop '{}'; $r += 'leu-segredo' }} catch {{ $r += 'sem-segredo' }}; \
             try {{ Get-ChildItem -ErrorAction Stop $([Environment]::GetFolderPath('Desktop').Replace('\\AC','')) | Out-Null; $r += 'viu-desktop' }} catch {{ $r += 'sem-desktop' }}; \
             Set-Content \"$env:STAYALONE_DADOS\\x.txt\" 'ok'; $r += Get-Content \"$env:STAYALONE_DADOS\\x.txt\"; \
             try {{ $c = New-Object Net.Sockets.TcpClient; if ($c.ConnectAsync('1.1.1.1', 443).Wait(3000) -and $c.Connected) {{ $r += 'rede' }} else {{ $r += 'sem-rede' }} }} catch {{ $r += 'sem-rede' }}; \
             $r -join ','",
            folder.join("dados.txt").display(),
            secret.display()
        );
        let reply = child::run(powershell("teste-limites", &folder, &script, false), "mascote", 4096);
        let _ = std::fs::remove_file(&secret);
        assert_eq!(reply, Ok("oi mascote,da pasta,sem-segredo,sem-desktop,ok,sem-rede".into()));
    }

    /// Precisa de internet: `cargo test --release -- --ignored internet_permission`.
    #[test]
    #[ignore]
    fn internet_permission_opens_the_network() {
        let folder = std::env::temp_dir().join("stayalone-sandbox-rede");
        std::fs::create_dir_all(&folder).unwrap();
        let script = "$c = New-Object Net.Sockets.TcpClient; if ($c.ConnectAsync('1.1.1.1', 443).Wait(5000) -and $c.Connected) { 'rede' } else { 'sem-rede' }";
        assert_eq!(child::run(powershell("teste-rede", &folder, script, true), "", 4096), Ok("rede".into()));
    }

    #[test]
    fn approved_folders_open_and_close() {
        let plugin = std::env::temp_dir().join("stayalone-sandbox-pastas");
        let notes = std::env::temp_dir().join("stayalone-sandbox-notas");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::create_dir_all(&notes).unwrap();
        std::fs::write(notes.join("nota.txt"), "comprar pao").unwrap();
        let script = format!(
            "try {{ Get-Content -ErrorAction Stop '{0}\\nota.txt' }} catch {{ 'sem-leitura' }}; \
             try {{ Set-Content -ErrorAction Stop '{0}\\nova.txt' 'x'; 'gravou' }} catch {{ 'sem-escrita' }}",
            notes.display()
        );
        let run = || child::run(powershell("teste-pastas-ab", &plugin, &script, false), "", 4096).map(|s| s.lines().collect::<Vec<_>>().join("|"));
        let _ = std::fs::remove_file(notes.join("nova.txt"));
        assert_eq!(run(), Ok("sem-leitura|sem-escrita".into()));
        allow_folder("teste-pastas-ab", &notes, false).unwrap();
        assert_eq!(run(), Ok("comprar pao|sem-escrita".into()));
        allow_folder("teste-pastas-ab", &notes, true).unwrap();
        assert_eq!(run(), Ok("comprar pao|gravou".into()));
        revoke_folder("teste-pastas-ab", &notes).unwrap();
        assert_eq!(run(), Ok("sem-leitura|sem-escrita".into()));
        let _ = std::fs::remove_dir_all(&notes);
    }

    #[test]
    fn arguments_are_quoted_like_windows_expects() {
        let q = |s: &str| {
            let mut out = Vec::new();
            quote(std::ffi::OsStr::new(s), &mut out);
            String::from_utf16(&out).unwrap()
        };
        assert_eq!(q("simples"), "simples");
        assert_eq!(q("com espaço"), "\"com espaço\"");
        assert_eq!(q(r"C:\pasta com espaço\"), r#""C:\pasta com espaço\\""#);
        assert_eq!(q(r#"diz "oi""#), r#""diz \"oi\"""#);
        assert_eq!(q(""), "\"\"");
    }
}
