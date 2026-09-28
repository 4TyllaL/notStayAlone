<#
.SYNOPSIS
  Teste de tela do !StayAlone: abre o app de verdade numa pasta de dados isolada,
  passa pelas telas principais e confere o que foi gravado.

.DESCRIPTION
  - Nunca usa os seus dados: APPDATA aponta para uma pasta dentro de -Out.
  - Se recusa a rodar com outro !StayAlone aberto (só uma cópia roda por vez; o
    teste acabaria mexendo na sua).
  - As fotos são só das janelas do app. Janelas transparentes (painel, mascote)
    são fotografadas sobre um fundo preto e depois branco, e a transparência é
    calculada a partir das duas: nada do que está na sua tela aparece.

  Com -Docs, também atualiza as imagens do README em docs/.

.EXAMPLE
  cargo build --release; powershell -ExecutionPolicy Bypass -File tools/smoke.ps1
#>
param(
    [string]$Exe = "$PSScriptRoot\..\target\release\dontStayAlone.exe",
    [string]$Out = "$PSScriptRoot\..\target\smoke",
    [switch]$Docs
)
$ErrorActionPreference = "Stop"
$Exe = (Resolve-Path $Exe).Path
if (Get-Process | Where-Object { $_.ProcessName -like "dontStayAlone*" }) {
    throw "Tem um !StayAlone aberto. Feche (painel → Sair) antes de rodar o teste."
}
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$DocsDir = (Resolve-Path "$PSScriptRoot\..\docs").Path

Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

public static class Smoke {
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowW(string c, IntPtr t);
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] inputs, int size);
    [StructLayout(LayoutKind.Sequential)] struct INPUT { public uint type; public int dx, dy; public uint data, flags, time; public IntPtr extra; public uint pad1, pad2; }

    /// "Movimento" de mouse de zero pixel: o Windows conta como uso do PC (os
    /// lembretes só contam tempo com alguém usando), mas o cursor não sai do lugar.
    public static void Nudge() {
        INPUT[] i = new INPUT[1];
        i[0].type = 0; i[0].flags = 0x0001; // INPUT_MOUSE, MOUSEEVENTF_MOVE
        SendInput(1, i, Marshal.SizeOf(typeof(INPUT)));
    }
    public struct RECT { public int L, T, R, B; }

    public static Rectangle Rect(IntPtr h) {
        RECT r; GetWindowRect(h, out r);
        return new Rectangle(r.L, r.T, r.R - r.L, r.B - r.T);
    }

    /// Janela comum: pede para ela mesma se desenhar (não copia a tela).
    public static Bitmap Print(IntPtr h) {
        Rectangle r = Rect(h);
        Bitmap bmp = new Bitmap(r.Width, r.Height);
        using (Graphics g = Graphics.FromImage(bmp)) {
            IntPtr dc = g.GetHdc();
            PrintWindow(h, dc, 2);
            g.ReleaseHdc(dc);
        }
        return bmp;
    }

    public static Bitmap Screen(Rectangle r) {
        Bitmap bmp = new Bitmap(r.Width, r.Height, PixelFormat.Format32bppArgb);
        using (Graphics g = Graphics.FromImage(bmp)) g.CopyFromScreen(r.Location, Point.Empty, r.Size);
        return bmp;
    }

    /// A mesma área sobre fundo preto e sobre fundo branco → imagem com transparência.
    public static Bitmap Unmatte(Bitmap black, Bitmap white) {
        Bitmap outp = new Bitmap(black.Width, black.Height, PixelFormat.Format32bppArgb);
        for (int y = 0; y < black.Height; y++) {
            for (int x = 0; x < black.Width; x++) {
                Color b = black.GetPixel(x, y), w = white.GetPixel(x, y);
                int diff = ((w.R - b.R) + (w.G - b.G) + (w.B - b.B)) / 3;
                int a = Math.Max(0, Math.Min(255, 255 - diff));
                if (a == 0) { outp.SetPixel(x, y, Color.Transparent); continue; }
                Func<int, int> un = c => Math.Min(255, c * 255 / a);
                outp.SetPixel(x, y, Color.FromArgb(a, un(b.R), un(b.G), un(b.B)));
            }
        }
        return outp;
    }
}
'@

