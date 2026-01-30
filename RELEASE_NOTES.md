# 🎤 silence! v1.6 — Mouse Hotkeys

> **Muting — now on your mouse too!**

## ✨ What's New in v1.6

### 🖱️ Mouse Button Hotkeys

- **Mouse Button Support** — Bind mouse buttons to toggle or hold hotkeys, with modifier combinations.

- **Hold-to-Record for LMB/RMB** — Left and right buttons require a 1-second hold during recording to prevent accidental binds.

---

# 🎤 silence! v1.5 — Hold to Mute

> **Press and hold. That's it.**

## ✨ What's New in v1.5

### 🎯 Hold to Mute Feature

- **Hold Hotkey Support** — Configure a separate hotkey that works while you hold it down. Perfect for quick unmutes during meetings.

- **Three Action Modes** — Choose how the hold hotkey behaves:
  - **Toggle current state** — Flip between muted/unmuted while holding
  - **Hold to mute** — Mute while holding, unmute on release (push-to-mute)
  - **Hold to unmute** — Unmute while holding, mute on release (push-to-talk)

- **Modifier-Only Hotkeys** — Bind 2+ modifier keys (Ctrl, Alt, Shift, Win) as your hold hotkey. Hold them for 1 second during recording to set. Great for avoiding conflicts with other apps.

- **Independent Settings** — Hold hotkey has its own sound and overlay toggles. Want sounds on toggle but not on hold? Done.

## 🔧 Technical Changes

- Extended `KeyboardHookService` with hold hotkey detection and modifier-only binding
- Added `ModifierHoldProgress` event for visual feedback during recording
- Separate hold hotkey state management independent of toggle hotkey
- Hold action respects individual sound and overlay preferences

---

<p align="center">
  <b>Hold it. Sneeze it. Release it. </b>
</p>

---

# 🎤 silence! v1.4.1 — Polish & Fixes

> **The little things matter.**

## 🔧 What's Fixed in v1.4.1

### 🐛 Bug Fixes

- **Overlay Animation** — Fixed animation glitches when using "show only when muted/unmuted" visibility modes. No more janky transitions.

- **DPI Scaling** — Window size now properly respects OS scaling settings. Looks right on every display.

- **Resolution Changes** — Overlay position automatically recalculates when you change screen resolution. No more lost overlays.

- **Set Position Button** — Button now returns to normal state when you press ESC during positioning. Small fix, big quality of life.

### ✨ Improvements

- **Overlay Size Control** — Added size parameter for overlay. Make it bigger or smaller, your choice.

- **Auto-Update Notifications** — Cleaned up notification system. No more debug spam, just clean update alerts.

- **Tray Menu** — App name and version now shown in tray menu. Know what you're running at a glance.

---

# 🎤 silence! v1.4 — Overlay Overhaul

> **Now it's actually good. No, seriously.**

## ✨ What's New in v1.4

### 🚀 Complete Overlay Rewrite

- **Pure Win32 Layered Window** — Rewrote the entire overlay from scratch using Win32 API. No more WinUI/XAML bullshit. It's faster, lighter, and actually works properly.

- **True Click-Through** — The overlay is now completely invisible to your cursor. No more accidental clicks, no more window stealing focus. It's there, but Windows doesn't even know it exists.

- **Smooth Animations** — Added buttery smooth fade-in/fade-out animations. Overlay appears and disappears like magic. State transitions use crossfade animation so you never see jarring icon swaps.

### 🎨 Appearance Customization

- **Show Text Option** — Toggle between icon-only or icon with "Microphone is muted/unmuted" text. Your choice, your desktop.

- **Icon Styles** — Choose between colored (red/green) or monochrome icons. Match your aesthetic, or don't. We don't judge.

- **Background Styles** — Dark or light background. Because sometimes you want it to blend in, sometimes you want it to stand out.

