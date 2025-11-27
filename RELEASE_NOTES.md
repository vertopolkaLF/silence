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



