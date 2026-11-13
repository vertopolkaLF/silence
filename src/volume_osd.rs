//! Output-volume on-screen display.
//!
//! This is a second topmost click-through window on its own GPUI thread
//! (`silence-gpui-volume-osd`), not a second surface on `silence-gpui-overlay`.
//! The mic overlay thread exits when that overlay is disabled and it tears the
//! whole platform down to swap fonts. A volume flyout has to appear while the
//! mic overlay is off, and a mouse-wheel burst must not rebuild a window.
//!
//! Theme ids match `gpui_overlay::theme::THEMES`. `display` uses the same
//! monitor ids as the mic overlay (`Primary`, `Monitor2`, …) plus `cursor`
//! (the monitor under the pointer at show time). Positioning is a percentage
//! of the monitor, anchored on the card, DPI-aware.
//!
//! Dev preview: `silence.exe --volume-osd-demo`. Optional theme ids after the
//! flag restyle the OSD before it exits. The flag is handled before the
//! single-instance mutex, so a running tray process is left alone.

use anyhow::{Context as _, Result};
use flume::Sender;
use once_cell::sync::Lazy;
use std::{
    mem::size_of,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicIsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use windows::{
    Win32::{
        Foundation::{BOOL, COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Dwm::{
                DWM_SYSTEMBACKDROP_TYPE, DWM_WINDOW_CORNER_PREFERENCE, DWMNCRP_DISABLED,
                DWMSBT_NONE, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_NCRENDERING_POLICY,
                DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
                DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DwmExtendFrameIntoClientArea,
                DwmSetWindowAttribute,
            },
            Gdi::{
                BeginPaint, EndPaint, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR,
                MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, PAINTSTRUCT,
            },
        },
        UI::{
            Controls::MARGINS,
            HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI},
            WindowsAndMessaging::*,
        },
    },
};

const WM_VOLUME_SURFACE: u32 = WM_APP + 60;
const WM_VOLUME_PRESENT: u32 = WM_APP + 61;
const ID_TOPMOST_TIMER: usize = 41;
const TOPMOST_INTERVAL_MS: u32 = 1_000;

static OSD: Lazy<Mutex<Option<NativeOsd>>> = Lazy::new(|| Mutex::new(None));
static RUNTIME: Lazy<Mutex<Option<Runtime>>> = Lazy::new(|| Mutex::new(None));
static READY: Lazy<Mutex<Option<Sender<()>>>> = Lazy::new(|| Mutex::new(None));
static ORIGINAL_WNDPROC: AtomicIsize = AtomicIsize::new(0);
/// The card is entering or on screen (not fading out). Only then does a click
/// on it count.
static CARD_SHOWN: AtomicBool = AtomicBool::new(false);

struct Runtime {
    sender: Sender<Command>,
    thread: Option<JoinHandle<()>>,
    stopping: Arc<AtomicBool>,
}

pub(crate) enum Command {
    Show {
        content: VolumeOsdContent,
        style: VolumeOsdStyle,
    },
    Style(VolumeOsdStyle),
    Hide,
    SurfaceReady,
    /// Slider popup above the tray icon; a second window on this thread.
    Popup(crate::volume_popup::PopupCommand),
    /// "Move OSD": stay visible and draggable until turned off.
    Positioning(bool),
    /// The playing track changed; the meter's now-playing strip re-reads it.
    Media,
    Shutdown,
}

/// Where the flyout sits, and how long it stays after the latest change.
#[derive(Clone, Debug)]
pub struct VolumeOsdStyle {
    /// Catalogue id: `Windows`, `MaterialYou`, `Cute`, `CuteSticker`, `Neon`,
    /// `Brutalism`, `Terminal`, `Blueprint`, `Cassette`, `Arcade`, `Paper`,
    /// `Frosted`, `Porcelain`, `Radar`, `Custom`.
    pub theme: String,
    /// `Primary`, `MonitorN`, or `cursor`.
    pub display: String,
    /// 0–100 across the chosen monitor. 50 is centered.
    pub position_x: f32,
    /// 0–100 down the chosen monitor. 92 sits just above a typical taskbar.
    pub position_y: f32,
    /// Hide this long after the latest [`volume_osd_show`]. Each call restarts it.
    pub duration_ms: u32,
    /// Stay on screen for as long as the shown content is muted.
    pub while_muted: bool,
    pub enabled: bool,
    /// Size multiplier (1.0 = 100%).
    pub scale: f32,
    /// User corner radius for the current theme, if they set one.
    pub corner_radius: Option<f32>,
    /// Cover, title and artist of the playing track above the meter.
    pub show_media: bool,
    /// The Custom theme's look.
    pub custom: CustomLook,
}

