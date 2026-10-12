//! Event-driven state for the open settings window.
//!
//! Window messages arrive on the settings window procedure
//! (`settings_window_proc`) and are forwarded here:
//! - `WM_AUDIO_MUTE_STATE_CHANGED` / `WM_AUDIO_ENDPOINT_CHANGED` from the same
//!   COM audio callbacks the tray uses, registered for this window;
//! - `WM_CONFIG_CHANGED` posted by the tray after it saves `config.json`;
//! - `WM_DISPLAYCHANGE` for the overlay display list.
//!
//! Only the list of apps using the microphone is polled, because Windows has
//! no convenient notification for it, and only while this window is open.
use super::SettingsApp;
use gpui::{Context, Task, Window};
use once_cell::sync::Lazy;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiveEvent {
    MuteChanged,
    EndpointsChanged,
    ConfigChanged,
    DisplaysChanged,
}

static EVENTS: Lazy<(flume::Sender<LiveEvent>, flume::Receiver<LiveEvent>)> =
    Lazy::new(flume::unbounded);

/// Called from the settings window procedure on the UI thread.
pub(crate) fn push(event: LiveEvent) {
    let _ = EVENTS.0.send(event);
}

/// A key press captured by the window procedure while a shortcut is being
/// recorded (`SETTINGS_HOTKEY_RECORDING`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RawKey {
    pub vk: u32,
    pub extended: bool,
    pub down: bool,
}

static KEYS: Lazy<(flume::Sender<RawKey>, flume::Receiver<RawKey>)> = Lazy::new(flume::unbounded);

pub(crate) fn push_key(key: RawKey) {
    let _ = KEYS.0.send(key);
}

/// Receiver for recorded keys; the hotkey editor drains it while recording.
pub(crate) fn keys() -> flume::Receiver<RawKey> {
    KEYS.1.clone()
}

/// Shared Tokio runtime for network work (update checks and downloads).
pub(crate) fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("start settings runtime")
    });
    &RUNTIME
}

pub(super) fn start(window: &mut Window, cx: &mut Context<SettingsApp>) -> [Task<()>; 2] {
    if let Ok(handle) = raw_window_handle::HasWindowHandle::window_handle(window) {
        if let raw_window_handle::RawWindowHandle::Win32(raw) = handle.as_raw() {
            crate::register_settings_audio_notifications(raw.hwnd.get());
        }
    }
    let receiver = EVENTS.1.clone();
    let events = cx.spawn(async move |this, cx| {
        while let Ok(first) = receiver.recv_async().await {
            // Coalesce a burst (e.g. several endpoint callbacks) into one refresh.
            let mut events = vec![first];
            events.extend(receiver.try_iter());
            let alive = this.update(cx, |app, cx| {
                if events.contains(&LiveEvent::EndpointsChanged) {
                    crate::rebind_settings_audio_notifications();
                    app.refresh_devices(cx);
                }
                if events
                    .iter()
                    .any(|event| matches!(event, LiveEvent::MuteChanged | LiveEvent::EndpointsChanged))
                {
                    let muted = crate::mic_mute_state(None).unwrap_or(app.snapshot.muted);
                    if muted != app.snapshot.muted {
                        app.snapshot.muted = muted;
                        cx.notify();
                    }
                }
                if events.contains(&LiveEvent::ConfigChanged) {
                    app.reload_config(cx);
                }
                if events.contains(&LiveEvent::DisplaysChanged) {
                    let displays = crate::overlay_displays();
                    if displays != app.snapshot.overlay_displays {
                        app.snapshot.overlay_displays = displays;
                        cx.notify();
                    }
                }
            });
            if alive.is_err() {
                break;
            }
        }
    });
    let mic_apps = cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(crate::MIC_IN_USE_REFRESH_MS))
                .await;
            let alive = this.update(cx, |app, cx| {
                let apps = crate::current_mic_using_apps();
                if apps != app.snapshot.mic_using_apps {
                    app.snapshot.mic_using_apps = apps;
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
        }
    });
    [events, mic_apps]
}
