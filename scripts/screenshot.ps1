<#
.SYNOPSIS
Captures the settings window ("silence!") as composited on screen.

.EXAMPLE
./scripts/screenshot.ps1 -Out shots/hotkeys.png
./scripts/screenshot.ps1 -Out shots/overlay.png -ClickX 120 -ClickY 219 -WaitMs 700
#>
param(
    [Parameter(Mandatory = $true)][string]$Out,
    # Render through PrintWindow instead of copying the screen. Works while the
    # session is locked, but Mica and other DWM backdrops are not included.
    [switch]$PrintWindow,
    # Deliver the click as posted window messages; works while the session is locked.
    [switch]$PostClick,
    [string]$Title = "silence!",
    [double]$ClickX = -1,
    [double]$ClickY = -1,
    [int]$Wheel = 0,
    [int]$WaitMs = 400
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class ShotNative {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr hwnd, int attr, out RECT rect, int size);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, int data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr value);
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowExW(IntPtr parent, IntPtr after, string cls, string title);
    // WebView2 receives input on its render widget.
    public static IntPtr FindRenderWidget(IntPtr root) {
        IntPtr found = IntPtr.Zero;
        foreach (var cls in new[] { "WRY_WEBVIEW", "Chrome_WidgetWin_0", "Chrome_WidgetWin_1", "Chrome_RenderWidgetHostHWND" }) {
            IntPtr next = FindWindowExW(found == IntPtr.Zero ? root : found, IntPtr.Zero, cls, null);
            if (next == IntPtr.Zero) return IntPtr.Zero;
            found = next;
        }
        return found;
    }
}
"@
# Per-monitor v2: window rectangles and capture coordinates are physical pixels.
[void][ShotNative]::SetProcessDpiAwarenessContext([IntPtr]-4)

$hwnd = [ShotNative]::FindWindowW([NullString]::Value, $Title)
if ($hwnd -eq [IntPtr]::Zero) { throw "Window '$Title' not found" }
# An Alt tap lets this process move the window to the foreground.
[ShotNative]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
[ShotNative]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
[void][ShotNative]::SetForegroundWindow($hwnd)
Start-Sleep -Milliseconds 150

$rect = New-Object ShotNative+RECT
# DWMWA_EXTENDED_FRAME_BOUNDS excludes the invisible resize border.
[void][ShotNative]::DwmGetWindowAttribute($hwnd, 9, [ref]$rect, 16)
$scale = [ShotNative]::GetDpiForWindow($hwnd) / 96.0

if ($ClickX -ge 0 -and $PostClick) {
    $target = [ShotNative]::FindRenderWidget($hwnd)
    # GPUI takes input on the top-level window; both use physical client pixels.
    if ($target -eq [IntPtr]::Zero) { $target = $hwnd }
    $x = [int]($ClickX * $scale); $y = [int]($ClickY * $scale)
    $lparam = [IntPtr](($y -shl 16) -bor ($x -band 0xffff))
    [void][ShotNative]::PostMessageW($target, 0x0200, [IntPtr]0, $lparam)
    [void][ShotNative]::PostMessageW($target, 0x0201, [IntPtr]1, $lparam)
    Start-Sleep -Milliseconds 40
    [void][ShotNative]::PostMessageW($target, 0x0202, [IntPtr]0, $lparam)
    [void][ShotNative]::PostMessageW($target, 0x02A3, [IntPtr]0, [IntPtr]0)
} elseif ($ClickX -ge 0 -and [ShotNative]::GetForegroundWindow() -eq $hwnd) {
    [void][ShotNative]::SetCursorPos([int]($rect.Left + $ClickX * $scale), [int]($rect.Top + $ClickY * $scale))
    Start-Sleep -Milliseconds 40
    if ($Wheel -ne 0) {
        [ShotNative]::mouse_event(0x0800, 0, 0, $Wheel, [UIntPtr]::Zero)
    } else {
        [ShotNative]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [ShotNative]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    }
    # Park the cursor outside the window so hover states do not leak into shots.
    [void][ShotNative]::SetCursorPos($rect.Right + 40, $rect.Top + 40)
}
Start-Sleep -Milliseconds $WaitMs

if ($PrintWindow) {
    $outer = New-Object ShotNative+RECT
    [void][ShotNative]::GetWindowRect($hwnd, [ref]$outer)
    $full = New-Object System.Drawing.Bitmap ($outer.Right - $outer.Left), ($outer.Bottom - $outer.Top)
    $fullGraphics = [System.Drawing.Graphics]::FromImage($full)
    $hdc = $fullGraphics.GetHdc()
    # PW_RENDERFULLCONTENT captures DirectComposition/WebView content.
    [void][ShotNative]::PrintWindow($hwnd, $hdc, 2)
    $fullGraphics.ReleaseHdc($hdc)
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    $bitmap = $full.Clone((New-Object System.Drawing.Rectangle ($rect.Left - $outer.Left), ($rect.Top - $outer.Top), $width, $height), $full.PixelFormat)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $fullGraphics.Dispose()
    $full.Dispose()
} else {
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    $bitmap = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
}
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
$bitmap.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
Write-Host "$Out ${width}x${height} scale=$scale"