/// Same knobs as the mic overlay's Custom theme.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomLook {
    pub font: String,
    pub weight: u16,
    /// `Colored`, `Monochrome` or `SystemColor`.
    pub icon_style: String,
    pub light_background: bool,
    /// 0.0–1.0
    pub background_opacity: f32,
    /// 0.2–1.0, applies to the whole card.
    pub content_opacity: f32,
    pub border: bool,
}

impl Default for CustomLook {
    fn default() -> Self {
        Self {
            font: String::new(),
            weight: 500,
            icon_style: "SystemColor".to_string(),
            light_background: false,
            background_opacity: 0.96,
            content_opacity: 1.0,
            border: true,
        }
    }
}

impl Default for VolumeOsdStyle {
    fn default() -> Self {
        Self {
            theme: "Windows".to_string(),
            display: crate::OVERLAY_DISPLAY_PRIMARY.to_string(),
            position_x: 50.0,
            position_y: 92.0,
            duration_ms: 1_500,
            while_muted: false,
            enabled: true,
            scale: 1.0,
            corner_radius: None,
            show_media: false,
            custom: CustomLook::default(),
        }
    }
}

/// What the flyout is saying. Level changes animate the bar; they do not rebuild the window.
#[derive(Clone, Debug)]
pub enum VolumeOsdContent {
    Level { percent: u8, muted: bool },
    Device {
        name: String,
        kind: DeviceKind,
        percent: u8,
        muted: bool,
    },
    /// The music widget: a track and what just happened to it.
    Media {
        track: std::sync::Arc<crate::media_session::NowPlaying>,
        action: MediaAction,
    },
    /// Caps, Num or Scroll Lock was toggled.
    LockKey { key: LockKey, on: bool },
}

/// A toggle key the lock-key OSD reports on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockKey {
    Caps,
    Num,
    Scroll,
}

impl LockKey {
    pub fn from_vk(vk: u32) -> Option<Self> {
        match vk {
            0x14 => Some(Self::Caps),
            0x90 => Some(Self::Num),
            0x91 => Some(Self::Scroll),
            _ => None,
        }
    }

    pub fn vk(self) -> u32 {
        match self {
            Self::Caps => 0x14,
            Self::Num => 0x90,
            Self::Scroll => 0x91,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Caps => "Caps Lock",
            Self::Num => "Num Lock",
            Self::Scroll => "Scroll Lock",
        }
    }
}

/// Why the music widget is up. Drives its status glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaAction {
    Play,
    Pause,
    Next,
    Previous,
    /// The playlist moved on by itself.
    UpNext,
}

impl VolumeOsdContent {
    pub fn is_muted(&self) -> bool {
        match self {
            Self::Level { muted, .. } | Self::Device { muted, .. } => *muted,
            Self::Media { .. } | Self::LockKey { .. } => false,
        }
    }

    pub fn percent(&self) -> u8 {
        match self {
            Self::Level { percent, .. } | Self::Device { percent, .. } => (*percent).min(100),
            Self::Media { .. } | Self::LockKey { .. } => 0,
        }
    }
}

/// Output device class for the device-change layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    Speakers,
    Headphones,
    /// Monitors and other video-output endpoints.
    Display,
    /// Default input device change.
    Microphone,
    Other,
}

