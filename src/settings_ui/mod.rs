//! The settings window, rendered with GPUI in its own `--settings` process.
//!
//! The look is ported from the former Dioxus/CSS implementation: tokens in
//! [`theme`], CSS-like transitions in [`anim`], shared widgets in [`controls`]
//! and one module per sidebar tab in [`sections`].
pub(crate) mod anim;
pub(crate) mod assets;
pub(crate) mod color;
pub(crate) mod controls;
pub(crate) mod live;
pub(crate) mod sections;
pub(crate) mod shell;
pub(crate) mod style;
pub(crate) mod theme;
pub(crate) mod welcome;

use anyhow::Result;
use gpui::{
    App, AppContext as _, Bounds, Context, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement,
    Render, SharedString, Styled, TitlebarOptions, Window, WindowBackgroundAppearance,
    WindowBounds, WindowOptions, div, px, size,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use shell::nav::{Nav, SettingsTab};
use std::{rc::Rc, time::Duration};
use theme::Palette;

/// Logical size of the settings window; also its minimum size.
pub(crate) const WINDOW_WIDTH: f32 = 760.;
pub(crate) const WINDOW_HEIGHT: f32 = 590.;

#[derive(Clone, PartialEq)]
pub(crate) struct SettingsSnapshot {
    pub config: crate::Config,
    pub devices: Vec<crate::MicDevice>,
    pub output_devices: Vec<crate::AudioDevice>,
    pub mic_using_apps: Vec<crate::MicUsingApp>,
    pub overlay_displays: Vec<crate::OverlayDisplay>,
    pub system_fonts: Vec<crate::SystemFont>,
    pub muted: bool,
}

impl SettingsSnapshot {
    fn load() -> Self {
        Self {
            config: crate::load_config().unwrap_or_default(),
            devices: crate::capture_devices().unwrap_or_default(),
            output_devices: crate::render_devices().unwrap_or_default(),
            mic_using_apps: crate::current_mic_using_apps(),
            overlay_displays: crate::overlay_displays(),
            system_fonts: crate::system_fonts(),
            muted: crate::mic_mute_state(None).unwrap_or(false),
        }
    }
}

/// A request to open the hotkey editor, possibly after navigating to Hotkeys.
#[derive(Clone, PartialEq)]
pub(crate) enum HotkeyModalRequest {
    Add {
        preset_action: Option<crate::HotkeyAction>,
    },
    Edit {
        binding: crate::HotkeyBinding,
    },
}

pub(crate) struct SettingsApp {
    pub snapshot: SettingsSnapshot,
    pub palette: Palette,
    pub focus_handle: FocusHandle,
    pub nav: Nav,
    pub sections: sections::Sections,
    pub welcome: welcome::State,
    /// Id of the open select menu; only one can be open at a time.
    pub open_select: Option<SharedString>,
    pub available_update: Option<crate::updater::UpdateInfo>,
    pub hotkey_modal_request: Option<HotkeyModalRequest>,
    pub pending_hotkey_modal_after_nav: Option<HotkeyModalRequest>,
    /// Set when the close button starts the window's fade-out.
    pub closing: Option<std::time::Instant>,
    /// Set when the welcome sequence hands over to the main screen.
    pub main_intro: Option<std::time::Instant>,
    _tasks: Vec<gpui::Task<()>>,
}

impl SettingsApp {
    fn new(snapshot: SettingsSnapshot, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let palette = *cx.global::<Palette>();
        let startup_tab = if settings_should_open_about() {
            SettingsTab::About
        } else {
            SettingsTab::Hotkeys
        };
        let mut app = Self {
            snapshot,
            palette,
            focus_handle: cx.focus_handle(),
            nav: Nav::new(startup_tab),
            sections: sections::Sections::default(),
            welcome: welcome::State::default(),
            open_select: None,
            available_update: None,
            hotkey_modal_request: None,
            pending_hotkey_modal_after_nav: None,
            closing: None,
            main_intro: None,
            _tasks: Vec::new(),
        };
        app._tasks.extend(live::start(window, cx));
        app._tasks.push(check_for_update(cx));
        app.focus_handle.focus(window, cx);
        app
    }

    /// Loads the latest config, applies `update`, normalizes and saves it, and
    /// tells the tray process to reload. Mirrors the old `update_settings`.
    pub fn update_config(&mut self, cx: &mut Context<Self>, update: impl FnOnce(&mut crate::Config)) {
        let mut config = crate::load_config().unwrap_or_else(|_| self.snapshot.config.clone());
        let startup_was_enabled = config.startup.launch_on_startup;
        let mica_was_enabled = crate::effective_settings_mica_enabled(&config);
        update(&mut config);
        crate::normalize_hotkeys(&mut config.hotkeys);
        crate::normalize_overlay_config(&mut config.overlay);
        crate::normalize_tray_icon_config(&mut config.tray_icon);
        let _ = crate::save_config(&config);
        if config.startup.launch_on_startup != startup_was_enabled {
            let _ = crate::sync_startup_registration(config.startup.launch_on_startup);
        }
        crate::apply_live_config(&config, crate::config_modified_time());
        let mica = crate::effective_settings_mica_enabled(&config);
        self.snapshot.config = config;
        if mica != mica_was_enabled {
            self.set_mica(mica, cx);
        }
        cx.notify();
    }

    /// Re-reads the config from disk (after import/reset or a tray change).
    pub fn reload_config(&mut self, cx: &mut Context<Self>) {
        if let Ok(config) = crate::load_config() {
            let mica = crate::effective_settings_mica_enabled(&config);
            let changed_mica = mica != self.palette.mica;
            if self.snapshot.config != config {
                self.snapshot.config = config;
                cx.notify();
            }
            if changed_mica {
                self.set_mica(mica, cx);
            }
        }
    }

    pub fn refresh_devices(&mut self, cx: &mut Context<Self>) {
        let devices = crate::capture_devices().unwrap_or_default();
        let output_devices = crate::render_devices().unwrap_or_default();
        if devices != self.snapshot.devices || output_devices != self.snapshot.output_devices {
            self.snapshot.devices = devices;
            self.snapshot.output_devices = output_devices;
            cx.notify();
        }
    }

    fn set_mica(&mut self, mica: bool, cx: &mut Context<Self>) {
        self.palette = Palette::load(mica);
        cx.set_global(self.palette);
        crate::set_settings_mica_enabled(mica);
        cx.notify();
    }

    /// Starts the close fade (`.window.closing`, 120ms) and then quits.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.closing.is_some() {
            return;
        }
        self.closing = Some(std::time::Instant::now());
        if self.snapshot.config.welcome_completed {
            self.update_config(cx, |config| config.hotkeys_paused = false);
        }
        cx.spawn(async move |_, cx| {
            cx.background_executor().timer(Duration::from_millis(120)).await;
            let _ = cx.update(|cx| cx.quit());
        })
        .detach();
        cx.notify();
    }
}

