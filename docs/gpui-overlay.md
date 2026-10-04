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

The GPU surface includes a transparent shadow gutter and reserves room for
both mute labels. The card animates within that surface, keeping its percentage
anchor stable and avoiding swapchain resizing on each animation frame.
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
