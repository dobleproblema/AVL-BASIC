<#
.SYNOPSIS
Compara Smoke con las variantes históricas de Ink Reactor.

.DESCRIPTION
Prepara copias con la misma anchura de cuadrícula y
etiquetas ORIGINAL, OPTIMIZED y LITE en target/ink-reactor-compare.
Cada versión conserva sus valores de presión, suavizado y demás parámetros.
Requiere restaurar las muestras históricas original y Lite en samples: ya no
forman parte del checkout actual. Cotejar sus hashes con el informe
tools/benchmarks/INK-REACTOR-LITE-2026-10-03.md. El lanzador no restaura archivos.
OPTIMIZED usa la Smoke actual, con cambios posteriores a las mediciones históricas.

Las ventanas se distribuyen horizontalmente en la pantalla principal sin
cambiar su tamaño. Si no caben, se solapan: pueden moverse a otro monitor o
reorganizarse con Windows Snap. ESC cierra cada simulación por separado.

Las FPS con tres procesos simultáneos no son una medida aislada de rendimiento.
Para medirlo, utiliza tools/benchmarks/ink_reactor.py con ejecuciones separadas.

.PARAMETER Grid
Anchura de la cuadrícula de tinta: 40, 64, 80, 96 o 128. Por defecto, 96 x 60.

.PARAMETER Executable
Ruta del intérprete. Por defecto, target/release/avl-basic.exe del repositorio.

.EXAMPLE
.\tools\compare_ink_reactor.ps1 -Grid 128
#>
[CmdletBinding()]
param(
    [ValidateSet(40, 64, 80, 96, 128)]
    [int] $Grid = 96,
    [string] $Executable
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw 'Este lanzador necesita Windows.'
}

$repository = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($Executable)) {
    $Executable = Join-Path $repository 'target/release/avl-basic.exe'
}
$executablePath = (Resolve-Path -LiteralPath $Executable).ProviderPath
if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) {
    throw "No se encuentra el intérprete: $executablePath"
}

$variants = @(
    @{ Label = 'ORIGINAL'; File = 'g-ink-reactor-original.bas' },
    @{ Label = 'OPTIMIZED'; File = 'g-smoke.bas' },
    @{ Label = 'LITE'; File = 'g-ink-reactor-lite.bas' }
)
$missingSources = @($variants | Where-Object {
    -not (Test-Path -LiteralPath (Join-Path $repository ('samples/' + $_.File)) -PathType Leaf)
} | ForEach-Object { 'samples/' + $_.File })
if ($missingSources.Count -gt 0) {
    throw ('Faltan fuentes para esta comparación histórica: ' + ($missingSources -join ', ') +
        '. Restaura las variantes original y Lite y coteja sus hashes con ' +
        'tools/benchmarks/INK-REACTOR-LITE-2026-10-03.md antes de ejecutarla. ' +
        'El lanzador no restaura archivos ni ha iniciado intérpretes.')
}
$levels = @{ 40 = 0; 64 = 1; 80 = 2; 96 = 3; 128 = 4 }
$level = $levels[$Grid]
$gridHeight = [int]($Grid * 5 / 8)

# Validate and adapt every source before starting any interpreter.
foreach ($variant in $variants) {
    $sourcePath = Join-Path $repository ('samples/' + $variant.File)
    $source = [IO.File]::ReadAllText($sourcePath)
    $settingsPattern = [regex]'(?m)^200\s+[^\r\n]*'
    $settings = $settingsPattern.Matches($source)
    if ($settings.Count -ne 1) {
        throw "Se esperaba una línea 200 de configuración en $sourcePath"
    }
    $levelPattern = [regex]'\bLEVEL\s*=\s*[^:]+'
    if ($levelPattern.Matches($settings[0].Value).Count -ne 1) {
        throw "Se esperaba un único LEVEL en la línea 200 de $sourcePath"
    }
    $newSettings = $levelPattern.Replace($settings[0].Value, "LEVEL=$level ")
    $source = $source.Replace($settings[0].Value, $newSettings)

    $titlePattern = [regex]'(?m)^((?:5520|5550)\s+[^\r\n]*?GPRINT\s+")[^"]*(")'
    if ($titlePattern.Matches($source).Count -ne 1) {
        throw "Se esperaba un único título GPRINT de la demo en $sourcePath"
    }
    $replacement = '${1}SMOKE ' + $variant.Label + '  ${2}'
    $variant.Source = $titlePattern.Replace($source, $replacement)
}

