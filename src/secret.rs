//! Chave da API no Gerenciador de Credenciais do Windows, protegida pela sua
//! conta (DPAPI). Nunca em arquivo do app nem em variável de ambiente — que todo
//! programa aberto depois herdaria. O plugin de conversa lê a mesma credencial.

use std::{mem::zeroed, ptr::null_mut};

use windows_sys::Win32::{
    Foundation::ERROR_SUCCESS,
    Security::Credentials::*,
    System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ},
};

use crate::win::w;

/// Onde a chave está guardada.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeySource {
    /// No Gerenciador de Credenciais (colada nas Configurações).
    Vault,
    /// Numa variável de ambiente que você mesmo criou (ex.: `setx`).
    Environment,
}

/// Nome da credencial — o plugin procura exatamente este.
fn target(name: &str) -> Vec<u16> {
    w(&format!("StayAlone:{name}"))
}

/// Nome aceito para identificar a chave (ex.: GEMINI_API_KEY).
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Formato plausível de chave: sem espaços nem caracteres de controle
/// (evita, por exemplo, quebrar o cabeçalho HTTP em que ela vai).
pub fn valid_key(key: &str) -> bool {
    (8..=512).contains(&key.len()) && key.chars().all(|c| c.is_ascii_graphic())
}

pub fn find(name: &str) -> Option<KeySource> {
    if !valid_name(name) {
        return None;
    }
    unsafe {
        let mut cred: *mut CREDENTIALW = null_mut();
        if CredReadW(target(name).as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) != 0 {
            CredFree(cred.cast());
            return Some(KeySource::Vault);
        }
    }
    let in_process = std::env::var(name).is_ok_and(|v| !v.trim().is_empty());
    (in_process || user_env_has(name)).then_some(KeySource::Environment)
}

/// A variável existe nas variáveis do usuário (criadas depois que o app abriu)?
fn user_env_has(name: &str) -> bool {
    let mut len = 0u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w("Environment").as_ptr(),
            w(name).as_ptr(),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            null_mut(),
            null_mut(),
            &mut len,
        )
    };
    status == ERROR_SUCCESS && len > 2
}

pub fn save(name: &str, key: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err("o nome da chave só pode ter letras, números e _.".into());
    }
    if !valid_key(key) {
        return Err("isso não parece uma chave de API (sem espaços, de 8 a 512 caracteres).".into());
    }
    let mut target = target(name);
    let mut user = w("StayAlone");
    let mut blob = key.as_bytes().to_vec();
    let ok = unsafe {
        let mut cred: CREDENTIALW = zeroed();
        cred.Type = CRED_TYPE_GENERIC;
        cred.TargetName = target.as_mut_ptr();
        cred.UserName = user.as_mut_ptr();
        cred.CredentialBlobSize = blob.len() as u32;
        cred.CredentialBlob = blob.as_mut_ptr();
        cred.Persist = CRED_PERSIST_LOCAL_MACHINE; // só neste PC, não viaja com o perfil
        CredWriteW(&cred, 0) != 0
    };
    blob.fill(0);
    if ok {
        Ok(())
    } else {
        Err("o Windows não deixou guardar a chave.".into())
    }
}

pub fn remove(name: &str) {
    if valid_name(name) {
        unsafe { CredDeleteW(target(name).as_ptr(), CRED_TYPE_GENERIC, 0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_keys_are_validated() {
        assert!(valid_name("GEMINI_API_KEY"));
        assert!(!valid_name("PATH;x") && !valid_name("") && !valid_name("A B"));
        assert!(valid_key("AIzaSyD-abc_123"));
        assert!(!valid_key("abc\r\nX-Evil: 1") && !valid_key("curta") && !valid_key("com espaço aqui"));
    }

    /// Mexe no Gerenciador de Credenciais de verdade: rode com `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn vault_round_trip() {
        let name = "STAYALONE_SELFTEST";
        save(name, "chave-de-teste-123").unwrap();
        assert_eq!(find(name), Some(KeySource::Vault));
        remove(name);
        assert_eq!(find(name), None);
    }

    #[test]
    fn unknown_names_are_not_found() {
        assert_eq!(find("STAYALONE_TEST_NO_SUCH_KEY_42"), None);
        assert_eq!(find("inválido!"), None);
    }
}