/// Start the volume OSD thread. Later calls are free.
pub fn volume_osd_init() -> Result<()> {
    let mut runtime = RUNTIME.lock().unwrap();
    if runtime.as_ref().is_some_and(|runtime| {
        runtime.thread.as_ref().is_some_and(|thread| !thread.is_finished())
    }) {
        return Ok(());
    }
    // A stopped renderer must not leave a permanently dead sender behind.
    runtime.take();
    let stopping = Arc::new(AtomicBool::new(false));
    let thread_stopping = stopping.clone();
    let (sender, receiver) = flume::unbounded();
    prepare(sender.clone())?;
    let thread = thread::Builder::new()
        .name("silence-gpui-volume-osd".into())
        .spawn(move || {
            while !thread_stopping.load(Ordering::Acquire) {
                let surface_receiver = receiver.clone();
                // Fresh thread per attempt also releases COM/driver TLS after failure.
                let result = thread::Builder::new()
                    .name("silence-gpui-volume-surface".into())
                    .spawn(move || crate::gpui_overlay::volume::run(surface_receiver))
                    .context("start volume OSD surface thread")
                    .and_then(|thread| {
                        thread.join().unwrap_or_else(|panic| {
                            Err(crate::gpui_overlay::surface_panic(panic))
                        })
                    });
                detach();
                match result {
                    Ok(()) => break,
                    Err(error) => {
                        crate::gpui_overlay::report_failure("volume OSD", &error);
                        thread::sleep(Duration::from_secs(1));
                    }
                }
            }
        })
        .context("start volume OSD thread")?;
    *runtime = Some(Runtime {
        sender,
        thread: Some(thread),
        stopping,
    });
    Ok(())
}

pub fn volume_osd_show(content: VolumeOsdContent, style: &VolumeOsdStyle) {
    send(Command::Show {
        content,
        style: style.clone(),
    });
}

/// Restyle a visible flyout without restarting its content. Also restarts the hide timer.
pub fn volume_osd_apply_style(style: &VolumeOsdStyle) {
    send_if_running(Command::Style(style.clone()));
}

/// "Move OSD": keep the flyout up and let the mouse drag it. The new spot is
/// posted to the tray as `WM_VOLUME_OSD_MOVED` when a drag ends.
pub fn volume_osd_set_positioning(active: bool) {
    if active {
        send(Command::Positioning(true));
    } else {
        send_if_running(Command::Positioning(false));
    }
}

/// Where the last drag left the card: (display id, x %, y %).
pub fn take_moved_position() -> Option<(String, f32, f32)> {
    MOVED.lock().unwrap().take()
}

static MOVED: Lazy<Mutex<Option<(String, f32, f32)>>> = Lazy::new(|| Mutex::new(None));

pub(crate) fn set_positioning_native(active: bool) {
    let hwnd = {
        let mut guard = OSD.lock().unwrap();
        let Some(osd) = guard.as_mut() else {
            return;
        };
        osd.positioning = active;
        osd.hwnd
    };
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let click_through = WS_EX_TRANSPARENT.0 as i32;
        SetWindowLongW(
            hwnd,
            GWL_EXSTYLE,
            if active {
                ex_style & !click_through
            } else {
                ex_style | click_through
            },
        );
    }
}

/// The media watcher read a different track or state. A running OSD updates
/// its now-playing strip in place.
pub(crate) fn media_changed() {
    send_if_running(Command::Media);
}

/// No-op when the OSD thread was never started.
pub fn volume_osd_hide() {
    send_if_running(Command::Hide);
}

/// Popup commands share the OSD thread. Showing starts it; the rest only
/// matter while it already runs.
pub(crate) fn send_popup(command: crate::volume_popup::PopupCommand) {
    if matches!(
        command,
        crate::volume_popup::PopupCommand::Show { .. } | crate::volume_popup::PopupCommand::Prewarm
    ) {
        send(Command::Popup(command));
    } else {
        send_if_running(Command::Popup(command));
    }
}

/// Restyle/hide must not spin up a GPUI thread for an OSD nobody has shown.
fn send_if_running(command: Command) {
    if let Some(runtime) = RUNTIME.lock().unwrap().as_ref() {
        let _ = runtime.sender.send(command);
    }
}