- **Opacity Controls** — Two separate sliders:
  - **Background opacity** (0-100%) — Control how transparent the background is
  - **Content opacity** (20-100%) — Control icon and text visibility independently

- **Border Radius** — Adjust corner rounding from 0px (sharp) to 24px (pill-shaped). Make it yours.

- **Border Toggle** — Show or hide the Windows 11 style border. Because borders are optional, not mandatory.

### ⏱️ New Visibility Mode

- **Show After Toggle** — Overlay appears briefly after you mute/unmute, then disappears automatically. Perfect for quick confirmation without permanent screen clutter.

- **Customizable Duration** — Set how long the overlay stays visible (0.1 to 10 seconds). Want a quick flash? 0.5s. Want to stare at it? 10s. Your call.

### 🎯 Improvements

- **Better Performance** — Win32 Layered Window is way more efficient than WinUI. Lower CPU usage, smoother animations, no stuttering.

- **DPI Scaling** — Proper DPI awareness. Overlay looks crisp on any display, whether it's 96 DPI or 300 DPI.

- **Anchor-Based Repositioning** — When switching between icon-only and icon+text, overlay stays anchored correctly. No more jumping around.

- **Preview Button** — Test your overlay settings before committing. See what it looks like without toggling mute.

## 🔧 Technical Changes

- Complete rewrite: `LayeredOverlay` class using `UpdateLayeredWindow` API
- Per-pixel alpha transparency with proper compositing
- Custom fade animation system with 60fps updates
- Content crossfade for smooth state transitions
- DPI-aware rendering with proper font scaling
- Anchor-based positioning algorithm for dynamic width changes

---

<p align="center">
  <b>It's faster. It's smoother. It's better.</b>
</p>

---

# 🎤 silence! v1.3 — Visual Overlay

> **Now you can see your mute status. Everywhere. All the time.**

## ✨ What's New in v1.3

### 👁️ Visual Overlay

- **Always-On-Top Indicator** — A floating microphone icon stays on top of all windows. No more "wait, am I muted?" moments.

- **Three Visibility Modes** — Choose when to see the overlay:
  - **Always visible** — Never lose track of your mic status
  - **Visible when muted** — Show only when you're muted (default)
  - **Visible when unmuted** — Show only when you're live

- **Multi-Monitor Support** — Pick which screen displays the overlay. Works with any number of monitors.

- **Drag-and-Drop Positioning** — Click "Set Position", drag the overlay wherever you want. It magnetically snaps to the center when you get close. Press ESC or click Done to save.

- **Click-Through Design** — The overlay doesn't steal your clicks. It's there, but it doesn't get in the way.

### 🎨 Visual Polish

- **Acrylic Blur Background** — Semi-transparent with a nice blur effect. Looks sleek, doesn't block your view.

- **Color-Coded Status** — Green when live, red when muted. Instant visual feedback.

- **Clean Rounded Design** — Small 48x48 icon that fits naturally on any desktop.

## 🔧 Technical Changes

- New `OverlayWindow` using DWM attributes for borderless, topmost, click-through behavior
- Win32 API integration for precise window positioning and monitor enumeration
- Magnetic snap algorithm with smooth cubic easing
- Position stored as percentages (survives resolution changes)

---

<p align="center">
  <b>See your status. Don't guess it.</b>
</p>

---

---

# 🎤 silence! v1.2 — Sound Feedback

> **Now you can hear when you mute.**

## ✨ What's New in v1.2

### 🔊 Sound Feedback System

- **Audio Feedback on Toggle** — Hear a sound when you mute or unmute. Never wonder "did it work?" again.

- **8 Preloaded Sounds** — Choose from 8-Bit, Blob, Digital, Discord, Pop, Punchy, Sci-Fi, or Vibrant. Something for every taste.

- **Custom Sounds** — Don't like our sounds? Add your own! Supports MP3, WAV, FLAC, OGG, M4A, and WMA.

- **Separate Mute/Unmute Sounds** — Set different sounds for mute and unmute actions. Know your state by ear.

