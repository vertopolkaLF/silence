<#
.SYNOPSIS
Measures silence! resource usage for one build across fixed scenarios.

.DESCRIPTION
Each scenario starts a fresh tray process from -Exe, optionally opens the
settings window, then samples once per second for -Duration seconds.
The process tree includes every descendant of silence.exe (for the Dioxus
build this includes WebView2 browser/renderer/GPU processes).

The user's config.json is backed up before each scenario and restored after.

Scenarios:
  tray-idle        Tray only, overlay disabled, settings closed.
  overlay-visible  Tray only, overlay forced "Always" visible.
  settings-idle    Tray + settings window open, no interaction.
  settings-load    Tray + settings window, scripted clicks through every
                   sidebar tab plus wheel scrolling in the content area.

.EXAMPLE
./scripts/measure.ps1 -Exe target/release/silence.exe -Label after -OutDir measurements
#>
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Label,
    [string]$OutDir = "measurements",
    [int]$Duration = 60,
    [string[]]$Scenarios = @("tray-idle", "overlay-visible", "settings-idle", "settings-load"),
    [string]$SettingsArg = "--settings"
)

$ErrorActionPreference = "Stop"
$Exe = (Resolve-Path $Exe).Path
$configPath = Join-Path $env:APPDATA "SilenceV2\config.json"
$backupPath = "$configPath.measure-backup"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class MeasureNative {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, int data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowExW(IntPtr parent, IntPtr after, string cls, string title);
    // WebView2 receives input on its render widget; GPUI on the top-level window.
    public static IntPtr InputTarget(IntPtr root) {
        IntPtr found = root;
        foreach (var cls in new[] { "WRY_WEBVIEW", "Chrome_WidgetWin_0", "Chrome_WidgetWin_1", "Chrome_RenderWidgetHostHWND" }) {
            IntPtr next = FindWindowExW(found, IntPtr.Zero, cls, null);
            if (next == IntPtr.Zero) return root;
            found = next;
        }
        return found;
    }
}
"@
[void][MeasureNative]::SetProcessDpiAwarenessContext([IntPtr]-4)

function Stop-Silence {
    Get-Process silence -ErrorAction SilentlyContinue | Stop-Process -Force
    for ($i = 0; $i -lt 50 -and (Get-Process silence -ErrorAction SilentlyContinue); $i++) {
        Start-Sleep -Milliseconds 100
    }
    Start-Sleep -Milliseconds 500
}

function Set-ScenarioConfig([string]$scenario) {
    $config = Get-Content $configPath -Raw | ConvertFrom-Json
    switch ($scenario) {
        "overlay-visible" { $config.overlay.enabled = $true; $config.overlay.visibility = "Always" }
        default { $config.overlay.enabled = $false }
    }
    $config.welcome_completed = $true
    $config | ConvertTo-Json -Depth 32 | Set-Content -Encoding utf8 $configPath
}

function Get-TreeIds([int[]]$roots) {
    # Parent ids are not cleared when a parent exits, and Windows reuses ids.
    # A child must have started after its parent to belong to the tree.
    $all = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, CreationDate
    $created = @{}
    foreach ($p in $all) { $created[[int]$p.ProcessId] = $p.CreationDate }
    $ids = [System.Collections.Generic.HashSet[int]]::new()
    foreach ($root in $roots) { if ($created.ContainsKey($root)) { [void]$ids.Add($root) } }
    do {
        $added = $false
        foreach ($p in $all) {
            $parent = [int]$p.ParentProcessId
            if ($ids.Contains($parent) -and $p.CreationDate -ge $created[$parent] -and $ids.Add([int]$p.ProcessId)) {
                $added = $true
            }
        }
    } while ($added)
    return @($ids)
}

function Get-Sample([int[]]$roots) {
    $ids = Get-TreeIds $roots
    $perf = Get-CimInstance Win32_PerfRawData_PerfProc_Process -Property IDProcess, WorkingSetPrivate |
        Where-Object { $ids -contains [int]$_.IDProcess }
    $privateWs = ($perf | Measure-Object WorkingSetPrivate -Sum).Sum
    $procs = foreach ($id in $ids) { Get-Process -Id $id -ErrorAction SilentlyContinue }
    [pscustomobject]@{
        Processes = @($procs).Count
        Threads = ($procs | ForEach-Object { $_.Threads.Count } | Measure-Object -Sum).Sum
        PrivateBytes = ($procs | Measure-Object PrivateMemorySize64 -Sum).Sum
        WorkingSet = ($procs | Measure-Object WorkingSet64 -Sum).Sum
        PrivateWorkingSet = $privateWs
        CpuSeconds = ($procs | ForEach-Object { $_.TotalProcessorTime.TotalSeconds } | Measure-Object -Sum).Sum
    }
}