/// Hidden CLI: cycle levels, mute, and a device change, then exit.
pub fn run_demo() -> Result<()> {
    let themes: Vec<String> = std::env::args()
        .skip(1)
        .filter(|arg| arg != "--volume-osd-demo")
        .collect();
    let (ready_tx, ready_rx) = flume::bounded(1);
    *READY.lock().unwrap() = Some(ready_tx);
    volume_osd_init()?;
    let _ = ready_rx.recv_timeout(Duration::from_secs(10));
    // The HWND exists. Give the first off-screen warmup paint a moment.
    thread::sleep(Duration::from_millis(250));

    let mut style = VolumeOsdStyle::default();
    style.duration_ms = 20_000;
    for step in 0..=20 {
        let percent = (step * 5) as u8;
        volume_osd_show(
            VolumeOsdContent::Level {
                percent,
                muted: false,
            },
            &style,
        );
        let pause = if percent == 0 || percent == 100 { 900 } else { 40 };
        thread::sleep(Duration::from_millis(pause));
    }
    volume_osd_show(
        VolumeOsdContent::Level {
            percent: 40,
            muted: true,
        },
        &style,
    );
    thread::sleep(Duration::from_millis(1_000));
    volume_osd_show(
        VolumeOsdContent::Device {
            name: "Headphones (Realtek(R) Audio)".into(),
            kind: DeviceKind::Headphones,
            percent: 62,
            muted: false,
        },
        &style,
    );
    thread::sleep(Duration::from_millis(1_200));
    volume_osd_show(
        VolumeOsdContent::Device {
            name: "Speakers (High Definition Audio Device)".into(),
            kind: DeviceKind::Speakers,
            percent: 28,
            muted: false,
        },
        &style,
    );
    thread::sleep(Duration::from_millis(1_200));
    for theme in themes {
        style.theme = theme;
        volume_osd_apply_style(&style);
        thread::sleep(Duration::from_millis(900));
    }
    volume_osd_hide();
    thread::sleep(Duration::from_millis(600));
    shutdown();
    Ok(())
}

fn send(command: Command) {
    if let Err(error) = volume_osd_init() {
        eprintln!("volume OSD: {error:#}");
        return;
    }
    if let Some(runtime) = RUNTIME.lock().unwrap().as_ref() {
        let _ = runtime.sender.send(command);
    }
}

fn shutdown() {
    let thread = {
        let mut runtime = RUNTIME.lock().unwrap();
        let Some(runtime) = runtime.as_mut() else {
            return;
        };
        runtime.stopping.store(true, Ordering::Release);
        let _ = runtime.sender.send(Command::Shutdown);
        runtime.thread.take()
    };
    if let Some(thread) = thread {
        let _ = thread.join();
    }
}

pub(crate) fn resolve_display(requested: &str) -> String {
    if requested.eq_ignore_ascii_case("cursor") {
        return monitor_under_cursor();
    }
    if requested.is_empty() {
        return crate::OVERLAY_DISPLAY_PRIMARY.to_string();
    }
    requested.to_string()
}

pub(crate) fn set_placement(display: String, x: f32, y: f32) {
    let mut guard = OSD.lock().unwrap();
    let Some(osd) = guard.as_mut() else {
        return;
    };
    osd.display = display;
    osd.position_x = x.clamp(0.0, 100.0);
    osd.position_y = y.clamp(0.0, 100.0);
    osd.apply_layout();
}

pub(crate) fn set_geometry(
    card_width: i32,
    card_height: i32,
    surface_width: i32,
    surface_height: i32,
    inset_x: i32,
    inset_y: i32,
    slide: f32,
    slide_travel: Option<i32>,
    held: Option<(i32, i32)>,
) -> (i32, i32) {
    let mut guard = OSD.lock().unwrap();
    let Some(osd) = guard.as_mut() else {
        return (inset_x, inset_y);
    };
    osd.card_width = card_width.max(1);
    osd.card_height = card_height.max(1);
    osd.surface_width = surface_width.max(1);
    osd.surface_height = surface_height.max(1);
    osd.inset_x = inset_x.max(0);
    osd.inset_y = inset_y.max(0);
    osd.slide = slide.clamp(0.0, 1.0);
    osd.slide_travel = slide_travel;
    osd.held = held.map(|(width, height)| (width.max(card_width), height.max(card_height)));
    osd.apply_layout();
    osd.origin
}

/// Screen rectangles (physical pixels) of the monitor and of a card of this
/// size once it has slid in, for sampling the wallpaper under it.
pub(crate) fn card_rect(width: i32, height: i32) -> Option<(RECT, RECT)> {
    let guard = OSD.lock().unwrap();
    let osd = guard.as_ref()?;
    let monitor = selected_monitor(&osd.display);
    let rect = monitor.rect;
    let width = width.max(1);
    let height = height.max(1);
    let screen_w = (rect.right - rect.left).max(width);
    let screen_h = (rect.bottom - rect.top).max(height);
    let x = rect.left + percent_to_axis(osd.position_x, screen_w, width);
    let y = rect.top + percent_to_axis(osd.position_y, screen_h, height);
    Some((
        rect,
        RECT {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        },
    ))
}

