//! Assina as releases do !StayAlone (Ed25519). Ferramenta de quem publica: não entra
//! no .exe.
//!
//!   cargo run --example assinar -- nova-chave
//!   cargo run --example assinar -- 1.2.3 target\release\dontStayAlone.exe
//!
//! O segundo comando grava `dontStayAlone.exe.sig` ao lado do .exe; publique os dois na
//! release. O app só instala uma atualização cuja assinatura confira com a chave pública
//! embutida nele (`RELEASE_KEY` em src/update.rs).
//!
//! A chave privada fica em `%USERPROFILE%\.stayalone\release-key.txt` (ou no caminho de
//! `STAYALONE_RELEASE_KEY`) e nunca vai para o repositório. Guarde uma cópia em lugar
//! seguro: sem ela, as versões publicadas não conseguem mais se atualizar sozinhas.

#[path = "../src/sha256.rs"]
#[allow(dead_code)]
mod sha256;

use std::{fs, path::PathBuf, process::exit};

use ed25519_compact::{KeyPair, PublicKey, Seed, Signature};

/// A frase assinada. Igual a `update::signed_message`: amarra o arquivo (SHA-256) à versão,
/// então uma release antiga assinada não passa por uma versão nova.
fn signed_message(version: &str, sha256: &str) -> String {
    format!("!StayAlone {version} sha256:{sha256}")
}

fn key_path() -> PathBuf {
    if let Some(path) = std::env::var_os("STAYALONE_RELEASE_KEY") {
        return path.into();
    }
    let home = std::env::var_os("USERPROFILE").unwrap_or_else(|| fail("USERPROFILE não definido"));
    PathBuf::from(home).join(".stayalone").join("release-key.txt")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    text.len().is_multiple_of(2)
        .then(|| (0..text.len()).step_by(2).map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok()).collect())
        .flatten()
}

fn fail(message: &str) -> ! {
    eprintln!("erro: {message}");
    exit(1)
}

fn new_key() {
    let path = key_path();
    if path.exists() {
        fail(&format!("já existe uma chave em {} (não sobrescrevo)", path.display()));
    }
    let seed = Seed::generate();
    let keys = KeyPair::from_seed(seed);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).unwrap_or_else(|e| fail(&e.to_string()));
    }
    fs::write(&path, format!("{}\n", hex(seed.as_ref()))).unwrap_or_else(|e| fail(&e.to_string()));
    println!("Chave privada gravada em {}", path.display());
    println!("Faça uma cópia de segurança dela. Chave pública (cole em RELEASE_KEY, src/update.rs):\n");
    print_public(&keys.pk);
}

fn print_public(pk: &PublicKey) {
    let bytes: Vec<String> = pk.as_ref().iter().map(|b| format!("0x{b:02x}")).collect();
    println!("const RELEASE_KEY: [u8; 32] = [\n    {},\n    {},\n];", bytes[..16].join(", "), bytes[16..].join(", "));
}

fn sign(version: &str, exe: &str) {
    let version = version.trim_start_matches('v');
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 || !parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) {
        fail("versão no formato 1.2.3");
    }
    let text = fs::read_to_string(key_path()).unwrap_or_else(|e| fail(&format!("{}: {e}", key_path().display())));
    let seed = unhex(&text).and_then(|b| Seed::from_slice(&b).ok()).unwrap_or_else(|| fail("chave privada inválida"));
    let keys = KeyPair::from_seed(seed);

    let bytes = fs::read(exe).unwrap_or_else(|e| fail(&format!("{exe}: {e}")));
    if !bytes.starts_with(b"MZ") {
        fail("isso não é um .exe");
    }
    let sha = sha256::hex(&bytes);
    let message = signed_message(version, &sha);
    let signature = keys.sk.sign(message.as_bytes(), None);
    // Confere com a própria chave pública antes de gravar.
    let check = Signature::from_slice(signature.as_ref()).unwrap();
    keys.pk.verify(message.as_bytes(), &check).unwrap_or_else(|_| fail("a assinatura não conferiu"));

    let out = format!("{exe}.sig");
    fs::write(&out, format!("{}\n", hex(signature.as_ref()))).unwrap_or_else(|e| fail(&e.to_string()));
    println!("{message}\nAssinatura gravada em {out}\nChave pública usada:");
    print_public(&keys.pk);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd] if cmd == "nova-chave" => new_key(),
        [version, exe] => sign(version, exe),
        _ => fail("uso: assinar nova-chave | assinar <versão> <caminho do .exe>"),
    }
}
