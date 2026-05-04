<p align="center">
  <img src="assets/app.png" alt="silence! logo" width="128" height="128">
</p>

<h1 align="center">silence!</h1>

<p align="center">
  <b>Free, open-source microphone mute control for Windows with multi-action hotkeys, gamepad input, hold actions, auto-mute, overlay feedback, sounds, tray controls, and device switching.</b>
</p>

<p align="center">
  <a href="https://silencemute.fun/">Website</a>
  |
  <a href="https://github.com/vertopolkaLF/silence/releases">Download</a>
  |
  <a href="https://github.com/vertopolkaLF/silence/issues">Issues</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-Windows%2010%20%2F%2011-blue?style=flat-square" alt="Platform">
  <img src="https://img.shields.io/badge/Rust-2024-orange?style=flat-square" alt="Rust edition">
  <img src="https://img.shields.io/badge/UI-Dioxus%200.7-green?style=flat-square" alt="UI framework">
</p>

---

## Overview

`silence!` is a Windows microphone mute utility built for fast, reliable control from anywhere. Mute a specific microphone or every active microphone with keyboard, mouse, or gamepad input, then confirm the state with tray icons, sounds, and an on-screen overlay.

`silence! v2` is a Rust rebuild focused on more control, cleaner settings, live updates, and much less disk/RAM usage.

## What's new in v2

- Rebuilt with Rust, Dioxus desktop, and a new settings UI
- Tiny footprint: about **20MB for the app plus bundled assets** and **under 10MB RAM** during normal work
- Device management: Bind hotkey to switch to/between Output/Input devices or use new tray menu
- More reliable gamepad support, including wheels and PlayStation controllers
- More options to customize overlay and different icons
- New tray customize options
- Import/export for settings with ability to import settings from the v1 app

## Features

### Input and mute control

- Multiple hotkeys per action
- Toggle mute, force mute, force unmute
- Hold to mute, hold to unmute, or hold to toggle
- Optional "ignore modifiers" matching for more flexible shortcuts
- Microphone selection with an `All microphones` mode
- Default input and output device switching
- Open Settings from a hotkey

### Feedback and customization

- Visual overlay with mic icon or dot variants
- Overlay visibility modes: always visible, muted only, unmuted only, or after toggle
- Duration control for after-toggle visibility
- Custom overlay icon pairs from multiple icon packs
- Colored, monochrome, and system-color icon styles
- Dark or light overlay background
- Background opacity, content opacity, border, border radius, size scale, and position controls
- Optional text next to the overlay icon
- Tray icon variants: logo, mic status, or color dot
- Built-in sound themes plus custom audio files (`MP3`, `WAV`, `OGG`, `FLAC`)
- Separate hold-action sound overrides for volume, mute sound, and unmute sound

### Everyday quality-of-life

- Start with Windows
- Mute microphone on app launch
- Auto-mute after keyboard and mouse inactivity
- Optional unmute on activity
- Optional sounds on auto-mute
- Double-click tray icon to open Settings, or disable that delay for faster single-click muting
- Mica background support
- Settings backup and restore
- v1 settings import
- In-app update flow

### Performance and size

- About **20MB total** for the app and bundled assets
- Uses **under 10MB of RAM** during normal work
- Built to sit quietly in the background instead of cosplaying as an Electron-powered space heater

## Screenshots

| Hotkeys                                        | Devices                                       |
| ---------------------------------------------- | --------------------------------------------- |
| ![Hotkeys settings](website/screenshots/1.png) | ![Device settings](website/screenshots/2.png) |

| Sounds                                        | Overlay                                        |
| --------------------------------------------- | ---------------------------------------------- |
| ![Sounds settings](website/screenshots/3.png) | ![Overlay settings](website/screenshots/4.png) |

| Tray icon                                        | Auto-Mute                                        |
| ------------------------------------------------ | ------------------------------------------------ |
| ![Tray icon settings](website/screenshots/6.png) | ![Auto-Mute settings](website/screenshots/7.png) |

## Installation

### Download a release

1. Open the [releases page](https://github.com/vertopolkaLF/silence/releases).
2. Pick the package for your architecture:
   - Installer: `silence-<version>-windows-x64-setup.exe`, `silence-<version>-windows-x86-setup.exe`, or `silence-<version>-windows-arm64-setup.exe`
   - Portable: `silence-<version>-windows-x64-portable.zip`, `silence-<version>-windows-x86-portable.zip`, or `silence-<version>-windows-arm64-portable.zip`
3. Install it or extract the portable archive.
4. Launch `silence!.exe`.

No MSIX packaging is required.

## Build from source

Requirements:

- Windows 10 or Windows 11
- [Rust](https://www.rust-lang.org/tools/install)
- [`dioxus-cli`](https://dioxuslabs.com/)
- NSIS if you want to build the installer bundle

Install Dioxus CLI if needed:

```powershell
cargo install dioxus-cli
```

Clone and run:

```powershell
git clone https://github.com/vertopolkaLF/silence.git
cd silence
dx serve
```

Build a release app:

```powershell
dx build --platform windows --release --target x86_64-pc-windows-msvc
```

Build all release artifacts used by the project:

```powershell
.\build.ps1
```

The packaging script builds `x64`, `x86`, and `arm64` portable archives and NSIS installers into `dist\<version>`.

## Contributing

Bug reports, feature requests, and pull requests are welcome.

## License

This project is open source and available under the [Apache License 2.0](LICENSE).