pub(crate) fn present(visible: bool) {
    let mut guard = OSD.lock().unwrap();
    let Some(osd) = guard.as_mut() else {
        return;
    };
    if osd.presented == visible {
        return;
    }
    osd.presented = visible;
    if osd.hwnd.0.is_null() {
        return;
    }
    unsafe {
        let _ = PostMessageW(osd.hwnd, WM_VOLUME_PRESENT, WPARAM(0), LPARAM(0));
    }
}

pub(crate) fn set_card_shown(shown: bool) {
    CARD_SHOWN.store(shown, Ordering::Relaxed);
}

/// The window stays click-through, so the tray's mouse hook asks this. Only
/// the card counts, not the shadow gutter around it, and never during "Move OSD".
pub(crate) fn card_hit(point: POINT) -> bool {
    if !CARD_SHOWN.load(Ordering::Relaxed) {
        return false;
    }
    let guard = OSD.lock().unwrap();
    let Some(osd) = guard.as_ref() else {
        return false;
    };
    if osd.hwnd.0.is_null() || osd.positioning || osd.dragging || !osd.presented {
        return false;
    }
    let Some((x, y, _, _)) = osd.last_surface else {
        return false;
    };
    let left = x + osd.origin.0;
    let top = y + osd.origin.1;
    point.x >= left
        && point.x < left + osd.card_width
        && point.y >= top
        && point.y < top + osd.card_height
}

/// `true` while a fullscreen app (a game, a fullscreen video) covers the
/// monitor the OSD shows on.
pub(crate) fn over_fullscreen() -> bool {
    let center = {
        let guard = OSD.lock().unwrap();
        let Some(osd) = guard.as_ref() else {
            return false;
        };
        let rect = selected_monitor(&osd.display).rect;
        POINT {
            x: rect.left + (rect.right - rect.left) / 2,
            y: rect.top + (rect.bottom - rect.top) / 2,
        }
    };
    // The lock is released first: the query makes cross-process calls.
    crate::wheel_volume::fullscreen_blocks_wheel(center)
}

pub(crate) fn monitor_scale() -> f32 {
    OSD.lock()
        .unwrap()
        .as_ref()
        .map(NativeOsd::dpi_scale)
        .unwrap_or(1.0)
}

fn prepare(sender: Sender<Command>) -> Result<()> {
    *OSD.lock().unwrap() = Some(NativeOsd {
        hwnd: HWND::default(),
        sender,
        display: crate::OVERLAY_DISPLAY_PRIMARY.to_string(),
        position_x: 50.0,
        position_y: 92.0,
        card_width: 194,
        card_height: 49,
        surface_width: 194,
        surface_height: 49,
        inset_x: 0,
        inset_y: 0,
        slide: 1.0,
        slide_travel: None,
        held: None,
        origin: (0, 0),
        presented: false,
        awaiting_first_paint: true,
        last_surface: None,
        positioning: false,
        dragging: false,
    });
    Ok(())
}

pub(crate) fn attach(hwnd: HWND) -> Result<()> {
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_STYLE);
        let decorations =
            (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX).0;
        SetWindowLongW(
            hwnd,
            GWL_STYLE,
            (style & !(decorations as i32)) | WS_POPUP.0 as i32,
        );
        let policy = DWMNCRP_DISABLED;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY,
            &policy as *const _ as _,
            size_of_val(&policy) as u32,
        )?;
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let click_through =
            (WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW).0 as i32;
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | click_through);
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
        apply_frame(hwnd);
        let previous = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, volume_wnd_proc as *const () as _);
        anyhow::ensure!(previous != 0, "subclass volume OSD window");
        ORIGINAL_WNDPROC.store(previous as isize, Ordering::Release);
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )?;
    }
    {
        let mut guard = OSD.lock().unwrap();
        let osd = guard.as_mut().context("volume OSD control state missing")?;
        osd.hwnd = hwnd;
        osd.last_surface = None;
        osd.presented = false;
        osd.awaiting_first_paint = true;
        osd.apply_layout();
    }
    if let Some(sender) = READY.lock().unwrap().take() {
        let _ = sender.send(());
    }
    Ok(())
}

fn detach() {
    if let Some(osd) = OSD.lock().unwrap().as_mut() {
        osd.hwnd = HWND::default();
        osd.presented = false;
        osd.last_surface = None;
    }
}