[Smoke]::SetProcessDPIAware() | Out-Null # coordenadas reais, como as do app
$script:failures = 0
function Check($ok, $what) {
    if ($ok) { Write-Host "  ok    $what" -ForegroundColor Green }
    else { Write-Host "  FALHA $what" -ForegroundColor Red; $script:failures++ }
}
function Find($class) { [Smoke]::FindWindowW($class, [IntPtr]::Zero) }
function WaitFor($class, $seconds = 5) {
    $until = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $until) {
        $h = Find $class
        if ($h -ne [IntPtr]::Zero) { return $h }
        Start-Sleep -Milliseconds 200
    }
    [IntPtr]::Zero
}
function Lparam($x, $y) { [IntPtr]($y * 65536 + $x) }

# --- o app numa pasta isolada ---------------------------------------------------------

$script:data = $null
$script:proc = $null
function Start-App($name, [string]$config, [string]$arguments = "") {
    $script:data = Join-Path $Out $name
    if (Test-Path $script:data) { Remove-Item -Recurse -Force $script:data }
    New-Item -ItemType Directory -Force "$script:data\StayAlone" | Out-Null
    if ($config) { [IO.File]::WriteAllText("$script:data\StayAlone\config.ini", $config) }
    Run-Exe $arguments
    Start-Sleep -Seconds 3
}
function Run-Exe([string]$arguments) {
    $psi = New-Object Diagnostics.ProcessStartInfo $Exe
    $psi.UseShellExecute = $false
    $psi.EnvironmentVariables["APPDATA"] = $script:data
    $psi.Arguments = $arguments
    $p = [Diagnostics.Process]::Start($psi)
    if (-not $script:proc) { $script:proc = $p }
}
function Stop-App {
    $m = Find "StayAloneMascot"
    if ($m -ne [IntPtr]::Zero) { [Smoke]::PostMessageW($m, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null } # WM_CLOSE
    if ($script:proc) { $script:proc.WaitForExit(5000) | Out-Null; $script:proc = $null }
    Get-Process | Where-Object { $_.ProcessName -like "dontStayAlone*" } | Stop-Process -Force -ErrorAction SilentlyContinue
}
function Setting($file, $key) {
    $line = Get-Content "$script:data\StayAlone\$file" -ErrorAction SilentlyContinue | Where-Object { $_ -like "$key=*" } | Select-Object -First 1
    if ($line) { $line.Substring($key.Length + 1) } else { $null }
}

# --- fotos ----------------------------------------------------------------------------

function Save($bmp, $name, [switch]$ForDocs) {
    $bmp.Save("$Out\$name.png", [Drawing.Imaging.ImageFormat]::Png)
    if ($ForDocs -and $Docs) { $bmp.Save("$DocsDir\$name.png", [Drawing.Imaging.ImageFormat]::Png) }
}

# Janela transparente sobre um fundo preto e depois branco (uma janela nossa, sem
# ativar, logo atrás dela): a transparência sai das duas fotos.
function Shoot-Layered($hwnd, $name, [switch]$ForDocs, $margin = 24) {
    $r = [Smoke]::Rect($hwnd)
    $area = [Drawing.Rectangle]::Inflate($r, $margin, $margin)
    $area.Intersect([Windows.Forms.SystemInformation]::VirtualScreen) # só o que existe na tela
    $form = New-Object Windows.Forms.Form
    $form.FormBorderStyle = "None"; $form.ShowInTaskbar = $false; $form.StartPosition = "Manual"
    $form.Bounds = $area
    $shots = @{}
    foreach ($color in "Black", "White") {
        $form.BackColor = [Drawing.Color]::$color
        [Smoke]::SetWindowPos($form.Handle, [IntPtr]::Zero, $area.X, $area.Y, $area.Width, $area.Height, 0x0050) | Out-Null # SHOWWINDOW|NOACTIVATE
        $form.Refresh(); [Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 300
        $shots[$color] = [Smoke]::Screen($area)
    }
    $form.Dispose()
    Save ([Smoke]::Unmatte($shots["Black"], $shots["White"])) $name -ForDocs:$ForDocs
}

function Open-Panel {
    $m = Find "StayAloneMascot"
    [Smoke]::PostMessageW($m, 0x0204, [IntPtr]2, (Lparam 20 20)) | Out-Null # WM_RBUTTONDOWN
    [Smoke]::PostMessageW($m, 0x0205, [IntPtr]0, (Lparam 20 20)) | Out-Null # WM_RBUTTONUP
    $f = WaitFor "StayAloneFlyout"
    Start-Sleep -Milliseconds 700 # termina de aparecer
    # O painel abre onde está o seu mouse: tira o destaque do item que ficou embaixo dele.
    [Smoke]::PostMessageW($f, 0x02A3, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null # WM_MOUSELEAVE
    Start-Sleep -Milliseconds 200
    $f
}
function Close-Panel($f) {
    [Smoke]::PostMessageW($f, 0x0100, [IntPtr]0x1B, [IntPtr]::Zero) | Out-Null # Esc
    Start-Sleep -Milliseconds 500
}

# Configurações: abre e fotografa cada página (a barra lateral tem um item a cada 44 px).
function Shoot-Settings($prefix, $pages, [switch]$ForDocs) {
    Run-Exe "--configurar"
    $s = WaitFor "StayAloneSettings"
    Check ($s -ne [IntPtr]::Zero) "Configurações abriu ($prefix)"
    if ($s -eq [IntPtr]::Zero) { return }
    Start-Sleep -Milliseconds 800
    foreach ($page in $pages.GetEnumerator()) {
        # Páginas da navegação a cada 44 px; "Sobre" (6) fica no rodapé da barra lateral.
        $y = if ($page.Key -eq 6) { 656 } else { 191 + 44 * $page.Key }
        [Smoke]::PostMessageW($s, 0x0201, [IntPtr]1, (Lparam 100 $y)) | Out-Null
        [Smoke]::PostMessageW($s, 0x0202, [IntPtr]0, (Lparam 100 $y)) | Out-Null
        Start-Sleep -Milliseconds 700
        Save ([Smoke]::Print($s)) "$prefix$($page.Value)" -ForDocs:$ForDocs
    }
    [Smoke]::PostMessageW($s, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    Start-Sleep -Milliseconds 500
}

$pages = [ordered]@{ 0 = "general"; 1 = "reminders"; 2 = "chat"; 3 = "maker"; 4 = "plugins"; 5 = "gallery"; 6 = "about" }

try {
    Write-Host "1. Primeira vez (boas-vindas)"
    Start-App "primeira-vez" ""
    $w = WaitFor "StayAloneWelcome"
    Check ($w -ne [IntPtr]::Zero) "boas-vindas apareceu"
    Save ([Smoke]::Print($w)) "welcome1"
    [Smoke]::PostMessageW($w, 0x0202, [IntPtr]0, (Lparam 360 190)) | Out-Null # 3º mascote
    Start-Sleep -Seconds 1
    Check ((Setting "config.ini" "mascot") -eq "zeze") "escolher um mascote já troca o da tela"
    foreach ($step in 2, 3) {
        [Smoke]::PostMessageW($w, 0x0111, [IntPtr]11, [IntPtr]::Zero) | Out-Null # Próximo
        Start-Sleep -Milliseconds 600
        Save ([Smoke]::Print($w)) "welcome$step"
    }
    [Smoke]::PostMessageW($w, 0x0111, [IntPtr]11, [IntPtr]::Zero) | Out-Null # Começar!
    Start-Sleep -Seconds 1
    Check ((Find "StayAloneWelcome") -eq [IntPtr]::Zero) "boas-vindas fechou"
    Check ((Setting "config.ini" "water") -eq "on") "lembrete de água ligado"

    Write-Host "2. Água e painel"
    Run-Exe "--agua"; Start-Sleep -Seconds 1
    Run-Exe "--agua"; Start-Sleep -Seconds 1
    $f = Open-Panel
    Check ($f -ne [IntPtr]::Zero) "painel abriu"
    if ($f -ne [IntPtr]::Zero) { Shoot-Layered $f "panel-water"; Close-Panel $f }
    Stop-App
    Check ((Setting "state.ini" "water") -eq "2") "dois copos d'água gravados no state.ini"

    Write-Host "3. Configurações em português e em inglês"
    Start-App "pt" "mascot=calcifer`nlanguage=pt`ntheme=light`nupdates=off`n"
    Shoot-Settings "pt-" $pages
    Stop-App
    Check ((Get-Content "$script:data\StayAlone\config.ini" -Raw) -notmatch "language=en") "português continua português"
    Start-App "en" "mascot=calcifer`nlanguage=en`ntheme=light`nupdates=off`n"
    Shoot-Settings "en-" $pages
    Stop-App

    Write-Host "4. Pausa guiada e atalho (leva uns 2 minutos; não digite enquanto isso)"
    Start-App "pausa" "mascot=calcifer`nlanguage=pt`nupdates=off`nwater=off`nstretch=off`neyes=on`neyes_minutes=1`n"
    $m = Find "StayAloneMascot"
    [Smoke]::PostMessageW($m, 0x0312, [IntPtr]1, [IntPtr]::Zero) | Out-Null # WM_HOTKEY (Ctrl+Alt+M)
    Check ((WaitFor "StayAloneChatInput" 3) -ne [IntPtr]::Zero) "Ctrl+Alt+M abre a conversa"
    $c = Find "StayAloneChatInput"
    if ($c -ne [IntPtr]::Zero) { [Smoke]::PostMessageW($c, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
    # O lembrete de olhos (1 min de uso) aparece; um clique no balão começa a pausa guiada.
    # O primeiro balão pode ser o "boa tarde" da abertura: o lembrete só vem depois de
    # 1 minuto de uso, então a busca começa perto disso.
    $bubble = [IntPtr]::Zero
    $start = Get-Date
    while (((Get-Date) - $start).TotalSeconds -lt 50) { [Smoke]::Nudge(); Start-Sleep -Seconds 2 }
    $until = (Get-Date).AddSeconds(100)
    while ((Get-Date) -lt $until) {
        [Smoke]::Nudge()
        $bubble = Find "StayAloneBubble"
        if ($bubble -ne [IntPtr]::Zero -and [Smoke]::IsWindowVisible($bubble)) { break }
        Start-Sleep -Seconds 2
    }
    Check ($bubble -ne [IntPtr]::Zero -and [Smoke]::IsWindowVisible($bubble)) "lembrete de olhos apareceu"
    if ($bubble -ne [IntPtr]::Zero) {
        Shoot-Layered $bubble "reminder-eyes"
        [Smoke]::PostMessageW($m, 0x8002, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null # clique no balão
        Start-Sleep -Seconds 3
        Check ([Smoke]::IsWindowVisible($bubble)) "pausa guiada começou"
        Shoot-Layered $bubble "guide-eyes"
        Start-Sleep -Seconds 24
        Shoot-Layered $bubble "guide-done"
    }
    Stop-App

    if ($Docs) {
        Write-Host "4. Imagens do README (docs/)"
        $today = Get-Date
        Start-App "docs-light" "mascot=calcifer`nbuddy=jujubs`nlanguage=en`ntheme=light`nupdates=off`n"
        Run-Exe "--agua"; Start-Sleep -Seconds 1
        Run-Exe "--agua"; Start-Sleep -Seconds 1
        Run-Exe "--agua"; Start-Sleep -Seconds 1
        $f = Open-Panel; Shoot-Layered $f "panel" -ForDocs; Close-Panel $f
        Shoot-Settings "settings-" ([ordered]@{ 0 = "general"; 1 = "reminders"; 3 = "maker" }) -ForDocs
        Stop-App
        Start-App "docs-dark" "mascot=jujubs`nlanguage=en`ntheme=dark`nupdates=off`n"
        $f = Open-Panel; Shoot-Layered $f "panel-dark" -ForDocs; Close-Panel $f
        Shoot-Settings "settings-dark-" ([ordered]@{ 2 = "chat" }) -ForDocs
        Stop-App
        # Aniversário hoje: chapéu de festa.
        Start-App "docs-hat" ("mascot=calcifer`nlanguage=en`nupdates=off`nbirthday={0:dd}/{0:MM}`n" -f $today)
        Start-Sleep -Seconds 2
        Shoot-Layered (Find "StayAloneMascot") "mascot-hat" -ForDocs -margin 4
        Stop-App
    }
}
finally {
    Stop-App
}

Write-Host ""
Write-Host "Fotos em $Out"
if ($script:failures -gt 0) { Write-Host "$script:failures falha(s)" -ForegroundColor Red; exit 1 }
Write-Host "Tudo certo." -ForegroundColor Green
