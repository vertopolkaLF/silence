# silence! v2.0 - Rust Rebuild + Settings Overhaul

> **Same mic-muting obsession. New engine. Way more control. Somehow still not a toaster.**

## What's New in v2.0

### Complete v2 Rebuild

- **Rebuilt in Rust + Dioxus** - The app has been rebuilt around Rust, Dioxus desktop, and direct Windows integration. The goal is the same: mute the damn microphone reliably, just with a cleaner foundation.

- **New Settings UI** - Settings now have a proper sidebar, animated tab transitions, cleaner controls, and section navigation that does not feel like it was assembled during a power outage.

- **Immediate Settings Updates** - Settings apply as soon as you change them. No Save button. No Apply button. No ceremonial nonsense. You change the overlay, it changes right now.

- **Mica Background Support** - Added a toggle for the app's Mica background, because Windows can look nice when it is not busy inventing new ways to be weird.

### Tiny Footprint

- **~20MB App + Assets** - The app plus bundled assets come in at about 20MB. Not 200MB. Not "please clear disk space for a mute button." Just 20MB-ish.

- **Under 10MB RAM During Work** - Normal background usage stays under 10MB of RAM, because muting a microphone should not require the memory budget of a small browser tab.

### Hotkeys Got Way More Useful

- **Multi-Action Hotkeys** - Hotkeys can now be assigned to toggle mute, mute, unmute, hold actions, default device switching, and opening Settings.

- **Keyboard, Mouse, and Gamepad Input** - Bind normal keyboard shortcuts, mouse buttons, modifier-only shortcuts, Xbox controller buttons, or two-button gamepad combos.

- **Hold Actions** - Configure hold-to-mute, hold-to-unmute, or hold-to-toggle behavior without cramming everything into one sad shortcut.

- **Per-Hotkey Targets** - Apply mute actions to the current default mic, a specific mic, or all active microphones.

- **Device Switching Hotkeys** - Set or toggle default input and output devices from hotkeys. Because digging through Windows sound settings mid-game is an act of self-harm.

### Overlay Customization

- **Overlay Variants** - Choose between a mic icon overlay or a minimal dot overlay.

- **Expanded Icon Packs** - Pick from multiple microphone icon pairs instead of being stuck with one blessed little symbol forever.

- **Icon Color Modes** - Use colored, monochrome, or system-color icon styling.

- **Live Appearance Controls** - Change overlay text, background style, background opacity, content opacity, border, border radius, scale, and position with immediate feedback.

- **Visibility Rules** - Keep the overlay always visible, show it only while muted, show it only while unmuted, or show it briefly after toggling.

- **Move Overlay Mode** - Position the overlay precisely with live horizontal and vertical controls.

### Tray Icon Control

- **Tray Icon Style Picker** - Choose between the app logo, mic status icon, or color dot.

- **Tray Status Icon Customization** - When using the mic status tray icon, pick the icon pair and color style.

- **Double-Click Settings Option** - Double-clicking the tray icon can open Settings, or you can disable that delay so single-click muting feels faster. Tiny setting, big "why was this annoying before" energy.

### Sounds

- **Built-In Sound Themes** - Includes 8-Bit, Blob, Digital, Discord, Pop, Punchy, Sci-Fi, and Vibrant themes.

- **Custom Sound Library** - Add your own `MP3`, `WAV`, `OGG`, or `FLAC` sounds and use them for mute/unmute feedback.

- **Preview Sounds** - Preview sounds directly from the picker instead of toggling your mic like a confused metronome.

- **Hold Sound Overrides** - Hold actions can override the default volume, mute sound, and unmute sound, or inherit the global sound settings.

### Auto-Mute

- **Mute on App Launch** - Start silence! with your microphone already muted.

- **Mute After Inactivity** - Automatically mute after a configurable number of minutes without keyboard or mouse activity.

- **Optional Unmute on Activity** - If inactivity mute triggered the mute, activity can optionally unmute again.

- **Auto-Mute Sounds** - Decide whether auto-mute should play feedback sounds.

### Devices

- **Input Device Switching** - Change the Windows default input device from the app.

- **Output Device Switching** - Change the Windows default output device too, because apparently microphones were not enough trouble.

- **Default Device Awareness** - Device pickers show and track the current Windows default devices.

### Import, Export, and Updates

- **Settings Export** - Back up your current settings.

- **Settings Import** - Restore settings from a backup.

- **Import from silence! v1** - Convert and import settings from the old v1 app data folder.

- **In-App Update Flow** - Check for updates, see when a new version is available, download it with progress, and launch the installer from the About screen.

### Packaging

- **NSIS Installer** - v2 uses a normal Windows installer package. No MSIX circus.

- **Portable Builds** - Portable archives are still produced for people who like control and hate unnecessary installers. Respect.

- **Architecture Builds** - Release packaging supports `x64`, `x86`, and `arm64`.

---

## Technical Changes

- Rebuilt project as a Rust 2024 application
- Added Dioxus 0.7 desktop UI
- Reduced normal working memory usage to under 10MB RAM with app and bundled assets around 20MB
- Added direct Windows API integration for audio devices, hotkeys, tray behavior, windowing, Mica, and notifications
- Added `gilrs` gamepad input support
- Added `rodio` sound playback
- Added GitHub release update checks and streaming update downloads
- Added NSIS packaging through Dioxus bundle configuration
- Added release packaging script for multi-architecture installer and portable artifacts

---

<p align="center">
  <b>v2 is bigger, sharper, and less willing to tolerate Windows audio bullshit.</b>
</p>