pub(crate) fn apply_frame(hwnd: HWND) {
    unsafe {
        let _ = DwmExtendFrameIntoClientArea(
            hwnd,
            &MARGINS {
                cxLeftWidth: 0,
                cxRightWidth: 0,
                cyTopHeight: 0,
                cyBottomHeight: 0,
            },
        );
        // DWMWA_COLOR_NONE. The card paints its own rounded fill.
        let color: u32 = 0xFFFF_FFFE;
        for attribute in [DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR] {
            let _ = DwmSetWindowAttribute(
                hwnd,
                attribute,
                &color as *const _ as _,
                size_of::<u32>() as u32,
            );
        }
        let dark_mode = 1i32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_mode as *const _ as _,
            size_of::<i32>() as u32,
        );
        let corners = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corners as *const _ as _,
            size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );
        let backdrop = DWMSBT_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as _,
            size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        );
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

struct NativeOsd {
    hwnd: HWND,
    sender: Sender<Command>,
    display: String,
    position_x: f32,
    position_y: f32,
    card_width: i32,
    card_height: i32,
    surface_width: i32,
    surface_height: i32,
    inset_x: i32,
    inset_y: i32,
    slide: f32,
    /// Slide this many pixels instead of all the way from beyond the edge.
    slide_travel: Option<i32>,
    /// Largest card (width, height) the window is kept sized for. With it
    /// the window stays put and the card moves inside, like the slider
    /// popup, instead of moving or resizing every animation frame.
    held: Option<(i32, i32)>,
    /// The card's top left corner inside the window.
    origin: (i32, i32),
    presented: bool,
    awaiting_first_paint: bool,
    last_surface: Option<(i32, i32, i32, i32)>,
    /// "Move OSD" is on: the window takes the mouse and drags like a caption.
    positioning: bool,
    /// A drag is in progress; layout must not move the window under it.
    dragging: bool,
}

unsafe impl Send for NativeOsd {}

impl NativeOsd {
    fn apply_layout(&mut self) {
        if self.hwnd.0.is_null() || self.dragging {
            return;
        }
        let monitor = selected_monitor(&self.display);
        let rect = monitor.rect;
        let screen_w = (rect.right - rect.left).max(self.card_width);
        let screen_h = (rect.bottom - rect.top).max(self.card_height);
        let x = rect.left + percent_to_axis(self.position_x, screen_w, self.card_width);
        let y = rect.top + percent_to_axis(self.position_y, screen_h, self.card_height);
        let slide = self.slide_offset(y, &rect);
        let bounds = match (self.held, self.slide_travel) {
            (Some((held_w, held_h)), Some(travel)) => {
                // The same placement rule for the larger card always
                // contains the current one; the slide happens inside.
                let held_x = rect.left
                    + percent_to_axis(self.position_x, screen_w.max(held_w), held_w);
                let held_y = rect.top
                    + percent_to_axis(self.position_y, screen_h.max(held_h), held_h);
                let left = held_x - self.inset_x;
                let top = held_y - self.inset_y - travel;
                self.origin = (x - left, y + slide - top);
                (
                    left,
                    top,
                    held_w + self.inset_x * 2,
                    held_h + (self.inset_y + travel) * 2,
                )
            }
            _ => {
                self.origin = (self.inset_x, self.inset_y);
                (
                    x - self.inset_x,
                    y - self.inset_y + slide,
                    self.surface_width,
                    self.surface_height,
                )
            }
        };
        if self.last_surface == Some(bounds) {
            return;
        }
        self.last_surface = Some(bounds);
        unsafe {
            let _ = PostMessageW(self.hwnd, WM_VOLUME_SURFACE, WPARAM(0), LPARAM(0));
        }
    }

    fn slide_offset(&self, card_y: i32, rect: &RECT) -> i32 {
        if self.slide <= 0.0 {
            return 0;
        }
        let surface_top = card_y - self.inset_y;
        let surface_bottom = surface_top + self.surface_height;
        let from_top = surface_top - rect.top < rect.bottom - surface_bottom;
        let distance = match (self.slide_travel, from_top) {
            (Some(travel), true) => -travel,
            (Some(travel), false) => travel,
            (None, true) => rect.top - surface_bottom,
            (None, false) => rect.bottom - surface_top,
        };
        (distance as f32 * self.slide).round() as i32
    }