if (-not ('InkReactorCompare.Windows' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

namespace InkReactorCompare {
    public static class Windows {
        public delegate bool EnumWindowsCallback(IntPtr window, IntPtr parameter);

        [StructLayout(LayoutKind.Sequential)]
        public struct Rect { public int Left, Top, Right, Bottom; }

        [DllImport("user32.dll")]
        private static extern bool EnumWindows(EnumWindowsCallback callback, IntPtr parameter);
        [DllImport("user32.dll")]
        private static extern bool IsWindowVisible(IntPtr window);
        [DllImport("user32.dll")]
        private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
        [DllImport("user32.dll")]
        public static extern bool GetWindowRect(IntPtr window, out Rect rectangle);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        public static extern bool SetWindowText(IntPtr window, string text);
        [DllImport("user32.dll")]
        public static extern bool SetWindowPos(IntPtr window, IntPtr insertAfter,
            int x, int y, int width, int height, uint flags);
        [DllImport("user32.dll")]
        public static extern bool SystemParametersInfo(uint action, uint parameter,
            out Rect rectangle, uint flags);

        public static IntPtr FindVisibleWindow(int wantedProcessId) {
            IntPtr result = IntPtr.Zero;
            EnumWindows(delegate(IntPtr window, IntPtr parameter) {
                uint processId;
                GetWindowThreadProcessId(window, out processId);
                Rect rectangle;
                if (processId == wantedProcessId && IsWindowVisible(window) &&
                    GetWindowRect(window, out rectangle) &&
                    rectangle.Right - rectangle.Left > 100 &&
                    rectangle.Bottom - rectangle.Top > 100) {
                    result = window;
                    return false;
                }
                return true;
            }, IntPtr.Zero);
            return result;
        }
    }
}
'@
}

$runName = (Get-Date -Format 'yyyyMMdd-HHmmss-fff') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 8)
$runDirectory = Join-Path $repository ('target/ink-reactor-compare/' + $runName)
$null = New-Item -ItemType Directory -Path $runDirectory
$utf8 = New-Object System.Text.UTF8Encoding($false)
foreach ($variant in $variants) {
    $variant.ProgramPath = Join-Path $runDirectory $variant.File
    [IO.File]::WriteAllText($variant.ProgramPath, $variant.Source, $utf8)
}

