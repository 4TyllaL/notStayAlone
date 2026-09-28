# Release local do !StayAlone (enquanto o GitHub Actions estiver fora do ar).
#
#   powershell -ExecutionPolicy Bypass -File tools\release.ps1 -Version 1.2.5
#       compila, confere, assina e grava o BUILDINFO
#   ... -Version 1.2.5 -Publish
#       ... e cria a release no GitHub
#
# Sempre o mesmo ambiente: toolchain Rust MSVC fixada (com Control Flow Guard, CRT
# estático e /CETCOMPAT), árvore git limpa, e para quando falta alguma proteção.
# O BUILDINFO registra commit, versões do Rust/Cargo/MSVC/Windows SDK, flags e hashes,
# para quem quiser auditar ou reproduzir a release depois.
param(
    [Parameter(Mandatory)] [string] $Version,
    [switch] $Publish
)
$ErrorActionPreference = 'Stop'
$Toolchain = '1.98.1-x86_64-pc-windows-msvc'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

function Run([string] $what) {
    # Roda um comando nativo e para se ele falhar.
    Write-Host "> $what" -ForegroundColor Cyan
    cmd /c "$what 2>&1"
    if ($LASTEXITCODE) { throw "falhou: $what" }
}
function Sha256([string] $path) { (Get-FileHash $path -Algorithm SHA256).Hash.ToLower() }

# --- pré-condições --------------------------------------------------------------
if (git status --porcelain) { throw 'há mudanças não commitadas: a release precisa sair de um commit exato' }
$cargoVersion = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
if ($cargoVersion -ne $Version) { throw "Cargo.toml está em $cargoVersion, não $Version" }
$notes = "docs/releases/v$Version.md"
if (-not (Test-Path $notes)) { throw "faltam as notas em $notes" }
$commit = (git rev-parse HEAD).Trim()

# Caminhos desta máquina (nome de usuário, pasta do projeto) não entram no .exe, então o
# build sai igual em qualquer PC. Esta variável substitui a de .cargo/config.toml: as
# flags de lá vão junto.
$rustflags = (Select-String -Path .cargo/config.toml -Pattern '^rustflags = (.+)').Matches[0].Groups[1].Value | ConvertFrom-Json
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { "$env:USERPROFILE\.cargo" }
$remap = "--remap-path-prefix=$cargoHome=/cargo", "--remap-path-prefix=$root=/src"
$env:CARGO_ENCODED_RUSTFLAGS = ($rustflags + $remap) -join [char]0x1f
Run "rustup toolchain install $Toolchain --profile minimal --component clippy"

# --- build e testes ---------------------------------------------------------------
Run "cargo +$Toolchain clippy --release --all-targets -- -D warnings"
Run "cargo +$Toolchain test --release"
Run "cargo +$Toolchain build --release"
$exe = 'target\release\dontStayAlone.exe'
& .github/scripts/check-exe.ps1 -Exe $exe -Version $Version

# O commit embutido (cartão de segurança) tem que ser este, sem "-dirty".
$short = $commit.Substring(0, 12)
$text = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes((Resolve-Path $exe)))
if ($text -notmatch "$short(?!-dirty)") { throw "o .exe não traz o commit $short (build velho?)" }
if ($text.Contains($env:USERPROFILE)) { throw "o .exe traz o caminho $env:USERPROFILE" }

# --- assinatura da atualização (Ed25519, chave offline) ---------------------------
Run "cargo +$Toolchain run --release --example assinar -- $Version $exe"
Run "cargo +$Toolchain test --release -- --ignored release_is_signed"

# --- BUILDINFO --------------------------------------------------------------------
$bytes = [IO.File]::ReadAllBytes((Resolve-Path $exe))
$pe = [BitConverter]::ToInt32($bytes, 0x3C)
$linker = '{0}.{1}' -f $bytes[$pe + 24 + 2], $bytes[$pe + 24 + 3]
# O MSVC mais novo instalado (é o que o rustc usa): ...\<VS>\<edição>\VC\Tools\MSVC\<versão>.
$msvcDir = Get-ChildItem "$env:ProgramFiles\Microsoft Visual Studio", "${env:ProgramFiles(x86)}\Microsoft Visual Studio" -ErrorAction SilentlyContinue |
    ForEach-Object { Get-ChildItem "$($_.FullName)\*\VC\Tools\MSVC\*" -Directory -ErrorAction SilentlyContinue } |
    Sort-Object { [version]$_.Name } | Select-Object -Last 1
if (-not $msvcDir) { throw 'não achei o MSVC (Visual Studio Build Tools)' }
$edition = $msvcDir.Parent.Parent.Parent.Parent
$msvc = "$($msvcDir.Name) (Visual Studio $($edition.Parent.Name) $($edition.Name))"
$sdk = (Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\Lib" |
    Where-Object { Test-Path "$($_.FullName)\um\x64\kernel32.lib" } |
    Sort-Object { [version]$_.Name } | Select-Object -Last 1).Name
$relProfile = ((Get-Content Cargo.toml -Raw) -split '\[profile\.release\]')[1] -split "`r?`n" |
    ForEach-Object { $_.Trim() } | Where-Object { $_ -match '^[\w-]+ = ' }
$linkArgs = (Select-String -Path build.rs -Pattern 'rustc-link-arg-bins=(/[A-Z:0-9x]+)').Matches | ForEach-Object { $_.Groups[1].Value }

$info = @"
!StayAlone $Version

commit            $commit
data (UTC)        $((Get-Date).ToUniversalTime().ToString('yyyy-MM-dd HH:mm'))
dontStayAlone.exe sha256:$(Sha256 $exe)
Cargo.lock        sha256:$(Sha256 Cargo.lock)

toolchain         $Toolchain
$((& rustc +$Toolchain -vV) -join "`n")
$(& cargo +$Toolchain -V)
MSVC              $msvc
linker no PE      $linker
Windows SDK       $sdk
rustflags         $($rustflags -join ' ') --remap-path-prefix=<CARGO_HOME>=/cargo --remap-path-prefix=<repo>=/src
link.exe          $($linkArgs -join ' ')
perfil release    $($relProfile -join '; ')

Para conferir: no mesmo commit, com a mesma toolchain, MSVC e Windows SDK, rode
tools/release.ps1 -Version $Version (ou cargo build com estas rustflags) e compare o SHA-256;
a assinatura Ed25519 está em dontStayAlone.exe.sig.
"@
$buildinfo = 'target\release\BUILDINFO.txt'
[IO.File]::WriteAllText((Join-Path $root $buildinfo), $info.Replace("`r`n", "`n"))
Write-Host $info

# --- publicação -------------------------------------------------------------------
$assets = "$exe $exe.sig $buildinfo"
$create = "gh release create v$Version $assets --target $commit --title `"!StayAlone $Version`" --notes-file $notes"
if ($Publish) { Run $create } else { Write-Host "Tudo pronto. Para publicar:`n  $create" -ForegroundColor Green }
