//! Gera os recursos do Windows (ícone, manifesto e versão) sem ferramentas externas:
//! desenha o ícone a partir do sprite do Calcifer e escreve um objeto COFF com a
//! seção `.rsrc`, que o linker (GNU ld ou link.exe) embute no executável.

use std::{env, fs, path::PathBuf};

const ICON_SPRITE: &str = "assets/mascots/calcifer/mascot.txt";
const ICON_SIZES: [u32; 5] = [16, 32, 48, 64, 256];

const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;
const RT_VERSION: u32 = 16;
const RT_MANIFEST: u32 = 24;
const LANG_EN_US: u32 = 0x0409;

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="StayAlone" version="1.0.0.0"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
        processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
      <activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>
    </windowsSettings>
  </application>
</assembly>
"#;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICON_SPRITE}");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let (w, h, pixels) = idle_sprite(&fs::read_to_string(ICON_SPRITE).expect("sprite do ícone"));
    let mut resources = Vec::new();
    let mut group = vec![0, 0, 1, 0, ICON_SIZES.len() as u8, 0]; // GRPICONDIR
    for (i, &size) in ICON_SIZES.iter().enumerate() {
        let rgba = scale_to(&pixels, w, h, size);
        let image = if size >= 256 { png(&rgba, size) } else { dib(&rgba, size) };
        let id = i as u16 + 1;
        // GRPICONDIRENTRY
        group.extend([(size % 256) as u8, (size % 256) as u8, 0, 0]);
        group.extend(1u16.to_le_bytes());
        group.extend(32u16.to_le_bytes());
        group.extend((image.len() as u32).to_le_bytes());
        group.extend(id.to_le_bytes());
        resources.push((RT_ICON, id as u32, image));
    }
    resources.push((RT_GROUP_ICON, 1, group));
    resources.push((RT_VERSION, 1, version_info()));
    resources.push((RT_MANIFEST, 1, MANIFEST.as_bytes().to_vec()));

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("resources.o");
    fs::write(&out, coff(&resources)).unwrap();
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

// --- ícone ---------------------------------------------------------------

/// Lê a paleta e o frame `idle`; devolve a área útil recortada (RGBA).
fn idle_sprite(src: &str) -> (u32, u32, Vec<[u8; 4]>) {
    let mut colors = [[0u8; 4]; 128];
    let mut rows = Vec::new();
    let mut in_idle = false;
    for line in src.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["color", key, hex] => {
                let v = u32::from_str_radix(hex, 16).unwrap();
                colors[key.as_bytes()[0] as usize] = [(v >> 16) as u8, (v >> 8) as u8, v as u8, 255];
            }
            ["frame", name] => in_idle = *name == "idle",
            _ if in_idle && rows.len() < 16 => rows.push(line.as_bytes().to_vec()),
            _ => {}
        }
    }
    // Recorta as linhas/colunas vazias para o bicho ocupar o ícone todo.
    let used = |x: usize, y: usize| rows[y][x] != b'.';
    let top = (0..16).find(|&y| (0..16).any(|x| used(x, y))).unwrap();
    let bottom = (0..16).rev().find(|&y| (0..16).any(|x| used(x, y))).unwrap();
    let left = (0..16).find(|&x| (0..16).any(|y| used(x, y))).unwrap();
    let right = (0..16).rev().find(|&x| (0..16).any(|y| used(x, y))).unwrap();
    let side = (bottom - top).max(right - left) + 1;
    // Centraliza num quadrado.
    let (ox, oy) = ((side - (right - left + 1)) / 2, (side - (bottom - top + 1)) / 2);
    let mut px = vec![[0u8; 4]; side * side];
    for y in top..=bottom {
        for x in left..=right {
            if used(x, y) {
                px[(y - top + oy) * side + (x - left + ox)] = colors[rows[y][x] as usize];
            }
        }
    }
    (side as u32, side as u32, px)
}

/// Amplia por vizinho mais próximo, centralizando (pixel art fica nítida).
fn scale_to(src: &[[u8; 4]], w: u32, h: u32, size: u32) -> Vec<[u8; 4]> {
    let k = (size / w.max(h)).max(1);
    let (sw, sh) = (w * k, h * k);
    let (ox, oy) = ((size.saturating_sub(sw)) / 2, (size.saturating_sub(sh)) / 2);
    let mut out = vec![[0u8; 4]; (size * size) as usize];
    for y in 0..sh.min(size) {
        for x in 0..sw.min(size) {
            out[((y + oy) * size + x + ox) as usize] = src[((y / k) * w + x / k) as usize];
        }
    }
    out
}