impl Focusable for SettingsApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SettingsApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.palette.mica != cx.global::<Palette>().mica {
            self.palette = *cx.global::<Palette>();
        }
        let palette = self.palette;
        let closing = self
            .closing
            .map(|started| (started.elapsed().as_secs_f32() / 0.12).min(1.))
            .unwrap_or(0.);
        if self.closing.is_some() && closing < 1. {
            window.request_animation_frame();
        }
        let ease = anim::Easing::Ease.apply(closing);

        let body = if self.snapshot.config.welcome_completed {
            shell::render_main(self, window, cx).into_any_element()
        } else {
            welcome::render(self, window, cx).into_any_element()
        };

        div()
            .id("settings-window")
            .track_focus(&self.focus_handle)
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(palette.window_bg.hsla())
            .text_color(palette.text.hsla())
            .font_family(theme::FONT_TEXT)
            .text_size(px(16.))
            // `.window.closing { opacity: 0; transform: scale(.985) }`
            .opacity(1. - ease)
            .child(body)
            .child(shell::titlebar::render(self, window, cx))
            .children(
                self.snapshot
                    .config
                    .welcome_completed
                    .then(|| sections::hotkeys::modal_host(self, window, cx)),
            )
    }
}

fn check_for_update(cx: &mut Context<SettingsApp>) -> gpui::Task<()> {
    let (sender, receiver) = flume::bounded(1);
    live::runtime().spawn(async move {
        let _ = sender.send(crate::check_for_update().await);
    });
    cx.spawn(async move |this, cx| {
        let Ok(result) = receiver.recv_async().await else {
            return;
        };
        let _ = this.update(cx, |app, cx| {
            match result {
                Ok(crate::updater::UpdateCheck::Available(update)) => {
                    app.available_update = Some(update)
                }
                Ok(crate::updater::UpdateCheck::UpToDate) => app.available_update = None,
                Err(error) => eprintln!("settings update check failed: {error:?}"),
            }
            cx.notify();
        });
    })
}

pub(crate) fn settings_should_open_about() -> bool {
    std::env::args().any(|arg| arg == "--about" || arg == "--about-update")
}

pub(crate) fn settings_should_start_update() -> bool {
    std::env::args().any(|arg| arg == "--about-update")
}

/// Runs the settings window until it closes. Blocks the calling thread.
pub(crate) fn run() -> Result<()> {
    let platform = Rc::new(gpui_windows::WindowsPlatform::new(false)?);
    gpui::Application::with_platform(platform)
        .with_assets(assets::SettingsAssets)
        .run(|cx: &mut App| {
            assets::register_fonts(cx);
            let snapshot = SettingsSnapshot::load();
            let mica = crate::effective_settings_mica_enabled(&snapshot.config);
            cx.set_global(Palette::load(mica));
            let window_size = size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT));
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    window_size,
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some(crate::SETTINGS_WINDOW_TITLE.into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                window_min_size: Some(window_size),
                is_resizable: true,
                focus: true,
                show: true,
                window_background: if mica {
                    WindowBackgroundAppearance::MicaBackdrop
                } else {
                    WindowBackgroundAppearance::Opaque
                },
                ..Default::default()
            };
            let opened = cx.open_window(options, |window, cx| {
                cx.new(|cx| SettingsApp::new(snapshot, window, cx))
            });
            match opened {
                Ok(handle) => {
                    let _ = handle.update(cx, |_, window, _| {
                        if let Ok(handle) = HasWindowHandle::window_handle(window) {
                            if let RawWindowHandle::Win32(raw) = handle.as_raw() {
                                crate::install_settings_window_guard(raw.hwnd.get());
                            }
                        }
                    });
                }
                Err(error) => {
                    eprintln!("open settings window: {error:#}");
                    cx.quit();
                }
            }
        });
    Ok(())
}
