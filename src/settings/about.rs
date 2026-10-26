//! Página Sobre: segurança e privacidade.

use super::*;

// --- segurança e privacidade ------------------------------------------------------

/// O domínio de um endereço, curto: "https://api.openai.com/v1" → "openai.com"
/// (endereço IP ou nome sem ponto, como localhost, fica inteiro, com a porta).
pub(super) fn site_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let name = host.split(':').next().unwrap_or("");
    if name.bytes().all(|b| b.is_ascii_digit() || b == b'.') || !name.contains('.') {
        return host;
    }
    let dots: Vec<usize> = name.match_indices('.').map(|(i, _)| i).collect();
    if dots.len() < 2 { name } else { &name[dots[dots.len() - 2] + 1..] }
}

/// Commit de origem do build, curto: "58a94be" (ou "58a94be-dirty").
pub(super) fn short_commit() -> String {
    let commit = env!("STAYALONE_COMMIT");
    let (hash, dirty) = commit.split_once('-').unwrap_or((commit, ""));
    let hash = &hash[..hash.len().min(7)];
    if dirty.is_empty() { hash.into() } else { format!("{hash}-{dirty}") }
}

/// As proteções marcadas no próprio `.exe` em execução (DllCharacteristics do
/// cabeçalho PE já mapeado na memória, sem ler o arquivo de novo).
pub(super) fn protections() -> String {
    // SAFETY: o módulo principal fica mapeado a vida toda; os offsets são os do PE32+.
    let flags = unsafe {
        let base = GetModuleHandleW(null()) as *const u8;
        let pe = base.add(std::ptr::read_unaligned(base.add(0x3C) as *const u32) as usize);
        std::ptr::read_unaligned(pe.add(24 + 70) as *const u16)
    };
    // "ASLR" só com as duas marcas (endereço aleatório e de alta entropia).
    let names = [(0x0100, "DEP"), (0x0060, "ASLR"), (0x4000, "CFG")];
    let on: Vec<&str> = names.iter().filter(|(bits, _)| flags & bits == *bits).map(|&(_, n)| n).collect();
    let off: Vec<&str> = names.iter().filter(|(bits, _)| flags & bits != *bits).map(|&(_, n)| n).collect();
    if off.is_empty() { on.join(" · ") } else { fill(tr("{} (sem {})"), &[&on.join(" · "), &off.join(", ")]) }
}

/// Preenche o cartão com o estado atual (inclusive o que foi mudado e ainda não salvo).
pub(super) unsafe fn fill_security(hwnd: HWND) {
    let Some(st) = state(hwnd) else { return };
    let sha = st.exe_sha.get_or_insert_with(|| {
        std::env::current_exe().and_then(std::fs::read).map(|b| crate::sha256::hex(&b)).unwrap_or_default()
    });
    let sha = if sha.len() == 64 { format!("{}…{}", &sha[..16], &sha[48..]) } else { tr("não consegui ler").into() };
    let program = short_commit();

    let updates = if is_checked(hwnd, IDC_UPDATES) { tr("Só assinadas, com o seu OK") } else { tr("Desligadas") };

    let key_name = text_of(item(hwnd, IDC_KEYENV));
    let key = match secret::find(&key_name) {
        _ if key_name.trim().is_empty() => tr("Não precisa"),
        Some(KeySource::Vault) => tr("Guardada no Windows"),
        Some(KeySource::Environment) => tr("Em variável de ambiente"),
        None => tr("Nenhuma"),
    };

    let enabled = |id: &str| st.draft.plugins.iter().any(|e| e.id == id);
    let mut net = Vec::new();
    if is_checked(hwnd, IDC_UPDATES) {
        net.push("GitHub".to_string());
    }
    if enabled(plugins::NATIVE_ID) {
        let base = text_of(item(hwnd, IDC_BASE));
        net.push(fill(tr("IA ({})"), &[&site_of(&base)]));
    }
    net.push(tr("Galeria").to_string());

    let external: Vec<&Plugin> = st.plugins.iter().filter(|p| !p.is_native()).collect();
    let on = external.iter().filter(|p| enabled(&p.id)).count();
    let online = external.iter().filter(|p| enabled(&p.id) && p.internet).count();
    let with_folders = st.draft.plugins.iter().filter(|e| !e.folders.is_empty()).count();
    let plugins = if on == 0 {
        tr("Nenhum ligado").to_string()
    } else {
        let mut parts = vec![fill(tr("{} ligado(s), isolados"), &[&on])];
        if online > 0 {
            parts.push(fill(tr("{} com internet"), &[&online]));
        }
        if with_folders > 0 {
            parts.push(fill(tr("{} com pastas"), &[&with_folders]));
        }
        parts.join(" · ")
    };

    let values = [program, sha, protections(), updates.into(), key.into(), net.join(" · "), plugins];
    for (i, value) in values.iter().enumerate() {
        set_text(hwnd, IDC_SEC_FIRST + i as i32, value);
    }
}