# Time from process start until the window shows rendered content: the window
# is visible and a PrintWindow capture is no longer a uniform fill. This is
# comparable between WebView2 (which shows the window before the page paints)
# and GPUI.
function Wait-SettingsWindow([int]$processId, [Diagnostics.Stopwatch]$watch, [double]$timeoutSec) {
    while ($watch.Elapsed.TotalSeconds -lt $timeoutSec) {
        $hwnd = [MeasureNative]::FindWindowW([NullString]::Value, "silence!")
        $owner = 0
        if ($hwnd -ne [IntPtr]::Zero) { [void][MeasureNative]::GetWindowThreadProcessId($hwnd, [ref]$owner) }
        if ($owner -eq $processId -and [MeasureNative]::IsWindowVisible($hwnd) -and (Test-Painted $hwnd)) {
            return @{ Hwnd = $hwnd; Ms = $watch.Elapsed.TotalMilliseconds }
        }
        Start-Sleep -Milliseconds 5
    }
    throw "Settings window did not paint within $timeoutSec s"
}

function Test-Painted($hwnd) {
    $rect = New-Object MeasureNative+RECT
    [void][MeasureNative]::GetWindowRect($hwnd, [ref]$rect)
    $width = $rect.Right - $rect.Left; $height = $rect.Bottom - $rect.Top
    if ($width -le 0 -or $height -le 0) { return $false }
    $bitmap = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $hdc = $graphics.GetHdc()
    [void][MeasureNative]::PrintWindow($hwnd, $hdc, 2)
    $graphics.ReleaseHdc($hdc)
    $colors = [System.Collections.Generic.HashSet[int]]::new()
    for ($y = 1; $y -lt 16; $y++) {
        for ($x = 1; $x -lt 16; $x++) {
            [void]$colors.Add($bitmap.GetPixel([int]($width * $x / 16), [int]($height * $y / 16)).ToArgb())
        }
    }
    $graphics.Dispose(); $bitmap.Dispose()
    return $colors.Count -ge 6
}

# Input is posted as window messages so the load scenario works while the
# session is locked and can never reach another application's window.
function Click-Logical($hwnd, [double]$x, [double]$y) {
    $scale = [MeasureNative]::GetDpiForWindow($hwnd) / 96.0
    $target = [MeasureNative]::InputTarget($hwnd)
    $lparam = [IntPtr]((([int]($y * $scale)) -shl 16) -bor ([int]($x * $scale) -band 0xffff))
    [void][MeasureNative]::PostMessageW($target, 0x0200, [IntPtr]0, $lparam)
    [void][MeasureNative]::PostMessageW($target, 0x0201, [IntPtr]1, $lparam)
    Start-Sleep -Milliseconds 30
    [void][MeasureNative]::PostMessageW($target, 0x0202, [IntPtr]0, $lparam)
}

function Wheel-Logical($hwnd, [double]$x, [double]$y, [int]$delta) {
    $rect = New-Object MeasureNative+RECT
    [void][MeasureNative]::GetWindowRect($hwnd, [ref]$rect)
    $scale = [MeasureNative]::GetDpiForWindow($hwnd) / 96.0
    $target = [MeasureNative]::InputTarget($hwnd)
    $sx = [int]($rect.Left + $x * $scale); $sy = [int]($rect.Top + $y * $scale)
    $wparam = [IntPtr](($delta -band 0xffff) -shl 16)
    $lparam = [IntPtr](($sy -shl 16) -bor ($sx -band 0xffff))
    [void][MeasureNative]::PostMessageW($target, 0x020A, $wparam, $lparam)
}

# Logical (96 DPI) window coordinates of the nine sidebar tabs, bottom to top.
# Tabs are 42 px tall with a 4 px gap starting at y = 60. An active tab's
# sub-section list only pushes the tabs below it, so clicking upward keeps
# every target at its resting position. Identical in the Dioxus and GPUI builds.
$sidebarX = 120
$sidebarTabY = 8..0 | ForEach-Object { 81 + 46 * $_ }
$contentPoint = @(500, 300)