$started = New-Object 'System.Collections.Generic.List[System.Diagnostics.Process]'
$previousWindowSetting = [Environment]::GetEnvironmentVariable('AVL_BASIC_WINDOW', 'Process')
try {
    # Child processes inherit this value, even when the caller was in headless mode.
    [Environment]::SetEnvironmentVariable('AVL_BASIC_WINDOW', '1', 'Process')
    foreach ($variant in $variants) {
        # Quote the single file argument; Start-Process joins ArgumentList itself.
        $quotedProgram = '"' + $variant.ProgramPath + '"'
        $process = Start-Process -FilePath $executablePath -ArgumentList $quotedProgram `
            -WorkingDirectory $runDirectory -WindowStyle Hidden -PassThru
        $started.Add($process)
        $variant.Process = $process
        $variant.Window = [IntPtr]::Zero
    }

    $timer = [Diagnostics.Stopwatch]::StartNew()
    do {
        $pending = $false
        foreach ($variant in $variants) {
            $variant.Process.Refresh()
            if ($variant.Process.HasExited) {
                throw "$($variant.Label) terminó antes de completar el arranque (código $($variant.Process.ExitCode))."
            }
            if ($variant.Window -eq [IntPtr]::Zero) {
                $variant.Window = [InkReactorCompare.Windows]::FindVisibleWindow($variant.Process.Id)
            }
            if ($variant.Window -eq [IntPtr]::Zero) { $pending = $true }
        }
        if (-not $pending) { break }
        if ($timer.Elapsed.TotalSeconds -ge 30) {
            throw 'No aparecieron las tres ventanas gráficas en 30 segundos.'
        }
        Start-Sleep -Milliseconds 150
    } while ($true)
}
catch {
    # Only processes created by this invocation belong to this cleanup.
    foreach ($process in $started) {
        try {
            if (-not $process.HasExited) { $process.Kill() }
        }
        catch { Write-Warning "No se pudo cerrar el proceso propio $($process.Id): $_" }
    }
    throw
}
finally {
    [Environment]::SetEnvironmentVariable('AVL_BASIC_WINDOW', $previousWindowSetting, 'Process')
}

# Best-effort arrangement: retain the actual window and simulation sizes.
$workArea = New-Object InkReactorCompare.Windows+Rect
if ([InkReactorCompare.Windows]::SystemParametersInfo(48, 0, [ref] $workArea, 0)) {
    $availableWidth = $workArea.Right - $workArea.Left
    $totalWidth = 0
    foreach ($variant in $variants) {
        $rectangle = New-Object InkReactorCompare.Windows+Rect
        if ([InkReactorCompare.Windows]::GetWindowRect($variant.Window, [ref] $rectangle)) {
            $variant.Width = $rectangle.Right - $rectangle.Left
            $totalWidth += $variant.Width
        }
        else { $variant.Width = 660; $totalWidth += 660 }
    }
    $left = $workArea.Left
    for ($index = 0; $index -lt $variants.Count; $index++) {
        $variant = $variants[$index]
        if ($totalWidth -gt $availableWidth) {
            $left = $workArea.Left + [int]([Math]::Max(0, $availableWidth - $variant.Width) * $index / ($variants.Count - 1))
        }
        # NOSIZE | NOZORDER | NOACTIVATE: do not resize or steal keyboard focus.
        $null = [InkReactorCompare.Windows]::SetWindowPos($variant.Window, [IntPtr]::Zero,
            $left, $workArea.Top, 0, 0, 0x0015)
        $left += $variant.Width
    }
    if ($totalWidth -gt $availableWidth) {
        Write-Warning 'Las tres ventanas no caben en una fila sin solaparse. Puedes moverlas a otro monitor o usar Windows Snap.'
    }
}
foreach ($variant in $variants) {
    $null = [InkReactorCompare.Windows]::SetWindowText($variant.Window,
        "SMOKE - $($variant.Label) ($Grid x $gridHeight)")
}

$manifest = [ordered]@{
    createdUtc = [DateTime]::UtcNow.ToString('o')
    executable = $executablePath
    grid = @($Grid, $gridHeight)
    processes = @($variants | ForEach-Object {
        [ordered]@{
            label = $_.Label
            processId = $_.Process.Id
            program = $_.ProgramPath
            windowHandle = $_.Window.ToInt64()
            title = "SMOKE - $($_.Label) ($Grid x $gridHeight)"
        }
    })
}
try {
    [IO.File]::WriteAllText((Join-Path $runDirectory 'manifest.json'),
        ($manifest | ConvertTo-Json -Depth 4), $utf8)
}
catch { Write-Warning "Las ventanas están abiertas, pero no se pudo guardar manifest.json: $_" }

Write-Host "Abiertas ORIGINAL, OPTIMIZED y LITE a $Grid x $gridHeight. ESC cierra cada ventana."
Write-Host 'Las FPS simultáneas comparten procesador; para medir rendimiento, usa tools/benchmarks/ink_reactor.py.'
Write-Host "Copias usadas: $runDirectory"