- **Volume Control** — Slider to adjust sound volume. Keep it subtle or make it loud.

- **Preview Sounds** — Test sounds before selecting them with the play button.

## 🔧 Technical Changes

- New `SoundService` using NAudio for playback (no media control integration)
- Sounds stored in `%LOCALAPPDATA%\silence\sounds\`
- Volume and sound preferences persist in settings

---

<p align="center">
  <b>Click. Hear. Know.</b>
</p>

---

---

# 🎤 silence! v1.1 — Auto-Updates & Navigation Tabs

> **Now with automatic updates and a fresh new look.**

## ✨ What's New in v1.1

### 🔄 Auto-Update System

- **Automatic Update Checks** — App checks GitHub releases on startup and notifies you when a new version is available.

- **One-Click Updates** — See the update notification in the sidebar, click "View Details", download the installer, and you're done.

- **Smart Architecture Detection** — Automatically finds the right installer for your system (x64, x86, or ARM64).

- **Toggle Auto-Check** — Don't want automatic checks? Disable it in the About page. Manual check button always available.

### 🗂️ Navigation Tabs

- **Tabbed Settings Interface** — Clean navigation between General, Appearance, and About pages.

- **Smooth Transitions** — Slide animations when switching between tabs.

- **Compact Sidebar** — Collapsible navigation with icons. Update notification adapts to collapsed state.

### 🎨 UI Improvements

- **Update Notification Badge** — Subtle indicator in the sidebar when updates are available.

- **Version Display** — Current version shown in sidebar footer and About page.

- **Improved About Page** — Now includes update status, check button, and release details.

---

## 🔧 Technical Changes

- Centralized version management in `.csproj`
- Dynamic version detection in build scripts
- GitHub Releases API integration for update checks

---

<p align="center">
  <b>Updates? We got 'em. Automatically.</b>
</p>

---

---

# 🎤 silence! v1.0 — Initial Release

> **Your meetings just got less awkward.**

We're thrilled to announce the first official release of **silence!** — a lightweight, no-bullshit microphone mute utility for Windows.

---

## ✨ What's New (Everything, it's v1.0!)

### 🎯 Core Features

- **Global Hotkey Muting** — Mute/unmute your microphone from absolutely anywhere. Gaming? Browsing? In Excel pretending to work? Doesn't matter, hotkey works everywhere.

- **System Tray Integration** — Lives quietly in your system tray. Green icon = you're live. Red icon = you're safe. No rocket science required.

- **One-Click Toggle** — Click the tray icon to toggle mute. Double-click opens settings. Your grandma could use this.

### ⌨️ Hotkey System

- **Full Modifier Support** — Create complex hotkeys like `Ctrl + Alt + M` or keep it simple with `F13`, `Pause`, whatever floats your boat.

- **Flexible Modifier Matching** — Enable "Ignore extra modifiers" so your `Shift + F23` hotkey also fires when you accidentally hit `Ctrl + Shift + F23`. We got you.

### 🎨 Modern UI

- **Mica/Acrylic Backdrop** — Windows 11 gets Mica, Windows 10 gets Acrylic. Everyone wins.

- **Smooth Animations** — Buttery smooth state transitions because we're not animals.

- **Adaptive Theme** — Follows your system theme. Dark mode gang rise up.

### ⚙️ Convenience

- **Microphone Selection** — Pick which mic to control. Useful if you have 47 audio devices like a normal person.

- **Auto-Start with Windows** — Enable it once, forget about it forever.

- **Start Minimized** — Boot straight to system tray. No window popping up in your face.

- **Portable** — No MSIX installer bullshit. Extract → Run → Profit.

---

## 🐛 Known Issues

- First release, let us know if something's broken!

---

## 📝 Feedback

Found a bug? Have a feature request? Open an issue on GitHub!

---

<p align="center">
  <b>Made for people who are tired of that "you're on mute" moment.</b>
</p>
