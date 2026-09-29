# Release local do !StayAlone (enquanto o GitHub Actions estiver fora do ar).
#
#   powershell -ExecutionPolicy Bypass -File tools\release.ps1 -Version 1.2.5
#       compila, confere, assina e grava o BUILDINFO
#   ... -Version 1.2.5 -Publish
#       ... e cria a release no GitHub
#
# Sempre o mesmo ambiente: toolchain Rust MSVC fixada (com Control Flow Guard, CRT
# estático e /CETCOMPAT), um checkout limpo do commit num caminho fixo, e para quando
# falta alguma proteção ou quando o Microsoft Defender acusa o .exe.
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

function Run {
    # Roda um programa direto (sem `cmd`, que pode não ser o do Windows no PATH) e para se
    # ele falhar. Nas chamadas, '--' vai entre aspas: solto, o PowerShell o engole.
    $program, $rest = $args
    Write-Host "> $program $rest" -ForegroundColor Cyan
    & $program @rest
    if ($LASTEXITCODE) { throw "falhou: $program $rest" }
}
function Sha256([string] $path) { (Get-FileHash $path -Algorithm SHA256).Hash.ToLower() }

# --- pré-condições --------------------------------------------------------------
if (git status --porcelain) { throw 'há mudanças não commitadas: a release precisa sair de um commit exato' }
$cargoVersion = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
if ($cargoVersion -ne $Version) { throw "Cargo.toml está em $cargoVersion, não $Version" }
$notes = "docs/releases/v$Version.md"
if (-not (Test-Path $notes)) { throw "faltam as notas em $notes" }
$commit = (git rev-parse HEAD).Trim()

# Compila um checkout limpo do commit, sempre em C:\StayAloneBuild\src: o Rust usa o
# caminho da pasta do projeto no hash interno do crate (muda o layout do código), então
# só o mesmo caminho dá o mesmo .exe, aqui ou em qualquer PC. De quebra, nada da pasta de
# trabalho (arquivos fora do git, fins de linha) entra na release.
$src = 'C:\StayAloneBuild\src'
if (Test-Path $src) {
    git worktree remove --force $src 2>$null
    if (Test-Path $src) { Remove-Item -Recurse -Force $src }
}
git worktree prune
Run git worktree add --detach $src $commit
Set-Location $src

# Caminhos desta máquina (nome de usuário, pasta do projeto) não entram no .exe. Esta
# variável substitui a de .cargo/config.toml: as flags de lá vão junto.
$rustflags = (Select-String -Path .cargo/config.toml -Pattern '^rustflags = (.+)').Matches[0].Groups[1].Value | ConvertFrom-Json
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { "$env:USERPROFILE\.cargo" }
$remap = "--remap-path-prefix=$cargoHome=/cargo", "--remap-path-prefix=$src=/src"
$env:CARGO_ENCODED_RUSTFLAGS = ($rustflags + $remap) -join [char]0x1f
Run rustup toolchain install $Toolchain --profile minimal --component clippy

# --- build e testes ---------------------------------------------------------------
Run cargo +$Toolchain clippy --release --all-targets '--' -D warnings
Run cargo +$Toolchain test --release
Run cargo +$Toolchain build --release
$exe = 'target\release\dontStayAlone.exe'
& .github/scripts/check-exe.ps1 -Exe $exe -Version $Version

# O commit embutido (cartão de segurança) tem que ser este, sem "-dirty".
$short = $commit.Substring(0, 12)
$text = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes((Resolve-Path $exe)))
if ($text -notmatch "$short(?!-dirty)") { throw "o .exe não traz o commit $short (build velho?)" }
if ($text.Contains($env:USERPROFILE)) { throw "o .exe traz o caminho $env:USERPROFILE" }

# O Defender (que o Chrome e o Edge usam em cada download) às vezes acusa um .exe novo
# sem assinatura só pelo modelo de machine learning ("!ml"). Não publica o que ele acusa.
$mp = "$env:ProgramFiles\Windows Defender\MpCmdRun.exe"
if (Test-Path $mp) {
    $scan = & $mp -Scan -ScanType 3 -File (Resolve-Path $exe).Path -DisableRemediation 2>&1 | Out-String
    if ($scan -match 'Threat\s+:\s+(\S+)') {
        throw "o Microsoft Defender acusou o .exe ($($Matches[1])). Não publique; se for falso positivo, envie em https://www.microsoft.com/en-us/wdsi/filesubmission"
    }
    if ($scan -notmatch 'found no threats') { throw "a verificação do Defender não terminou:`n$scan" }
    Write-Host "Microsoft Defender: nada encontrado"
}

# --- assinatura da atualização (Ed25519, chave offline) ---------------------------
Run cargo +$Toolchain run --release --example assinar '--' $Version $exe
Run cargo +$Toolchain test --release '--' --ignored release_is_signed

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
pasta do build    $src (checkout limpo; o caminho entra no hash do crate)
rustflags         $($rustflags -join ' ') --remap-path-prefix=<CARGO_HOME>=/cargo --remap-path-prefix=<repo>=/src
link.exe          $($linkArgs -join ' ')
perfil release    $($relProfile -join '; ')

Para conferir: com a mesma toolchain, MSVC e Windows SDK, faça um checkout deste commit
em $src e compile com estas rustflags (cargo build --release), ou rode
tools/release.ps1 -Version $Version, e compare o SHA-256. A assinatura Ed25519 está em
dontStayAlone.exe.sig.
"@
$buildinfo = 'target\release\BUILDINFO.txt'
[IO.File]::WriteAllText((Join-Path $src $buildinfo), $info.Replace("`r`n", "`n"))
Write-Host $info

# --- publicação -------------------------------------------------------------------
$create = @('release', 'create', "v$Version", $exe, "$exe.sig", $buildinfo, '--target', $commit,
    '--title', "!StayAlone $Version", '--notes-file', $notes)
if ($Publish) { Run gh @create } else { Write-Host "Tudo pronto (em $src). Para publicar: gh $create" -ForegroundColor Green }