$results = @()
Copy-Item $configPath $backupPath -Force
try {
    foreach ($scenario in $Scenarios) {
        Write-Host "== $Label / $scenario"
        Stop-Silence
        Set-ScenarioConfig $scenario

        $tray = Start-Process -FilePath $Exe -PassThru
        Start-Sleep -Seconds 3
        $roots = @($tray.Id)
        $startupMs = $null

        if ($scenario -like "settings-*") {
            $launch = [Diagnostics.Stopwatch]::StartNew()
            $settings = Start-Process -FilePath $Exe -ArgumentList $SettingsArg -PassThru
            $window = Wait-SettingsWindow $settings.Id $launch 20
            $startupMs = [math]::Round($window.Ms, 1)
            $roots += $settings.Id
            Start-Sleep -Seconds 3
        }

        $samples = @()
        $previous = Get-Sample $roots
        $loadStep = 0
        for ($second = 0; $second -lt $Duration; $second++) {
            $tick = [Diagnostics.Stopwatch]::StartNew()
            if ($scenario -eq "settings-load") {
                $hwnd = [MeasureNative]::FindWindowW([NullString]::Value, "silence!")
                $tab = $sidebarTabY[$loadStep % $sidebarTabY.Count]
                Click-Logical $hwnd $sidebarX $tab
                Start-Sleep -Milliseconds 400
                for ($w = 0; $w -lt 4; $w++) {
                    Wheel-Logical $hwnd $contentPoint[0] $contentPoint[1] (-120)
                    Start-Sleep -Milliseconds 60
                }
                for ($w = 0; $w -lt 4; $w++) {
                    Wheel-Logical $hwnd $contentPoint[0] $contentPoint[1] 120
                    Start-Sleep -Milliseconds 60
                }
                $loadStep++
            }
            $remaining = 1000 - $tick.ElapsedMilliseconds
            if ($remaining -gt 0) { Start-Sleep -Milliseconds $remaining }
            $current = Get-Sample $roots
            $elapsed = $tick.Elapsed.TotalSeconds
            $samples += [pscustomobject]@{
                Second = $second
                Processes = $current.Processes
                Threads = $current.Threads
                PrivateMB = [math]::Round($current.PrivateBytes / 1MB, 2)
                WorkingSetMB = [math]::Round($current.WorkingSet / 1MB, 2)
                PrivateWorkingSetMB = [math]::Round($current.PrivateWorkingSet / 1MB, 2)
                CpuPercent = [math]::Round(100 * ($current.CpuSeconds - $previous.CpuSeconds) / $elapsed / [Environment]::ProcessorCount, 3)
            }
            $previous = $current
        }

        $samples | Export-Csv -NoTypeInformation (Join-Path $OutDir "$Label-$scenario.csv")
        $stats = { param($name) $values = $samples | ForEach-Object { $_.$name } | Sort-Object
            [pscustomobject]@{ Median = $values[[int]($values.Count / 2)]; Max = ($values | Measure-Object -Maximum).Maximum } }
        $cpu = $samples | Measure-Object CpuPercent -Average -Maximum
        $results += [pscustomobject]@{
            Label = $Label
            Scenario = $scenario
            StartupToVisibleMs = $startupMs
            Processes = ($samples | Measure-Object Processes -Maximum).Maximum
            ThreadsMedian = (& $stats Threads).Median
            PrivateWorkingSetMB_Median = (& $stats PrivateWorkingSetMB).Median
            PrivateWorkingSetMB_Max = (& $stats PrivateWorkingSetMB).Max
            PrivateMB_Median = (& $stats PrivateMB).Median
            WorkingSetMB_Median = (& $stats WorkingSetMB).Median
            CpuPercent_Avg = [math]::Round($cpu.Average, 3)
            CpuPercent_Max = [math]::Round($cpu.Maximum, 3)
        }
        Stop-Silence
    }
}
finally {
    Stop-Silence
    Copy-Item $backupPath $configPath -Force
    Remove-Item $backupPath -Force
}

$exeItem = Get-Item $Exe
$results | Add-Member -NotePropertyName ExeMB -NotePropertyValue ([math]::Round($exeItem.Length / 1MB, 2)) -PassThru | Out-Null
$results | Format-Table -AutoSize | Out-String -Width 300 | Write-Host
$results | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 (Join-Path $OutDir "$Label-summary.json")
