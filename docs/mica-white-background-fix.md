# Mica White Background Fix

## Problem

The settings window uses a transparent WebView so Windows 11 Mica can show through the UI.
On screenshots and on non-primary monitors, a white rectangular area could appear behind the UI.
The rectangle was not CSS, not the settings layout, and not the WebView content itself.

The smoking gun was that Mica still worked outside the white rectangle. That meant the parent
window/backdrop was alive, but part of the window composition path was using a stale white backing
surface.

## Root Cause

Dioxus passes `WindowBuilder::with_transparent(true)` down to Tao.

On Windows, Tao handles transparent top-level windows by enabling a legacy DWM blur-behind path with
an empty blur region. That path can create or retain a DWM redirection bitmap/backing surface. When
the settings window is moved across monitors, especially across DPI/display boundaries, that backing
surface can get stuck at the wrong size or state and show up as a white rectangle.

The WebView still needs transparency so Mica can show through. The broken part is the top-level
window redirection bitmap/legacy blur path, not Mica itself.

## Correct Fix

Keep the settings window transparent, but disable the DWM redirection bitmap:

```rust
use dioxus::desktop::tao::platform::windows::WindowBuilderExtWindows;

WindowBuilder::new()
    .with_transparent(true)
    .with_no_redirection_bitmap(true)
```

In this project the fix lives in `src/main.rs` on the settings window builder.

`with_no_redirection_bitmap(true)` maps to `WS_EX_NOREDIRECTIONBITMAP`. In Tao, it also prevents the
legacy `DwmEnableBlurBehindWindow` transparent-region hack from being applied. This preserves the
transparent WebView path needed for Mica while removing the stale white backing surface.

## Things That Did Not Fix It

- CSS fallback backgrounds only hid the bug. They did not remove the white composition surface.
- Reapplying `DWMWA_SYSTEMBACKDROP_TYPE` helped keep Mica state fresh, but did not remove the white rectangle.
- Setting child WebView HWND bounds to `0x0` made the app UI disappear, but the white rectangle stayed.
  That proved the rectangle was not the child WebView content surface.
- `DwmExtendFrameIntoClientArea` did not help and introduced a thin colored top artifact.
- Setting `with_transparent(false)` removed the Mica transparency path and made the window white.

## Verification

After applying `with_no_redirection_bitmap(true)`, the settings window rendered correctly:

- on the primary monitor;
- after moving to a secondary monitor;
- in screenshots;
- with Mica still visible through the transparent UI.

If this bug comes back, inspect the top-level HWND composition path before changing CSS. The white
rectangle is a native Windows/DWM backing-surface issue, not a layout color.