    fn dpi_scale(&self) -> f32 {
        let monitor = selected_monitor(&self.display);
        if !monitor.handle.0.is_null() {
            return monitor_dpi_scale(monitor.handle);
        }
        if self.hwnd.0.is_null() {
            return 1.0;
        }
        let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96);
        dpi as f32 / 96.0
    }
}

fn percent_to_axis(percent: f32, screen: i32, size: i32) -> i32 {
    let available = (screen - size).max(0) as f32;
    (available * percent.clamp(0.0, 100.0) / 100.0).round() as i32
}

fn selected_monitor(display: &str) -> MonitorRect {
    let monitors = monitor_rects();
    let primary_index = monitors
        .iter()
        .position(|monitor| monitor.primary)
        .unwrap_or(0);
    if display == crate::OVERLAY_DISPLAY_PRIMARY || display.is_empty() {
        return monitors.into_iter().nth(primary_index).unwrap_or_default();
    }
    if let Some(monitor) = monitors.iter().find(|monitor| monitor.id == display) {
        return monitor.clone();
    }
    monitors.into_iter().nth(primary_index).unwrap_or_default()
}

fn monitor_under_cursor() -> String {
    let mut cursor = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut cursor);
    }
    let handle = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };
    let monitors = monitor_rects();
    monitors
        .into_iter()
        .find(|monitor| monitor.handle == handle)
        .map(|monitor| {
            if monitor.primary {
                crate::OVERLAY_DISPLAY_PRIMARY.to_string()
            } else {
                monitor.id
            }
        })
        .unwrap_or_else(|| crate::OVERLAY_DISPLAY_PRIMARY.to_string())
}

fn monitor_rects() -> Vec<MonitorRect> {
    let mut monitors = Vec::<MonitorRect>::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(collect_monitor_rect),
            LPARAM(&mut monitors as *mut _ as isize),
        );
    }
    monitors
}

#[derive(Clone, Default)]
struct MonitorRect {
    handle: HMONITOR,
    id: String,
    rect: RECT,
    primary: bool,
}

unsafe extern "system" fn collect_monitor_rect(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let monitors = unsafe { &mut *(data.0 as *mut Vec<MonitorRect>) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        monitors.push(MonitorRect {
            handle: monitor,
            id: format!("Monitor{}", monitors.len() + 1),
            rect: info.rcMonitor,
            primary: (info.dwFlags & 1) != 0,
        });
    }
    true.into()
}

fn monitor_dpi_scale(monitor: HMONITOR) -> f32 {
    let mut dpi_x = 96u32;
    let mut dpi_y = 96u32;
    if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }.is_err() {
        return 1.0;
    }
    dpi_x.max(dpi_y).max(96) as f32 / 96.0
}

/// Turn the dropped window position into a monitor and percentages, store it
/// for the tray, and let layout take over again.
fn finish_drag(hwnd: HWND) {
    let mut window = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut window) }.is_err() {
        return;
    }
    let tray = {
        let mut guard = OSD.lock().unwrap();
        let Some(osd) = guard.as_mut() else {
            return;
        };
        osd.dragging = false;
        let card_left = window.left + osd.origin.0;
        let card_top = window.top + osd.origin.1;
        let center = POINT {
            x: card_left + osd.card_width / 2,
            y: card_top + osd.card_height / 2,
        };
        let monitors = monitor_rects();
        let Some(monitor) = monitors
            .iter()
            .find(|monitor| {
                let r = monitor.rect;
                center.x >= r.left && center.x < r.right && center.y >= r.top && center.y < r.bottom
            })
            .or_else(|| monitors.first())
        else {
            return;
        };
        let rect = monitor.rect;
        let span_x = ((rect.right - rect.left) - osd.card_width).max(1) as f32;
        let span_y = ((rect.bottom - rect.top) - osd.card_height).max(1) as f32;
        let x = ((card_left - rect.left) as f32 / span_x * 100.0).clamp(0.0, 100.0);
        let y = ((card_top - rect.top) as f32 / span_y * 100.0).clamp(0.0, 100.0);
        // "Monitor under cursor" stays that way; a fixed monitor follows the drop.
        if !osd.display.eq_ignore_ascii_case(crate::VOLUME_OSD_DISPLAY_CURSOR) {
            osd.display = if monitor.primary {
                crate::OVERLAY_DISPLAY_PRIMARY.to_string()
            } else {
                monitor.id.clone()
            };
        }
        osd.position_x = x;
        osd.position_y = y;
        *MOVED.lock().unwrap() = Some((osd.display.clone(), x, y));
        let tray = crate::STATE.lock().unwrap_or_else(|p| p.into_inner()).hwnd;
        (!tray.0.is_null()).then_some(tray)
    };
    if let Some(tray) = tray {
        unsafe {
            let _ = PostMessageW(tray, crate::WM_VOLUME_OSD_MOVED, WPARAM(0), LPARAM(0));
        }
    }
}

