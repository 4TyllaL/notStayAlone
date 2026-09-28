//! Um POST de JSON via WinHTTP (já vem no Windows: sem biblioteca de TLS no binário).
//!
//! A winhttp.dll não está entre as "DLLs conhecidas" do Windows: importada do
//! jeito normal, o Windows a procuraria primeiro na pasta do .exe, e uma DLL
//! falsa deixada ali (por exemplo, na pasta Downloads) seria carregada no lugar.
//! Por isso ela é carregada à mão, e só de System32.

use std::{
    ffi::c_void,
    mem::{size_of, transmute_copy},
    ptr::{null, null_mut},
};

use windows_sys::{
    core::{PCWSTR, PCSTR},
    Win32::{
        Foundation::{GetLastError, BOOL},
        Networking::WinHttp::{
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2,
            WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3, WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
            WINHTTP_OPTION_SECURE_PROTOCOLS, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
        },
        System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32},
    },
};

use crate::win::w as wide;

/// Respostas maiores que isso são recusadas: a de uma conversa tem 1–3 KB (o
/// balão mostra 350 caracteres) e a de um desenho da IA, uns 4 KB.
const MAX_RESPONSE: usize = 64 * 1024;

type Handle = *mut c_void;

/// As funções da winhttp.dll que o plugin usa.
struct WinHttp {
    open: unsafe extern "system" fn(PCWSTR, u32, PCWSTR, PCWSTR, u32) -> Handle,
    set_timeouts: unsafe extern "system" fn(Handle, i32, i32, i32, i32) -> BOOL,
    set_option: unsafe extern "system" fn(Handle, u32, *const c_void, u32) -> BOOL,
    connect: unsafe extern "system" fn(Handle, PCWSTR, u16, u32) -> Handle,
    open_request: unsafe extern "system" fn(Handle, PCWSTR, PCWSTR, PCWSTR, PCWSTR, *const PCWSTR, u32) -> Handle,
    send_request: unsafe extern "system" fn(Handle, PCWSTR, u32, *const c_void, u32, u32, usize) -> BOOL,
    receive_response: unsafe extern "system" fn(Handle, *mut c_void) -> BOOL,
    query_headers: unsafe extern "system" fn(Handle, u32, PCWSTR, *mut c_void, *mut u32, *mut u32) -> BOOL,
    query_data_available: unsafe extern "system" fn(Handle, *mut u32) -> BOOL,
    read_data: unsafe extern "system" fn(Handle, *mut c_void, u32, *mut u32) -> BOOL,
    close: unsafe extern "system" fn(Handle) -> BOOL,
}

impl WinHttp {
    unsafe fn load() -> Result<WinHttp, String> {
        let lib = LoadLibraryExW(wide("winhttp.dll").as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        if lib.is_null() {
            return Err("não encontrei o WinHTTP do Windows.".into());
        }
        let find = |name: &str| -> Result<unsafe extern "system" fn() -> isize, String> {
            let name = format!("{name}\0");
            GetProcAddress(lib, name.as_ptr() as PCSTR).ok_or_else(|| format!("WinHTTP sem {name}"))
        };
        // Cada ponteiro é convertido para a assinatura documentada da função.
        Ok(WinHttp {
            open: cast(find("WinHttpOpen")?),
            set_timeouts: cast(find("WinHttpSetTimeouts")?),
            set_option: cast(find("WinHttpSetOption")?),
            connect: cast(find("WinHttpConnect")?),
            open_request: cast(find("WinHttpOpenRequest")?),
            send_request: cast(find("WinHttpSendRequest")?),
            receive_response: cast(find("WinHttpReceiveResponse")?),
            query_headers: cast(find("WinHttpQueryHeaders")?),
            query_data_available: cast(find("WinHttpQueryDataAvailable")?),
            read_data: cast(find("WinHttpReadData")?),
            close: cast(find("WinHttpCloseHandle")?),
        })
    }
}

type RawProc = unsafe extern "system" fn() -> isize;

/// Converte o endereço vindo do `GetProcAddress` para a assinatura documentada da função.
unsafe fn cast<F: Copy>(proc: RawProc) -> F {
    assert_eq!(size_of::<F>(), size_of::<RawProc>());
    transmute_copy(&proc)
}

/// Um handle do WinHTTP que se fecha sozinho.
struct Owned<'a>(Handle, &'a WinHttp);

impl Owned<'_> {
    fn check(self) -> Result<Self, String> {
        if self.0.is_null() {
            Err(net_error())
        } else {
            Ok(self)
        }
    }
}

impl Drop for Owned<'_> {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { (self.1.close)(self.0) };
        }
    }
}

/// Partes de uma URL `http(s)://host[:porta]/caminho`.
#[derive(Debug, PartialEq)]
pub struct Url<'a> {
    pub secure: bool,
    pub host: &'a str,
    pub port: u16,
    pub path: &'a str,
}