/// Imagem de ícone no formato DIB (BGRA de baixo para cima + máscara AND).
fn dib(rgba: &[[u8; 4]], size: u32) -> Vec<u8> {
    let mask_row = size.div_ceil(32) * 4;
    let mut v = Vec::new();
    for field in [40u32, size, size * 2] {
        v.extend(field.to_le_bytes());
    }
    v.extend(1u16.to_le_bytes());
    v.extend(32u16.to_le_bytes());
    v.extend(0u32.to_le_bytes());
    v.extend((size * size * 4 + mask_row * size).to_le_bytes());
    v.extend([0u8; 16]);
    for y in (0..size).rev() {
        for x in 0..size {
            let [r, g, b, a] = rgba[(y * size + x) as usize];
            v.extend([b, g, r, a]);
        }
    }
    for y in (0..size).rev() {
        let mut row = vec![0u8; mask_row as usize];
        for x in 0..size {
            if rgba[(y * size + x) as usize][3] == 0 {
                row[(x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
        v.extend(row);
    }
    v
}

fn png(rgba: &[[u8; 4]], size: u32) -> Vec<u8> {
    let mut raw = Vec::with_capacity((size * (size * 4 + 1)) as usize);
    for y in 0..size {
        raw.push(0); // sem filtro
        for x in 0..size {
            raw.extend(rgba[(y * size + x) as usize]);
        }
    }
    let mut ihdr = Vec::new();
    ihdr.extend(size.to_be_bytes());
    ihdr.extend(size.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in [
        (b"IHDR", ihdr),
        (b"IDAT", miniz_oxide::deflate::compress_to_vec_zlib(&raw, 10)),
        (b"IEND", Vec::new()),
    ] {
        out.extend((data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend(kind);
        out.extend(&data);
        let crc = crc32(&out[start..]);
        out.extend(crc.to_be_bytes());
    }
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

// --- versão (VS_VERSIONINFO) ---------------------------------------------

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect()
}

fn pad4(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
}

/// Um nó da árvore de versão: cabeçalho + chave + valor + filhos, alinhados a 4 bytes.
fn node(key: &str, value: &[u8], value_len: u16, text: bool, children: &[Vec<u8>]) -> Vec<u8> {
    let mut v = vec![0, 0];
    v.extend(value_len.to_le_bytes());
    v.extend((text as u16).to_le_bytes());
    v.extend(utf16z(key));
    pad4(&mut v);
    v.extend(value);
    for child in children {
        pad4(&mut v);
        v.extend(child);
    }
    let len = v.len() as u16;
    v[0..2].copy_from_slice(&len.to_le_bytes());
    v
}

fn version_info() -> Vec<u8> {
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let parts: Vec<u16> = version.split('.').map(|p| p.parse().unwrap_or(0)).chain([0; 4]).take(4).collect();
    let ms = (parts[0] as u32) << 16 | parts[1] as u32;
    let ls = (parts[2] as u32) << 16 | parts[3] as u32;
    let mut fixed = Vec::new();
    for field in [0xFEEF_04BD, 0x0001_0000, ms, ls, ms, ls, 0x3F, 0, 0x0004_0004, 1, 0, 0, 0u32] {
        fixed.extend(field.to_le_bytes());
    }
    let string = |k: &str, v: &str| {
        let value = utf16z(v);
        node(k, &value, (value.len() / 2) as u16, true, &[])
    };
    let strings = [
        string("FileDescription", "!StayAlone"),
        string("ProductName", "!StayAlone"),
        string("FileVersion", &version),
        string("ProductVersion", &version),
        string("OriginalFilename", "dontStayAlone.exe"),
        string("LegalCopyright", "© 2026 Atylla Azevedo · MIT License"),
        string("Comments", "Um mascote leve que faz companhia na área de trabalho."),
    ];
    let table = node("040904B0", &[], 0, true, &strings);
    let string_info = node("StringFileInfo", &[], 0, true, &[table]);
    let translation = [0x09, 0x04, 0xB0, 0x04]; // en-US, UTF-16
    let var = node("Translation", &translation, 4, false, &[]);
    let var_info = node("VarFileInfo", &[], 0, true, &[var]);
    node("VS_VERSION_INFO", &fixed, fixed.len() as u16, false, &[string_info, var_info])
}

// --- objeto COFF com a seção .rsrc -----------------------------------------

/// Monta a árvore de recursos (tipo → id → idioma → dados) num objeto COFF x64.
fn coff(resources: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
    let mut sorted: Vec<&(u32, u32, Vec<u8>)> = resources.iter().collect();
    sorted.sort_by_key(|r| (r.0, r.1));
    let mut types: Vec<u32> = sorted.iter().map(|r| r.0).collect();
    types.dedup();

    let dir_size = |entries: usize| 16 + 8 * entries;
    let root = dir_size(types.len());
    let type_dirs: usize = types.iter().map(|t| dir_size(sorted.iter().filter(|r| r.0 == *t).count())).sum();
    let lang_dirs = sorted.len() * dir_size(1);
    let entries_at = root + type_dirs + lang_dirs;
    let mut data_at = entries_at + 16 * sorted.len();

    let mut section = Vec::new();
    let dir = |s: &mut Vec<u8>, count: usize| {
        s.extend([0u8; 12]);
        s.extend(0u16.to_le_bytes());
        s.extend((count as u16).to_le_bytes());
    };
    let entry = |s: &mut Vec<u8>, id: u32, offset: usize, subdir: bool| {
        s.extend(id.to_le_bytes());
        s.extend((offset as u32 | if subdir { 0x8000_0000 } else { 0 }).to_le_bytes());
    };

    // Raiz: um item por tipo.
    dir(&mut section, types.len());
    let mut next = root;
    for t in &types {
        entry(&mut section, *t, next, true);
        next += dir_size(sorted.iter().filter(|r| r.0 == *t).count());
    }
    // Nível 2: ids de cada tipo.
    let mut lang_dir = root + type_dirs;
    for t in &types {
        let of_type: Vec<_> = sorted.iter().filter(|r| r.0 == *t).collect();
        dir(&mut section, of_type.len());
        for r in of_type {
            entry(&mut section, r.1, lang_dir, true);
            lang_dir += dir_size(1);
        }
    }
    // Nível 3: idioma.
    for i in 0..sorted.len() {
        dir(&mut section, 1);
        entry(&mut section, LANG_EN_US, entries_at + 16 * i, false);
    }
    // Entradas de dados (o endereço é corrigido pelo linker via relocação).
    let mut relocations = Vec::new();
    for r in &sorted {
        relocations.push(section.len() as u32);
        section.extend((data_at as u32).to_le_bytes());
        section.extend((r.2.len() as u32).to_le_bytes());
        section.extend([0u8; 8]);
        data_at += r.2.len().div_ceil(8) * 8;
    }
    for r in &sorted {
        section.extend(&r.2);
        while section.len() % 8 != 0 {
            section.push(0);
        }
    }

    let data_ptr = 20 + 40;
    let reloc_ptr = data_ptr + section.len();
    let symbols_ptr = reloc_ptr + relocations.len() * 10;

    let mut out = Vec::new();
    // Cabeçalho COFF (AMD64, 1 seção, 2 registros de símbolo).
    out.extend(0x8664u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend((symbols_ptr as u32).to_le_bytes());
    out.extend(2u32.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    // Cabeçalho da seção.
    out.extend(*b".rsrc\0\0\0");
    out.extend(0u32.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend((section.len() as u32).to_le_bytes());
    out.extend((data_ptr as u32).to_le_bytes());
    out.extend((reloc_ptr as u32).to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend((relocations.len() as u16).to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(0x4000_0040u32.to_le_bytes()); // dados inicializados, leitura
    out.extend(&section);
    for offset in &relocations {
        out.extend(offset.to_le_bytes());
        out.extend(0u32.to_le_bytes()); // símbolo 0 (.rsrc)
        out.extend(3u16.to_le_bytes()); // IMAGE_REL_AMD64_ADDR32NB
    }
    // Símbolo da seção + registro auxiliar.
    out.extend(*b".rsrc\0\0\0");
    out.extend(0u32.to_le_bytes());
    out.extend(1i16.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.push(3); // IMAGE_SYM_CLASS_STATIC
    out.push(1);
    out.extend((section.len() as u32).to_le_bytes());
    out.extend((relocations.len() as u16).to_le_bytes());
    out.extend([0u8; 12]);
    out.extend(4u32.to_le_bytes()); // tabela de strings vazia
    out
}
