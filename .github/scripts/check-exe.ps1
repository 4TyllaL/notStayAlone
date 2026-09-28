# Confere que o executável saiu com as proteções esperadas e a versão certa.
# Uso: check-exe.ps1 -Exe target\release\dontStayAlone.exe [-Version 1.2.3]
param(
    [Parameter(Mandatory)] [string] $Exe,
    [string] $Version
)
$ErrorActionPreference = 'Stop'
$bytes = [IO.File]::ReadAllBytes((Resolve-Path $Exe))
$pe = [BitConverter]::ToInt32($bytes, 0x3C)
if ([Text.Encoding]::ASCII.GetString($bytes, $pe, 4) -ne "PE`0`0") { throw "não é um executável PE" }
$optional = $pe + 24
if ([BitConverter]::ToUInt16($bytes, $optional) -ne 0x20B) { throw "não é PE32+ (64 bits)" }
$dll = [BitConverter]::ToUInt16($bytes, $optional + 70)

$flags = [ordered]@{
    'ASLR (DYNAMIC_BASE)'          = 0x0040
    'ASLR de alta entropia'        = 0x0020
    'DEP (NX_COMPAT)'              = 0x0100
    'Control Flow Guard (GUARD_CF)' = 0x4000
}
$missing = @()
foreach ($name in $flags.Keys) {
    $on = ($dll -band $flags[$name]) -ne 0
    "{0,-32} {1}" -f $name, $(if ($on) { 'ok' } else { 'FALTANDO' })
    if (-not $on) { $missing += $name }
}

# Não pode depender do runtime do Visual C++ (o .exe tem que rodar sozinho).
$text = [Text.Encoding]::ASCII.GetString($bytes)
foreach ($runtime in 'vcruntime140.dll', 'msvcp140.dll') {
    if ($text -match [regex]::Escape($runtime)) { $missing += "depende de $runtime" }
}

$info = (Get-Item $Exe).VersionInfo
"versão do arquivo                {0}" -f $info.ProductVersion
if ($Version -and $info.ProductVersion -ne $Version) { $missing += "versão $($info.ProductVersion) != $Version" }

if ($missing.Count) { throw "Problemas: $($missing -join '; ')" }
"tudo certo"