pub fn parse_url(url: &str) -> Result<Url<'_>, String> {
    let (secure, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err("api_base precisa começar com https://".into());
    };
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(':') => (h, p.parse::<u16>().map_err(|_| "porta inválida em api_base".to_string())?),
        _ => (authority, if secure { 443 } else { 80 }),
    };
    if host.is_empty() || host.contains('@') {
        return Err("endereço inválido em api_base".into());
    }
    // Sem criptografia, só dentro do próprio PC: a chave nunca atravessa a rede aberta.
    if !secure && !matches!(host.to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "[::1]") {
        return Err("por segurança, http:// só é aceito para serviços no seu PC (use https://).".into());
    }
    Ok(Url { secure, host: host.trim_start_matches('[').trim_end_matches(']'), port, path: if path.is_empty() { "/" } else { path } })
}

/// Envia `body` e devolve (status HTTP, corpo da resposta).
pub fn post(url: &str, key: Option<&str>, body: &str) -> Result<(u32, String), String> {
    let url = parse_url(url)?;
    unsafe {
        let api = WinHttp::load()?;
        let session = Owned((api.open)(wide(concat!("StayAlone-chat/", env!("CARGO_PKG_VERSION"))).as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, null(), null(), 0), &api)
            .check()?;
        (api.set_timeouts)(session.0, 10_000, 10_000, 30_000, 60_000);
        // A chave vai num cabeçalho: redirecionamentos poderiam levá-la a outro servidor.
        set_u32(&api, session.0, WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_NEVER);
        // Só TLS 1.2 ou mais novo (o 1.3 pode não existir em Windows antigos).
        let tls = WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2 | WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3;
        if !set_u32(&api, session.0, WINHTTP_OPTION_SECURE_PROTOCOLS, tls) {
            set_u32(&api, session.0, WINHTTP_OPTION_SECURE_PROTOCOLS, WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2);
        }
        let connection = Owned((api.connect)(session.0, wide(url.host).as_ptr(), url.port, 0), &api).check()?;
        let flags = if url.secure { WINHTTP_FLAG_SECURE } else { 0 };
        let post = wide("POST");
        let request =
            Owned((api.open_request)(connection.0, post.as_ptr(), wide(url.path).as_ptr(), null(), null(), null(), flags), &api)
                .check()?;

        let mut headers = String::from("Content-Type: application/json; charset=utf-8\r\n");
        if let Some(key) = key {
            headers += &format!("Authorization: Bearer {key}\r\n");
        }
        let mut headers = wide(&headers);
        let bytes = body.as_bytes();
        let sent = (api.send_request)(
            request.0,
            headers.as_ptr(),
            u32::MAX, // cabeçalho terminado em zero
            bytes.as_ptr().cast(),
            bytes.len() as u32,
            bytes.len() as u32,
            0,
        ) != 0;
        headers.fill(0); // tira a chave da memória assim que foi enviada
        if !sent || (api.receive_response)(request.0, null_mut()) == 0 {
            return Err(net_error());
        }

        let mut status = 0u32;
        let mut size = 4u32;
        (api.query_headers)(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            (&mut status as *mut u32).cast(),
            &mut size,
            null_mut(),
        );

        let mut out = Vec::new();
        loop {
            let mut available = 0u32;
            if (api.query_data_available)(request.0, &mut available) == 0 {
                return Err(net_error());
            }
            if available == 0 {
                break;
            }
            if out.len() + available as usize > MAX_RESPONSE {
                return Err("a resposta do servidor veio grande demais.".into());
            }
            let start = out.len();
            out.resize(start + available as usize, 0);
            let mut read = 0u32;
            if (api.read_data)(request.0, out[start..].as_mut_ptr().cast(), available, &mut read) == 0 {
                return Err(net_error());
            }
            out.truncate(start + read as usize);
        }
        Ok((status, String::from_utf8_lossy(&out).into_owned()))
    }
}

unsafe fn set_u32(api: &WinHttp, handle: Handle, option: u32, value: u32) -> bool {
    (api.set_option)(handle, option, (&value as *const u32).cast(), 4) != 0
}

fn net_error() -> String {
    match unsafe { GetLastError() } {
        12002 => "a API demorou demais para responder.".into(),
        12007 | 12029 => "sem conexão com a internet (ou com o servidor).".into(),
        12175 | 12157 | 12045 | 12038 | 12037 => "falha na conexão segura (HTTPS): certificado ou protocolo recusado.".into(),
        code => format!("erro de rede {code}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_parsed_and_plain_http_stays_local() {
        let u = parse_url("https://api.openai.com/v1/chat/completions").unwrap();
        assert_eq!(u, Url { secure: true, host: "api.openai.com", port: 443, path: "/v1/chat/completions" });
        let u = parse_url("http://localhost:11434/v1/chat/completions").unwrap();
        assert_eq!((u.host, u.port), ("localhost", 11434));
        assert_eq!(parse_url("http://[::1]:8080/x").unwrap().host, "::1");
        assert!(parse_url("http://api.openai.com/v1").is_err());
        assert!(parse_url("https://user@evil.com/v1").is_err());
        assert!(parse_url("ftp://x").is_err());
    }

    #[test]
    fn winhttp_loads_from_system32() {
        unsafe { WinHttp::load().unwrap() };
    }
}
