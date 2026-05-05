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
