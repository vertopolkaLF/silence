# GPUI overlay

The desktop overlay uses GPUI's Windows renderer. The settings window remains
Dioxus; audio, hotkeys, the tray, config persistence, and visibility rules retain
their existing message loop.

## Dependency choice

`gpui` and `gpui_windows` come from the same pinned Zed revision
`a84689073d296dfd39987bc7dd478e43ef76d83a`.
The published `gpui 0.2.2` requires `cocoa =0.26.0`, while Dioxus requires
`cocoa ^0.26.1`. Cargo resolves those constraints even for Windows targets.
Using the upstream core with its Windows backend avoids the conflict without
patching Dioxus or vendoring either framework. Do not replace `gpui_windows`
with the all-platform `gpui_platform` loader: that reintroduces `gpui_macos`
and its conflicting dependency.

Keep GPUI's `windows-manifest` feature enabled. Its resource selects Common
Controls v6, required by GPUI's statically imported `TaskDialogIndirect`.
Without this manifest the executable can build successfully but Windows loads
the legacy `comctl32.dll` and aborts before `main` with `0xC0000139`.

## Ownership

- `src/gpui_overlay/mod.rs` owns the GPUI application, embedded SVG assets,
  DirectWrite text measurement, appearance, and frame-driven transitions.
- `src/gpui_overlay/motion.rs` retargets from the current interpolated value.
  Repeated state updates do not restart an animation.
- `src/native_overlay.rs` owns Windows placement, DPI/monitor selection,
  non-activating/topmost behavior, click-through policy, and drag/click handling.

A dedicated Windows UI thread runs GPUI, with OLE initialization provided by
its Windows platform. Settings/state updates wake an async channel receiver;
there is no renderer polling loop. Updates in a burst are coalesced to the latest
snapshot. The inactive frame throttle is disabled because an overlay should
animate smoothly without taking focus.

The GPU surface follows the animated card size and keeps its percentage anchor
stable. There is no extra window backdrop or shadow gutter.
Native surface/show operations are posted to the GPUI thread so they do not
re-enter its renderer during a frame.

The native window explicitly uses `WS_POPUP`, removes caption/frame styles,
and disables DWM non-client decoration. `WM_NCCALCSIZE` leaves the entire
surface to GPUI. Pass-through mode uses `WS_EX_LAYERED | WS_EX_TRANSPARENT`
with native alpha 255 so clicks reach other applications; GPUI still owns
visual opacity. Button mode and position editing remove those input flags
immediately to retain their existing actions and dragging.

Overlay actions return to the existing background message loop through
`WM_OVERLAY_ACTION`. The renderer does not perform audio operations on its
own thread. The forced mute-failure warning continues to use the same visibility
path and embedded Solar warning icon.

## Bundled fonts

Before opening the overlay window, GPUI registers TTF bytes embedded in the EXE
with `include_bytes!`. Preset themes no longer inspect installed font families:

| Overlay | Bundled family |
| --- | --- |
| Custom default, Windows, mute-failure warning | Inter |
| Material You | Google Sans |
| Cute | Nunito |
| Cute Sticker | Nunito (outlined SVG lettering) |
| Neon | Orbitron |
| Brutalism | Archivo Black |

The Custom picker includes these families and loads their local font assets in
the settings WebView for previews. User-selected system fonts remain supported;
they depend on the user's machine and are not redistributed. Existing saved
custom font selections are preserved.

Font files and their upstream OFL notices are in `assets/fonts/overlay`.
The notices are also embedded in the settings document's font stylesheet.
See that directory's README for download provenance.

## Additional presets

The theme picker also includes eight presets, each with a distinct material detail:

| Theme | Appearance | Bundled family |
| --- | --- | --- |
| Terminal | Upward-moving phosphor scan lines and a blinking underscore | Orbitron |
| Blueprint | Drafting grid with square registration marks | Orbitron |
| Cassette | Cream tape label, bare microphone and screw rings | Google Sans |
| Arcade | Violet cabinet with pixel trim and an offset shadow | Orbitron |
| Paper | Ruled paper, red margin and ink colors | Nunito |
| Frosted | Translucent ice-blue surface with glass highlights | Google Sans |
| Porcelain | Ceramic pill, medallion and double blue rim | Nunito |
| Radar | Green instrument panel with expanding, fading signal rings | Orbitron |

`details.rs` clips line and ring geometry to the actual rounded card contour
before GPU tessellation, then paints these details behind the content and crossfades them with
existing state layers. Radar runs three staggered expanding rings on a stable
2.4-second clock. Terminal scrolls its scan lines upward at five logical pixels
per second, wrapping seamlessly every four pixels. Its underscore cursor sits
immediately after the shaped label and blinks every 500 ms; its measured width
is always reserved, so blinking does not move the text or resize the overlay. Both use a stable clock
and request animation frames only while visible or fading.
Icon-only content uses an explicitly sized centered row, retaining themed icon
containers where their fill provides contrast (such as Brutalism).
Cassette and Radar use bare microphone icons. All presets retain custom labels, content selection,
placement and scale, with separate live/muted palettes. Mute-failure warnings
retain the themed foreground for contrast and always display their warning
icon and failure label. No additional fonts or icon downloads are required.

## Theme icons

The microphone icon picker is available for every theme, including Cute Sticker.
Selecting a theme always resets the icon pair to that theme's default; subsequent
icon selections update immediately and persist until another theme selection.
Font and background customization remain specific to Custom. Failure feedback
continues to force the warning icon independently of the selected microphone pair.

## Corner radius and Neon motion

Painted card themes expose a 0?32 px corner-radius slider. Overrides are saved
per theme; themes with no override retain their original geometry. Windows
acrylic and Cute Sticker retain their existing forms and do not show this slider.
Legacy Custom radius settings remain the fallback when no override is saved.
Porcelain's inner rim follows its outer radius.

Neon's halo breathes over a four-second cycle, from 65% to 100% of its original
alpha, with its color, blur and gutter unchanged. Animation uses the same stable
clock as the other ambient details and requests frames only while visible or fading.

## Cute Sticker

`src/gpui_overlay/sticker.rs` composes the selected Iconify mic artwork with
stacked, tilted lettering and sparkles. Separate paper and ink SVG masks give
the artwork a white die-cut contour. Text is converted to paths with the bundled
Nunito font; custom labels are XML-escaped and preserved. Default labels become
`silence!` while muted and `on air!` while live. Content mode and scaling keep
their existing reactive controls.

The sticker lands in 560 ms and peels away in 440 ms, with perspective compression,
rotation, a lifted paper corner, and a separating shadow. It remains opaque
until lifted. Interrupted transitions retarget from their current value, even
when the new direction has a different duration. Transparent gutters reserve
space for the lift and shadow; long labels also reserve vertical tilt space.

## Verification

Dependency resolution/download, manifest metadata, Rust formatting/parser checks,
and standalone animation tests have been checked without building the app.
These checks do not establish that the full application compiles or that the
native overlay renders correctly on a device.

The existing `dx serve` session is the place to verify runtime behavior:

- Toggle mute quickly during fades; width and opacity should reverse smoothly.
- Change appearance, labels, fonts, scale, and opacity while visible.
- Exercise all visibility modes and the forced mute-failure warning.
- Check click-through against another application, all six action bindings,
  double clicks, and dragging without a trailing click action.
- Move between monitors with different DPI and test placement at 0/50/100%.
- Confirm the overlay never takes focus or adds a taskbar entry.