/// The OSD's window while it is on screen. The mic overlay stacks itself
/// right under it, so the two always keep the same order.
pub(crate) fn visible_hwnd() -> Option<HWND> {
    let hwnd = OSD.lock().unwrap().as_ref().map(|osd| osd.hwnd)?;
    (!hwnd.0.is_null() && unsafe { IsWindowVisible(hwnd).as_bool() }).then_some(hwnd)
}

fn restore_topmost(hwnd: HWND) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

unsafe extern "system" fn volume_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCALCSIZE => LRESULT(0),
        WM_PAINT => {
            let result = unsafe { forward_window_message(hwnd, msg, wparam, lparam) };
            if !unsafe { IsWindowVisible(hwnd).as_bool() } {
                let mut paint = PAINTSTRUCT::default();
                unsafe {
                    BeginPaint(hwnd, &mut paint);
                    let _ = EndPaint(hwnd, &paint);
                }
            } else if let Some(osd) = OSD.lock().unwrap().as_mut() {
                if osd.awaiting_first_paint && osd.presented {
                    osd.awaiting_first_paint = false;
                    let _ = osd.sender.send(Command::SurfaceReady);
                }
            }
            result
        }
        WM_VOLUME_SURFACE => {
            let bounds = OSD
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|osd| osd.last_surface);
            if let Some((x, y, width, height)) = bounds {
                unsafe {
                    // Position only. Stacking belongs to `restore_topmost`:
                    // raising here on every animation frame would hop over
                    // the mic overlay and back.
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        x,
                        y,
                        width,
                        height,
                        SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
                    );
                }
            }
            LRESULT(0)
        }
        WM_VOLUME_PRESENT => {
            let visible = OSD
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|osd| osd.presented);
            unsafe {
                let _ = ShowWindow(
                    hwnd,
                    if visible {
                        SW_SHOWNOACTIVATE
                    } else {
                        SW_HIDE
                    },
                );
                if visible {
                    restore_topmost(hwnd);
                    if SetTimer(hwnd, ID_TOPMOST_TIMER, TOPMOST_INTERVAL_MS, None) == 0 {
                        eprintln!("volume OSD: failed to start topmost timer");
                    }
                } else {
                    let _ = KillTimer(hwnd, ID_TOPMOST_TIMER);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        // Click-through, including the shadow gutter, unless "Move OSD" is
        // on: then the whole card is a caption the system drags for us.
        WM_NCHITTEST => {
            let positioning = OSD
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|osd| osd.positioning);
            LRESULT(if positioning { HTCAPTION as isize } else { HTTRANSPARENT as isize })
        }
        WM_ENTERSIZEMOVE => {
            if let Some(osd) = OSD.lock().unwrap().as_mut() {
                osd.dragging = true;
            }
            unsafe { forward_window_message(hwnd, msg, wparam, lparam) }
        }
        WM_EXITSIZEMOVE => {
            finish_drag(hwnd);
            unsafe { forward_window_message(hwnd, msg, wparam, lparam) }
        }
        WM_TIMER if wparam.0 == ID_TOPMOST_TIMER => {
            if unsafe { IsWindowVisible(hwnd).as_bool() }
                && crate::native_overlay::covered_by_foreign_window(hwnd)
            {
                restore_topmost(hwnd);
            }
            LRESULT(0)
        }
        _ => unsafe { forward_window_message(hwnd, msg, wparam, lparam) },
    }
}

unsafe fn forward_window_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let previous = ORIGINAL_WNDPROC.load(Ordering::Acquire);
    if previous == 0 {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    unsafe {
        CallWindowProcW(
            Some(std::mem::transmute::<
                isize,
                unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
            >(previous)),
            hwnd,
            msg,
            wparam,
            lparam,
        )
    }
}
