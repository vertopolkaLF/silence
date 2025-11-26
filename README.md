<p align="center">
  <img src="Assets/app.png" alt="silence! Logo" width="128" height="128">
</p>

<h1 align="center">silence!</h1>

<p align="center">
  <b>A simple, lightweight microphone mute toggle for Windows with global hotkey support</b>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-Windows%2010%2F11-blue?style=flat-square" alt="Platform">
  <img src="https://img.shields.io/badge/.NET-8.0-purple?style=flat-square" alt=".NET Version">
  <img src="https://img.shields.io/badge/UI-WinUI%203-green?style=flat-square" alt="UI Framework">
</p>

---

## ✨ Features

- **Global Hotkey** — Mute/unmute your microphone from anywhere using a customizable keyboard shortcut
- **System Tray** — Lives quietly in your system tray with a color-coded icon (🟢 unmuted / 🔴 muted)
- **Quick Toggle** — Single click on the tray icon to toggle mute state
- **Modifier Support** — Use complex hotkeys like `Ctrl + Alt + M` or simple ones like `F13`
- **Flexible Matching** — Option to ignore additional modifiers (e.g., hotkey `Shift + F23` also fires on `Ctrl + Shift + F23`)
- **Auto-Start** — Optionally launch with Windows
- **Modern UI** — Mica/Acrylic backdrop, smooth animations, native Windows 10/11 look
- **Portable** — No MSIX installer required, just extract and run

## 📸 Screenshot

<p align="center">
  <i>Settings window with microphone selection and hotkey configuration</i>
</p>

## 🚀 Installation

### Download Release (Recommended)

1. Go to [Releases](../../releases) page
2. Download the latest `silence-vX.X-win-x64.zip`
3. Extract to any folder
4. Run `silence!.exe`

### Build from Source

**Requirements:**
- Windows 10 version 1809 (build 17763) or later
- [.NET 8.0 SDK](https://dotnet.microsoft.com/download/dotnet/8.0)
- Visual Studio 2022 with "Windows application development" workload (optional)

```powershell
# Clone the repository
git clone https://github.com/yourusername/silence.git
cd silence

# Build and publish
dotnet publish -c Release -r win-x64 --self-contained
```

The output will be in `bin\Release\net8.0-windows10.0.19041.0\win-x64\publish\`

## 📝 License

This project is open source. Feel free to use, modify, and distribute.

## 🤝 Contributing

Contributions are welcome! Feel free to:
- Report bugs
- Suggest features
- Submit pull requests

---

<p align="center">
  Made with ❤️ for people who are tired of fumbling with mute buttons during meetings
</p>

