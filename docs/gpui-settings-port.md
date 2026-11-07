# Porting the settings window to GPUI

The Dioxus settings UI (`src/gui/**`, `assets/styles/*.css`,
`assets/scripts/welcome-stars.js`) is being rewritten in GPUI under
`src/settings_ui/**`. Until the port is complete the GPUI window opens with
`silence.exe --settings-gpui`; `--settings` still opens the Dioxus window.

## Goal

Keep the look and behaviour identical to the Dioxus version: layout, sizes,
colors, fonts, hover/active states, transitions and keyframe animations.
Read the original rsx **and** its CSS (including the cascade from
`global.css`, `controls.css`, `sounds.css`, `general.css`, ...) and port the
computed result. Accepted deviations: no CSS radial gradients (use a blurred
shadow or image), welcome stars drawn with paths/quad shadows, DirectWrite text
rendering, custom scrollbars. Any other visual difference must be reported.

Project rules (`AGENTS.md`): settings apply immediately (no Save buttons);
everything is reactive; every layout shift is animated so it explains itself;
no responsive breakpoints (fixed 760x590 window); hover states never change
borders (use background, shadow, opacity or transform). Icons come from
`assets/icons` (Solar first; download missing ones from Iconify into
`assets/icons`). Every file in `assets/icons`, `assets/gamepad` and
`assets/flags` is embedded automatically by `build.rs`.

## Architecture

- `settings_ui/mod.rs`: `SettingsApp` (the single root view). Config changes go
  through `app.update_config(cx, |config| ...)`, which saves, normalizes and
  notifies the tray. `app.snapshot` holds config, devices, displays, fonts,
  mute state; `live.rs` keeps it current from window-message events.
- `theme.rs`: `Palette` = the CSS custom properties (`--surface-2`,
  `--accent-text`, ...) computed with the same `color-mix(in oklch)` math
  (`color.rs`: `mix`, `mix_pct`, `lerp`, `Oklch::opacity`). Get it with
  `style::palette(cx)` or `app.palette`. Convert with `.hsla()`.
- `anim.rs`: CSS transitions. `anim::tween(window, cx, key, target, UI)`
  returns the animated value for a keyed element and requests frames while it
  moves; `tween_from` animates on mount (keyframe entrances). `Easing::Bezier`
  matches any CSS `cubic-bezier`. Keys must be unique in the window.
- `controls/`: shared widgets (`toggle`, `checkbox`, `button`, `hover_tween`,
  `track_pointer`, plus select/range/segmented/text input). Callbacks have the
  shape `Fn(&mut SettingsApp, T, &mut Window, &mut Context<SettingsApp>)`.
- `style.rs`: `h1`..`h4`, `label`, `p`, `divider`, `page_header`.
- `sections/<tab>.rs`: `pub(crate) struct State` (UI state that must survive
  re-renders, stored in `app.sections.<tab>`) and `render(app, window, cx) ->
  Vec<AnyElement>` returning **one block per entry in
  `SettingsTab::sections()`**, in order. The content area adds the page padding
  (`44px 38px`), scrolling, and uses the blocks' positions for the sidebar's
  sub-section tracking. `sections/general.rs` is the reference port, including
  a modal drawn above the window with `deferred(anchored()...)`.
- Window-level overlays (hotkey editor) render from
  `sections::hotkeys::modal_host`.
- Raw keyboard recording: set `crate::set_settings_hotkey_recording(true)`;
  the window procedure then forwards `WM_KEYDOWN/UP` as
  `live::RawKey { vk, extended, down }` on `live::keys()` and swallows them.
  Mouse and gamepad recording keep using the existing helpers in
  `main_parts/app_boot.rs`.
- Network work (update checks/downloads) runs on `live::runtime()` (Tokio);
  results come back over a channel to a `cx.spawn` task.
- GPUI is pinned to Zed rev `a84689073d296dfd39987bc7dd478e43ef76d83a`; read the
  source in `~/.cargo/git/checkouts/zed-a70e2ad075855582/a846890/crates/gpui`
  (examples include `input.rs`, `scrollable.rs`, `animation.rs`).
