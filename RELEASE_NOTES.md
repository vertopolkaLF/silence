# silence! v2.2.0 - mic in use, finally visible

### New stuff

- Added overlay visibility mode for showing overlay only while an app is using the microphone
- Added tray icon mic-in-use indicator with a tiny theme color badge
- Added tray menu submenu that lists apps currently using the microphone
- Clicking an app in that tray submenu now tries to bring that app to the front

### Tray icon

- Mic-in-use badge is enabled by default
- Badge works with logo, mic status, and color dot tray icon styles
- Badge uses a contrast color when the tray mic icon is set to system color

--

# silence! v2.1.0 - volume control and more overlay options

### New stuff

- Added volume control hotkeys
- New hotkey action picker since it now contains a lot of things :D
- Updated hotkeys list UI

### Overlay customization

- Added custom overlay labels
- Added font selector for overlay

### Fixes

- Reduced gamepad polling when it is not needed

--

# silence! v2.0.2 - yeah, i forgot something

- Restored feature to select display for overlay

--

# silence! v2.0.1 - bug fixing already

- Fixed sounds not playing on Windows 10
- Disabled Mica background on Windows 10
- Extended allowed keys list for hotkeys
- Recorded numpad keys now work even with numpad off
- Probably fixed system color fallback

--

# silence! v2.0.0 - Now in Rust!

Hey there! First of all - thx for still using silence! it means a lot especially for my first "real" app

Today I am finally ready to make a v2.0 release which I have been working for awhile. It's v2 for a reason, because it's now written in Rust which means:

- App's size is now only 20MB. (vs old 200-300MB)
- RAM usage in background is <10MB

Thanks to this reddit comment that haunted me for a while

> Your program is an astonishing 100+ MB. That's bigger than the Audacity audio editing program as a full install!

Why it was the way it was?

- I chose WinUI because I was too lazy to do actual UI work in C# and .NET bullshit and it came with bunch of WindowsSDK shit that bloated app's size. And now it's Rust and Dioxus for UI

yes, dioxus is webview, but: it's not loaded during normal work. Only when you open settings (aka actively using app).

To the new features!

- More **flexible hotkeys system** that allows you to create basically infinite amount of hotkeys
- Switching between **audio devices** with hotkeys or directly from tray menu
- **Gamepad support** that works even with Playstation controllers and even wheels

https://silencemute.fun/ (yay, also new website)
