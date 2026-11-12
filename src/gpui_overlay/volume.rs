//! Volume flyout renderer. Runs on `silence-gpui-volume-osd`, not the mic thread.
//!
//! Solar Bold only ships three speaker glyphs (no waves, one wave, two waves)
//! plus a crossed speaker. Level 0 uses the bare speaker, 1–33 the single wave,
//! and both mid and high (34–100) use `volume-loud-bold`, the fullest glyph.
//! Only the Cute themes draw these; the rest use the Fluent font's glyphs.
//! Cute Sticker's peel does not fit a meter, so that theme is a die-cut paper
//! pill with a badge around the icon, in the mic sticker's pastel ink.
//!
//! While something plays, a now-playing strip (cover, title, artist) can sit
//! above the meter inside the same card. The music widget (a media key, or
//! the playlist moving on) is its own layout, like a device switch: cover,
//! title and artist, with the action as a small badge on the cover's corner
//! in the theme's icon colors. Both
//! take every color, font and corner from the theme's chrome, so no theme
//! needs its own media layout.

use super::{
    details, fonts,
    motion::Motion,
    theme::{Detail, IconBox, Look, Shadow, System, color_alpha, from_hsl, hsl},
};
use crate::volume_osd::{Command, DeviceKind, MediaAction, VolumeOsdContent, VolumeOsdStyle};
use anyhow::Result;
use gpui::{
    App, AssetSource, Bounds, BoxShadow, Context, Div, FontWeight, Image, IntoElement, Render, Rgba,
    SharedString, Task, TextRun, Window, WindowBackgroundAppearance, WindowBounds, WindowKind,
    WindowOptions, WindowingRequest, div, font, linear_color_stop, linear_gradient, point,
    ObjectFit, Transformation, img, prelude::*, px, radians, rgb, size, svg,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    borrow::Cow,
    cell::RefCell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use windows::Win32::Foundation::HWND;

const LEVEL_MS: u64 = 120;
/// Bar-to-number gap as a share of the chrome gap. The number is centered in
/// a slot sized for "100", which already leaves air on both sides.
const NUMBER_GAP: f32 = 0.6;
/// Device names get room to show in full; only very long ones are cut.
const NAME_MAX_WIDTH: f32 = 440.0;
const CROSSFADE_MS: u64 = 200;
const SIZE_MS: u64 = 240;
const FADE_MS: u64 = 180;
const SLIDE_MS: u64 = 280;
/// Album art edge in the strip above the meter, before the size scale.
const COVER: f32 = 44.0;
/// Album art edge in the music widget.
const WIDGET_COVER: f32 = 44.0;
/// Action badge on the widget's cover, and the card-colored ring around it.
const MEDIA_BADGE: f32 = 20.0;
const MEDIA_BADGE_RING: f32 = 2.0;
/// Title and artist get this much room before they are cut.
const MEDIA_TEXT_MAX: f32 = 220.0;
/// Space between the title and the artist line.
const MEDIA_LINE_GAP: f32 = 2.0;
/// A track change: the old lines leave to the left while the new ones come
/// in from the right, and the covers shrink to half and grow from half.
const TRACK_MS: u64 = 320;
const TRACK_SHIFT: f32 = 18.0;
/// "Up next" holds the whole text column for a beat, then swipes up and the
/// track comes up under it.
const UP_NEXT_HOLD_MS: f32 = 800.0;
const UP_NEXT_SWIPE_MS: f32 = 380.0;
/// How fast the chevrons behind "Up next" drift forward, px per second.
const UP_NEXT_DRIFT: f32 = 36.0;
/// Ambient themes tick at ~30 fps, same as the mic overlay.
const AMBIENT_FRAME: Duration = Duration::from_millis(33);

struct VolumeAssets;

impl AssetSource for VolumeAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(icon_svg(path).map(|svg| Cow::Borrowed(svg.as_bytes())))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

pub(crate) fn run(receiver: flume::Receiver<Command>) -> Result<()> {
    let startup_error = Rc::new(RefCell::new(None));
    let app_error = startup_error.clone();
    let commands: Rc<RefCell<Option<Task<()>>>> = Rc::new(RefCell::new(None));
    let app_commands = commands.clone();
    let platform = Rc::new(gpui_windows::WindowsPlatform::new(false)?);
    gpui::Application::with_platform(platform)
        .with_quit_mode(gpui::QuitMode::Explicit)
        .with_assets(VolumeAssets)
        .run(move |cx: &mut App| {
            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(80.), px(80.)),
                        // GPUI never grows a non-resizable window past its
                        // initial size: leave room for long device names and
                        // the now-playing strip at the 400% size scale. The
                        // native surface is resized to the card every layout.
                        size(px(2000.), px(900.)),
                    ))),
                    titlebar: None,
                    inactive_frame_interval: None,
                    focus: false,
                    show: false,
                    kind: WindowKind::PopUp,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                |_window, cx| cx.new(|_| VolumeView::new()),
            );
            let result = opened.and_then(|handle| {
                // Attach on the App. Spawn stays outside this closure: `cx` in
                // here would be Context, whose spawn takes a weak entity.
                handle.update(cx, |view, window, _| -> Result<()> {
                    let RawWindowHandle::Win32(raw) =
                        HasWindowHandle::window_handle(window)?.as_raw()
                    else {
                        anyhow::bail!("volume OSD requires a Windows window");
                    };
                    let hwnd = HWND(raw.hwnd.get() as *mut _);
                    crate::volume_osd::attach(hwnd)?;
                    view.backdrop = match super::backdrop::Backdrop::new(
                        hwnd,
                        glass_luminosity(view.system.light),
                        super::backdrop::Material::Acrylic,
                    ) {
                        Ok(backdrop) => Some(backdrop),
                        Err(error) => {
                            // The blurred-wallpaper fallback stays in use.
                            eprintln!("volume OSD: no acrylic backdrop: {error:#}");
                            None
                        }
                    };
                    Ok(())
                })??;
                // The slider popup's window too, so the first click is instant.
                super::volume_popup::handle(crate::volume_popup::PopupCommand::Prewarm, cx);
                *app_commands.borrow_mut() = Some(cx.spawn(async move |cx| {
                        while let Ok(command) = receiver.recv_async().await {
                            let mut batch = vec![command];
                            while let Ok(command) = receiver.try_recv() {
                                batch.push(command);
                            }
                            let quit = batch.iter().any(|command| {
                                matches!(command, Command::Shutdown)
                            });
                            let (popup, batch): (Vec<_>, Vec<_>) = batch
                                .into_iter()
                                .partition(|command| matches!(command, Command::Popup(_)));
                            for command in popup {
                                if let Command::Popup(command) = command {
                                    let _ = cx.update(|cx| super::volume_popup::handle(command, cx));
                                }
                            }
                            let applied = handle.update(cx, |view, window, cx| {
                                for command in batch {
                                    if !matches!(command, Command::Shutdown) {
                                        view.on_command(command, cx);
                                    }
                                }
                                if quit {
                                    view.hide_wait = None;
                                    window.remove_window();
                                    cx.defer(|cx| {
                                        cx.spawn(async move |cx| {
                                            cx.background_executor()
                                                .timer(Duration::from_millis(120))
                                                .await;
                                            let _ = cx.update(|cx| {
                                                cx.request_windowing(WindowingRequest::Headless)
                                                    .detach();
                                                cx.quit();
                                            });
                                        })
                                        .detach();
                                    });
                                }
                            });
                            if quit || applied.is_err() {
                                if applied.is_err() {
                                    let _ = cx.update(|cx| cx.quit());
                                }
                                break;
                            }
                        }
                }));
                Ok(())
            });
            if let Err(error) = result {
                *app_error.borrow_mut() = Some(error);
                cx.quit();
            }
        });
    commands.borrow_mut().take();
    let error = startup_error.borrow_mut().take();
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Entrance {
    Fade,
    /// Travels in from beyond the nearest top or bottom screen edge.
    Slide,
    /// Fades in while settling the last few pixels, coming from the side of
    /// the nearest top or bottom screen edge.
    Nudge,
}

/// How far a `Nudge` entrance travels.
const NUDGE_DISTANCE: f32 = 10.0;

#[derive(Clone)]
struct VolumeChrome {
    entrance: Entrance,
    detail: Detail,
    animates: bool,
    halo_pulse: bool,
    height: f32,
    radius: f32,
    pad_x: f32,
    gap: f32,
    icon_size: f32,
    icon_box: Option<IconBox>,
    icon_color: u32,
    surface: Rgba,
    foreground: u32,
    track: Rgba,
    fill: Rgba,
    border: Option<(Rgba, f32)>,
    shadow: Shadow,
    gutter: f32,
    font: SharedString,
    weight: u16,
    text_size: f32,
    sheen: f32,
    bar_width: f32,
    bar_height: f32,
    ink: u32,
    /// Draw the speaker/device glyph from Segoe Fluent Icons (the system
    /// volume flyout's own glyphs) instead of the Solar SVGs.
    fluent: bool,
    /// Glass card: real acrylic under a thin `surface` tint, or the blurred
    /// wallpaper where the system has no backdrop brush.
    mica: bool,
    /// Width of the number's area from the bar's end to the card's edge; the
    /// number is centered in it. `None` sizes it from the text and `pad_x`.
    number_slot: Option<f32>,
    /// One physical pixel hairline just outside the card.
    outline: Option<Rgba>,
    /// Whole-card opacity (Custom theme's "Opacity").
    opacity: f32,
    /// Corner radius of the meter; `None` draws a capsule.
    bar_radius: Option<f32>,
    /// Die-cut sticker: a paper badge bulges around the icon and sparkles
    /// hang over the edge.
    sticker: bool,
    /// Rotation of an SVG glyph, in radians.
    icon_tilt: f32,
}

impl VolumeChrome {
    /// Left inset of the icon. A boxed icon sits as far from the left edge
    /// as from the top and bottom ones.
    fn lead(&self) -> f32 {
        match self.icon_box {
            Some(icon_box) => ((self.height - icon_box.size) / 2.0).max(0.0),
            None => self.pad_x,
        }
    }

    /// Right inset of a device name. Text needs more air than the icon, and
    /// has to clear the curve of a rounded end.
    fn text_pad(&self) -> f32 {
        let clearance = if self.icon_box.is_some() { 8.0 } else { 4.0 };
        (self.lead() + clearance).max(self.radius.min(self.height / 2.0) * 0.8)
    }
}

/// White rim of the Cute Sticker badge around its icon disc.
const STICKER_RIM: f32 = 8.0;

struct ChromeLayer {
    key: (String, bool),
    chrome: VolumeChrome,
    opacity: Motion,
}

struct Glyph {
    path: SharedString,
    color: u32,
    opacity: Motion,
}

struct Caption {
    text: SharedString,
    opacity: Motion,
}

/// One track in the now-playing strip. A track change crossfades two of these.
#[derive(Clone)]
struct MediaLayer {
    key: String,
    title: SharedString,
    artist: SharedString,
    cover: Option<Arc<Image>>,
    /// The playlist moved on: "Up next" plays before the track's lines.
    eyebrow: bool,
    /// Replaced another track (or is being replaced): swipe, not just fade.
    swipe: bool,
    /// When the "Up next" intro started.
    intro: Option<Instant>,
    /// Widest of the fitted lines, in window pixels.
    text_width: f32,
    opacity: Motion,
}

impl MediaLayer {
    fn intro_active(&self, now: Instant) -> bool {
        self.intro.is_some_and(|start| {
            now.saturating_duration_since(start).as_secs_f32() * 1000.0
                < UP_NEXT_HOLD_MS + UP_NEXT_SWIPE_MS
        })
    }

    /// Milliseconds since the "Up next" intro started.
    fn intro_elapsed(&self, now: Instant) -> f32 {
        self.intro
            .map(|start| now.saturating_duration_since(start).as_secs_f32() * 1000.0)
            .unwrap_or(f32::MAX)
    }

    /// Progress of the "Up next" swipe: 0 while it holds, 1 once the track's
    /// lines are up. Tracks without the intro are always at 1.
    fn intro_progress(&self, now: Instant) -> f32 {
        if self.intro.is_none() {
            return 1.0;
        }
        let t = ((self.intro_elapsed(now) - UP_NEXT_HOLD_MS) / UP_NEXT_SWIPE_MS).clamp(0.0, 1.0);
        // Ease in and out: the label lifts off, then the track settles.
        if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        }
    }

    /// Cover scale and sideways share of `TRACK_SHIFT` for a track swiping in
    /// (from the right) or out (to the left) at `opacity`.
    fn swipe(&self, opacity: f32) -> (f32, f32) {
        if !self.swipe {
            return (1.0, 0.0);
        }
        let away = 1.0 - opacity.clamp(0.0, 1.0);
        let direction = if self.opacity.to > 0.5 { 1.0 } else { -1.0 };
        (1.0 - 0.5 * away, direction * away)
    }
}

struct VolumeView {
    system: System,
    content: VolumeOsdContent,
    style: VolumeOsdStyle,
    device_name: Option<String>,
    caption_dirty: bool,
    layers: Vec<ChromeLayer>,
    glyphs: Vec<Glyph>,
    captions: Vec<Caption>,
    media: Vec<MediaLayer>,
    /// Re-read the playing track and re-fit its text on the next frame.
    media_dirty: bool,
    /// Height of the now-playing strip above the meter row.
    media_height: Motion,
    /// The card is appearing from nothing: take the new size without growing.
    snap_layout: bool,
    loaded_fonts: Vec<SharedString>,
    needs_measure: bool,
    measured: bool,
    level_ready: bool,
    width: Motion,
    height: Motion,
    radius: Motion,
    level: Motion,
    visibility: Motion,
    slide: Motion,
    want_visible: bool,
    /// "Move OSD": stays up, no hide timer.
    positioning: bool,
    surface_ready: bool,
    fallback_armed: bool,
    background_set: bool,
    /// Real acrylic under the window; `None` where it is unavailable.
    backdrop: Option<super::backdrop::Backdrop>,
    /// A fullscreen app owns the OSD's monitor. Windows scans such an app out
    /// on its own hardware plane: the acrylic brush cannot sample it and the
    /// glass shows a square corner there, so the card is painted solid.
    over_fullscreen: bool,
    /// Largest card size at this scale; see `volume_osd::set_geometry`.
    held: (f32, f32),
    last_scale: f32,
    hide_wait: Option<Task<()>>,
    ambient_frame: Option<Task<()>>,
    detail_epoch: Instant,
}

impl VolumeView {
    fn new() -> Self {
        let system = System::load();
        let chrome = chrome_for("Windows", system, false, &VolumeOsdStyle::default());
        Self {
            system,
            content: VolumeOsdContent::Level {
                percent: 0,
                muted: false,
            },
            style: VolumeOsdStyle::default(),
            device_name: None,
            caption_dirty: true,
            layers: vec![ChromeLayer {
                key: ("Windows".to_string(), false),
                chrome,
                opacity: Motion::new(1.0, CROSSFADE_MS),
            }],
            glyphs: Vec::new(),
            captions: Vec::new(),
            media: Vec::new(),
            media_dirty: true,
            media_height: Motion::new(0.0, SIZE_MS),
            snap_layout: false,
            loaded_fonts: Vec::new(),
            needs_measure: true,
            measured: false,
            level_ready: false,
            width: Motion::new(192.0, SIZE_MS),
            height: Motion::new(48.0, SIZE_MS),
            radius: Motion::new(8.0, SIZE_MS),
            level: Motion::new(0.0, LEVEL_MS),
            visibility: Motion::new(0.0, FADE_MS),
            slide: Motion::decelerate(1.0, SLIDE_MS),
            want_visible: false,
            positioning: false,
            surface_ready: false,
            fallback_armed: false,
            background_set: false,
            backdrop: None,
            over_fullscreen: false,
            held: (0.0, 0.0),
            last_scale: 0.0,
            hide_wait: None,
            ambient_frame: None,
            detail_epoch: Instant::now(),
        }
    }

    fn on_command(&mut self, command: Command, cx: &mut Context<Self>) {
        match command {
            Command::Show { content, style } => self.show(content, style, cx),
            Command::Style(style) => self.apply_style(style, cx),
            Command::Hide => self.hide(cx),
            Command::SurfaceReady => self.on_surface_ready(cx),
            Command::Positioning(active) => self.set_positioning(active, cx),
            Command::Media => {
                self.media_dirty = true;
                cx.notify();
            }
            Command::Popup(_) | Command::Shutdown => {}
        }
    }

    fn show(&mut self, content: VolumeOsdContent, style: VolumeOsdStyle, cx: &mut Context<Self>) {
        let now = Instant::now();
        let appearing = !self.want_visible;
        if appearing {
            self.system = System::load();
            // Fully faded out: nothing on screen to grow from.
            if self.visibility.value(now) < 0.01 {
                self.snap_layout = true;
            }
        }
        self.media_dirty = true;
        let next_name = device_caption(&content);
        if self.device_name != next_name || self.style.theme != style.theme {
            self.caption_dirty = true;
        }
        self.device_name = next_name;
        self.content = content;
        self.style = style;
        self.needs_measure = true;
        self.sync_chrome(now);
        self.sync_glyph(now);
        self.sync_level(now);
        self.place();
        if appearing {
            // After `place`, so the check looks at the monitor the OSD is on.
            self.over_fullscreen = crate::volume_osd::over_fullscreen();
        }
        if !self.style.enabled {
            self.hide_wait = None;
            self.begin_exit(now);
            cx.notify();
            return;
        }
        self.want_visible = true;
        self.begin_enter(now);
        self.arm_hide(cx, now);
        self.arm_surface_fallback(cx);
        cx.notify();
    }

    fn apply_style(&mut self, style: VolumeOsdStyle, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.system = System::load();
        self.caption_dirty = true;
        self.media_dirty = true;
        self.needs_measure = true;
        self.style = style;
        self.sync_chrome(now);
        self.sync_glyph(now);
        self.place();
        if !self.style.enabled {
            self.hide_wait = None;
            self.begin_exit(now);
        } else if self.want_visible {
            self.begin_enter(now);
            self.arm_hide(cx, now);
        }
        cx.notify();
    }

    fn set_positioning(&mut self, active: bool, cx: &mut Context<Self>) {
        self.positioning = active;
        crate::volume_osd::set_positioning_native(active);
        let now = Instant::now();
        if active {
            self.hide_wait = None;
            self.begin_enter(now);
        } else {
            self.arm_hide(cx, now);
        }
        cx.notify();
    }

    fn hide(&mut self, cx: &mut Context<Self>) {
        self.hide_wait = None;
        self.begin_exit(Instant::now());
        cx.notify();
    }

    fn on_surface_ready(&mut self, cx: &mut Context<Self>) {
        if self.surface_ready {
            return;
        }
        self.surface_ready = true;
        if self.want_visible {
            self.begin_enter(Instant::now());
        }
        cx.notify();
    }

    fn begin_enter(&mut self, now: Instant) {
        self.want_visible = true;
        // A hidden window gets no frames, so `render` would never get to call
        // `present(true)` again. Show the window here; the frame that follows
        // animates it in.
        crate::volume_osd::present(true);
        if !self.surface_ready {
            return;
        }
        self.visibility.retarget(1.0, now);
        if self.entrance() != Entrance::Fade {
            self.slide.retarget(0.0, now);
        } else {
            self.slide = Motion::new(0.0, SLIDE_MS);
        }
    }

    fn begin_exit(&mut self, now: Instant) {
        self.want_visible = false;
        if !self.surface_ready {
            return;
        }
        self.visibility.retarget(0.0, now);
        if self.entrance() != Entrance::Fade {
            self.slide.retarget(1.0, now);
        }
    }

    fn entrance(&self) -> Entrance {
        self.incoming().chrome.entrance
    }

    fn incoming(&self) -> &ChromeLayer {
        self.layers
            .iter()
            .find(|layer| layer.opacity.to >= 0.5)
            .or_else(|| self.layers.last())
            .expect("volume chrome")
    }

    fn place(&self) {
        let display = crate::volume_osd::resolve_display(&self.style.display);
        crate::volume_osd::set_placement(display, self.style.position_x, self.style.position_y);
    }

    /// Each show restarts the hide delay. `while_muted` cancels it and stays up.
    /// The task is replaced, which cancels the previous wait. The callback must
    /// not drop `hide_wait`: that task is the one currently running.
    fn arm_hide(&mut self, cx: &mut Context<Self>, now: Instant) {
        if self.positioning || (self.style.while_muted && self.content.is_muted()) {
            self.hide_wait = None;
            return;
        }
        if self.style.duration_ms == 0 {
            self.hide_wait = None;
            self.begin_exit(now);
            return;
        }
        // The "Up next" intro plays first; the track still gets its full time.
        let intro = match self.content {
            VolumeOsdContent::Media {
                action: MediaAction::UpNext,
                ..
            } => (UP_NEXT_HOLD_MS + UP_NEXT_SWIPE_MS) as u64,
            _ => 0,
        };
        let wait = self.style.duration_ms as u64 + intro;
        self.hide_wait = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(wait))
                .await;
            let _ = view.update(cx, |view, cx| {
                view.begin_exit(Instant::now());
                cx.notify();
            });
        }));
    }

    fn arm_surface_fallback(&mut self, cx: &mut Context<Self>) {
        if self.surface_ready || self.fallback_armed {
            return;
        }
        self.fallback_armed = true;
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            let _ = view.update(cx, |view, cx| {
                if !view.surface_ready && view.want_visible {
                    view.on_surface_ready(cx);
                }
            });
        })
        .detach();
    }

    fn sync_chrome(&mut self, now: Instant) {
        let theme = normalize_theme(&self.style.theme).to_string();
        let muted_bit = self.content.is_muted() && mute_sensitive(&theme);
        let key = (theme, muted_bit);
        let system = self.system;
        let mut matched = false;
        for layer in &mut self.layers {
            let current = layer.key == key;
            layer.opacity.retarget(if current { 1.0 } else { 0.0 }, now);
            if current {
                layer.chrome = chrome_for(&key.0, system, muted_bit, &self.style);
                matched = true;
            }
        }
        if !matched {
            let chrome = chrome_for(&key.0, system, muted_bit, &self.style);
            let opacity = if self.layers.is_empty() {
                Motion::new(1.0, CROSSFADE_MS)
            } else {
                let mut opacity = Motion::new(0.0, CROSSFADE_MS);
                opacity.retarget(1.0, now);
                opacity
            };
            self.layers.push(ChromeLayer { key, chrome, opacity });
        }
        self.layers
            .retain(|layer| layer.opacity.to > 0.0 || layer.opacity.active(now));
    }

    fn sync_glyph(&mut self, now: Instant) {
        // The music badge is too small for Fluent's hairline glyphs.
        let media = matches!(self.content, VolumeOsdContent::Media { .. });
        let id = if self.incoming().chrome.fluent && !media {
            fluent_glyph(&self.content)
        } else {
            content_glyph(&self.content)
        };
        let color = self.incoming().chrome.icon_color;
        let mut matched = false;
        for glyph in &mut self.glyphs {
            let current = glyph.path == id && glyph.color == color;
            glyph.opacity.retarget(if current { 1.0 } else { 0.0 }, now);
            if current {
                matched = true;
            }
        }
        if !matched {
            let opacity = if self.glyphs.is_empty() {
                Motion::new(1.0, 140)
            } else {
                let mut opacity = Motion::new(0.0, 140);
                opacity.retarget(1.0, now);
                opacity
            };
            self.glyphs.push(Glyph {
                path: id.into(),
                color,
                opacity,
            });
        }
        self.glyphs
            .retain(|glyph| glyph.opacity.to > 0.0 || glyph.opacity.active(now));
    }

    fn sync_level(&mut self, now: Instant) {
        let target = f32::from(self.content.percent()) / 100.0;
        if !self.level_ready {
            self.level = Motion::new(target, LEVEL_MS);
            self.level_ready = true;
        } else {
            self.level.retarget(target, now);
        }
    }

    fn sync_captions(&mut self, text: Option<String>, now: Instant) {
        let Some(text) = text else {
            for caption in &mut self.captions {
                caption.opacity.retarget(0.0, now);
            }
            self.captions
                .retain(|caption| caption.opacity.to > 0.0 || caption.opacity.active(now));
            return;
        };
        let text = SharedString::from(text);
        let mut matched = false;
        for caption in &mut self.captions {
            let current = caption.text == text;
            caption.opacity.retarget(if current { 1.0 } else { 0.0 }, now);
            if current {
                matched = true;
            }
        }
        if !matched {
            let opacity = if self.captions.is_empty() {
                Motion::new(1.0, FADE_MS)
            } else {
                let mut opacity = Motion::new(0.0, FADE_MS);
                opacity.retarget(1.0, now);
                opacity
            };
            self.captions.push(Caption { text, opacity });
        }
        self.captions
            .retain(|caption| caption.opacity.to > 0.0 || caption.opacity.active(now));
    }

    /// Crossfade the strip to `next`, or fade it out. A re-fit of the same
    /// track (new theme font, art arriving late) updates it in place.
    fn sync_media(&mut self, next: Option<MediaLayer>, now: Instant, snap: bool) {
        let key = next.as_ref().map(|layer| layer.key.clone());
        // A different track takes over from one already on screen.
        let replacing = !snap
            && key.is_some()
            && !self.media.is_empty()
            && !self.media.iter().any(|layer| key.as_ref() == Some(&layer.key));
        let mut matched = false;
        for layer in &mut self.media {
            let current = key.as_ref() == Some(&layer.key);
            if snap {
                layer.opacity = Motion::new(if current { 1.0 } else { 0.0 }, FADE_MS);
            } else if replacing {
                layer.swipe = true;
                layer.opacity.retarget_with_duration(0.0, now, TRACK_MS);
            } else {
                layer.opacity.retarget(if current { 1.0 } else { 0.0 }, now);
            }
            if current && let Some(next) = next.as_ref() {
                layer.title = next.title.clone();
                layer.artist = next.artist.clone();
                layer.text_width = next.text_width;
                layer.cover = next.cover.clone();
                matched = true;
            }
        }
        if let Some(mut next) = next.filter(|_| !matched) {
            next.opacity = if snap {
                Motion::new(1.0, FADE_MS)
            } else if self.media.is_empty() {
                // A strip that grows in fades in with the growth.
                let mut opacity = Motion::new(0.0, SIZE_MS);
                opacity.retarget(1.0, now);
                opacity
            } else {
                // A track change swipes in over the old one.
                next.swipe = true;
                let mut opacity = Motion::new(0.0, TRACK_MS);
                opacity.retarget(1.0, now);
                opacity
            };
            if next.eyebrow {
                next.intro = Some(now);
            }
            self.media.push(next);
        }
        self.media
            .retain(|layer| layer.opacity.to > 0.0 || layer.opacity.active(now));
    }

    fn ensure_fonts(&mut self, cx: &mut Context<Self>) {
        let families: Vec<SharedString> = self
            .layers
            .iter()
            .map(|layer| layer.chrome.font.clone())
            .collect();
        for family in families {
            if self.loaded_fonts.iter().any(|loaded| loaded == &family) {
                continue;
            }
            if let Some(data) = fonts::embedded(&family) {
                if let Err(error) = cx.text_system().add_fonts(vec![data]) {
                    eprintln!("volume OSD font {family}: {error:#}");
                }
            }
            self.loaded_fonts.push(family);
        }
    }
}

impl Render for VolumeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        if !self.background_set {
            window.set_background_appearance(WindowBackgroundAppearance::Transparent);
            self.background_set = true;
        }
        let window_scale = window.scale_factor().max(0.01);
        let monitor_scale = crate::volume_osd::monitor_scale().max(0.01);
        let scale = monitor_scale / window_scale * self.style.scale.clamp(0.1, 4.0);
        if (self.last_scale - scale).abs() > 0.01 {
            self.last_scale = scale;
            self.held = (0.0, 0.0);
            self.needs_measure = true;
            self.caption_dirty = true;
        }
        self.layers
            .retain(|layer| layer.opacity.to > 0.0 || layer.opacity.active(now));
        self.glyphs
            .retain(|glyph| glyph.opacity.to > 0.0 || glyph.opacity.active(now));
        self.captions
            .retain(|caption| caption.opacity.to > 0.0 || caption.opacity.active(now));
        self.ensure_fonts(cx);

        self.media
            .retain(|layer| layer.opacity.to > 0.0 || layer.opacity.active(now));
        // Fonts and scale fit the track's text as much as a device name's.
        self.media_dirty |= self.caption_dirty;
        let snap = std::mem::take(&mut self.snap_layout);
        if self.media_dirty {
            self.media_dirty = false;
            let chrome = self.incoming().chrome.clone();
            let source = match &self.content {
                VolumeOsdContent::Media { track, action } => {
                    Some((track.clone(), *action == MediaAction::UpNext))
                }
                VolumeOsdContent::Level { .. } if self.style.show_media => {
                    crate::media_session::playing_now().map(|track| (track, false))
                }
                _ => None,
            };
            let measure_window = &*window;
            let next = source.map(|(playing, eyebrow)| {
                let max = MEDIA_TEXT_MAX * scale;
                let (title_weight, title_size) = title_type(&chrome, scale);
                let (artist_weight, artist_size) = artist_type(&chrome, scale);
                let measure_title = |text: &str| {
                    measure_styled(text, &chrome.font, title_weight, title_size, measure_window)
                };
                let measure_artist = |text: &str| {
                    measure_styled(text, &chrome.font, artist_weight, artist_size, measure_window)
                };
                let title = fit_text(&playing.title, max, measure_title);
                let artist = fit_text(&playing.artist, max, measure_artist);
                let (intro_weight, intro_size) = intro_type(&chrome, scale);
                let eyebrow_width = if eyebrow {
                    measure_styled(UP_NEXT, &chrome.font, intro_weight, intro_size, measure_window)
                } else {
                    0.0
                };
                let text_width = measure_title(&title)
                    .max(measure_artist(&artist))
                    .max(eyebrow_width);
                MediaLayer {
                    key: format!("{}\u{1}{}\u{1}{eyebrow}", playing.title, playing.artist),
                    title: title.into(),
                    artist: artist.into(),
                    cover: playing.cover.clone(),
                    eyebrow,
                    swipe: false,
                    intro: None,
                    text_width,
                    opacity: Motion::new(0.0, FADE_MS),
                }
            });
            self.sync_media(next, now, snap);
            self.needs_measure = true;
        }

        if self.caption_dirty {
            self.caption_dirty = false;
            let chrome = self.incoming().chrome.clone();
            let measure_window = &*window;
            let fitted = self.device_name.clone().map(|name| {
                fit_name(&name, NAME_MAX_WIDTH * scale, |text| {
                    measure_text(text, &chrome, scale, measure_window)
                })
            });
            self.sync_captions(fitted, now);
            self.needs_measure = true;
        }

        let chrome = self.incoming().chrome.clone();
        let percent_slot = measure_text("100", &chrome, scale, window).max(20.0 * scale);
        let name_width = self
            .captions
            .iter()
            .find(|caption| caption.opacity.to > 0.5)
            .map(|caption| measure_text(&caption.text, &chrome, scale, window))
            .unwrap_or(0.0);
        let pad = chrome.pad_x * scale;
        let lead = chrome.lead() * scale;
        let text_pad = chrome.text_pad() * scale;
        let gap = chrome.gap * scale;
        let icon_slot = chrome
            .icon_box
            .map(|icon_box| icon_box.size)
            .unwrap_or(chrome.icon_size)
            * scale;
        let bar_w = chrome.bar_width * scale;
        let bar_h = chrome.bar_height * scale;
        let bar_r = chrome
            .bar_radius
            .map(|corner| corner * scale)
            .unwrap_or(bar_h / 2.0)
            .min(bar_h / 2.0);
        // A device switch shows only the glyph and the device name.
        let device_mode = matches!(self.content, VolumeOsdContent::Device { .. });
        // The music widget: cover, lines, and the action's glyph at the end.
        let media_mode = matches!(self.content, VolumeOsdContent::Media { .. });
        let (number_lead, number_trail) = if chrome.number_slot.is_some() {
            (0.0, 0.0)
        } else {
            // The slot already centers the number, so it needs a little less
            // than a device name does.
            (gap * NUMBER_GAP, (text_pad - 4.0 * scale).max(pad))
        };
        let number_w = chrome
            .number_slot
            .map(|slot| slot * scale)
            .unwrap_or(number_lead + percent_slot + number_trail);
        let base = if device_mode {
            lead + icon_slot + text_pad
        } else {
            lead + icon_slot + gap + bar_w + number_w
        };
        let extra = if name_width > 0.0 {
            gap + name_width
        } else {
            0.0
        };
        // The strip's cover lines up with the icon below it and sits as far
        // from the top edge as from the left one.
        let media_inset = (chrome.lead() * scale).max(8.0 * scale);
        let cover = COVER * scale;
        let media_text_x = media_inset + cover + gap;
        let widget_cover = WIDGET_COVER * scale;
        // Tighter than the strip: the widget is all cover and text.
        let widget_inset = media_inset.min(10.0 * scale);
        let widget_base = widget_inset + widget_cover + gap + text_pad;
        let incoming_media = self.media.iter().find(|layer| layer.opacity.to > 0.5);
        let (target_w, target_h, media_h) = if media_mode {
            (
                widget_base + incoming_media.map_or(0.0, |layer| layer.text_width),
                (chrome.height * scale).max(widget_cover + widget_inset * 2.0),
                0.0,
            )
        } else {
            let (media_w, media_h) = match incoming_media {
                Some(layer) => (media_text_x + layer.text_width + text_pad, media_inset + cover),
                None => (0.0, 0.0),
            };
            (
                (base + extra).max(icon_slot + lead * 2.0).max(media_w),
                (chrome.height * scale + media_h).max(1.0),
                media_h,
            )
        };
        let target_r = chrome.radius * scale;
        if self.needs_measure && target_w >= 8.0 {
            if self.measured && !snap {
                self.width.retarget(target_w, now);
                self.height.retarget(target_h, now);
                self.radius.retarget(target_r, now);
                self.media_height.retarget(media_h, now);
            } else {
                self.width = Motion::new(target_w, SIZE_MS);
                self.height = Motion::new(target_h, SIZE_MS);
                self.radius = Motion::new(target_r, SIZE_MS);
                self.media_height = Motion::new(media_h, SIZE_MS);
                self.measured = true;
            }
            self.needs_measure = false;
        }

        let width = self.width.value(now).max(1.0);
        let height = self.height.value(now).max(1.0);
        let radius = self
            .radius
            .value(now)
            .clamp(0.0, width.min(height) / 2.0);
        let slide_theme = chrome.entrance != Entrance::Fade;
        let slide = if !self.surface_ready {
            1.0
        } else if slide_theme {
            self.slide.value(now)
        } else {
            0.0
        };
        let alpha = if self.surface_ready {
            self.visibility.value(now)
        } else {
            1.0
        };
        let lingering = if !self.surface_ready {
            false
        } else if slide_theme {
            alpha > 0.02 || slide < 0.98
        } else {
            alpha > 0.02
        };
        let should_present = self.want_visible || lingering;
        let moving = self.width.active(now)
            || self.height.active(now)
            || self.radius.active(now)
            || self.media_height.active(now)
            || self
                .media
                .iter()
                .any(|layer| layer.opacity.active(now) || layer.intro_active(now))
            || self.visibility.active(now)
            || (slide_theme && self.slide.active(now))
            || self.level.active(now)
            || self.layers.iter().any(|layer| layer.opacity.active(now))
            || self.glyphs.iter().any(|glyph| glyph.opacity.active(now))
            || self.captions.iter().any(|caption| caption.opacity.active(now));
        let ambient = should_present
            && self.layers.iter().any(|layer| {
                layer.chrome.animates && layer.opacity.value(now) > 0.01
            });
        if should_present && moving {
            window.request_animation_frame();
        } else if ambient && self.ambient_frame.is_none() {
            self.ambient_frame = Some(cx.spawn(async move |view, cx| {
                cx.background_executor().timer(AMBIENT_FRAME).await;
                let _ = view.update(cx, |view, cx| {
                    view.ambient_frame = None;
                    cx.notify();
                });
            }));
        }

        // Whole physical pixels, so the card's edges and hairline stay crisp.
        let gutter = (chrome.gutter * scale * window_scale).round() / window_scale;
        // The glass theme keeps its window still, sized for the largest card
        // seen so far, and moves the card inside it.
        let held = (chrome.entrance == Entrance::Nudge && self.surface_ready).then(|| {
            self.held.0 = self.held.0.max(width).max(self.width.to);
            self.held.1 = self.held.1.max(height).max(self.height.to);
            (
                (self.held.0 * window_scale).ceil() as i32,
                (self.held.1 * window_scale).ceil() as i32,
            )
        });
        let origin = crate::volume_osd::set_geometry(
            (width * window_scale).round() as i32,
            (height * window_scale).round() as i32,
            ((width + gutter * 2.0) * window_scale).round() as i32,
            ((height + gutter * 2.0) * window_scale).round() as i32,
            (gutter * window_scale).round() as i32,
            (gutter * window_scale).round() as i32,
            slide,
            // Until the first paint lands the window waits fully off screen.
            (chrome.entrance == Entrance::Nudge && self.surface_ready)
                .then(|| (NUDGE_DISTANCE * scale * window_scale).round() as i32),
            held,
        );
        let (card_x, card_y) = (origin.0 as f32 / window_scale, origin.1 as f32 / window_scale);
        crate::volume_osd::present(should_present);

        let weights: Vec<(VolumeChrome, f32)> = {
            let mut weights: Vec<(VolumeChrome, f32)> = self
                .layers
                .iter()
                .map(|layer| (layer.chrome.clone(), layer.opacity.value(now).max(0.0)))
                .collect();
            let sum: f32 = weights.iter().map(|(_, weight)| *weight).sum();
            if sum < 0.001 {
                weights = vec![(chrome.clone(), 1.0)];
            }
            weights
        };
        let mut surface = mix_rgba(weights.iter().map(|(chrome, weight)| (chrome.surface, *weight)));
        let glass_share = {
            let total: f32 = weights.iter().map(|(_, weight)| *weight).sum();
            let glass: f32 = weights
                .iter()
                .filter(|(chrome, _)| chrome.mica)
                .map(|(_, weight)| *weight)
                .sum();
            if total > 0.001 { glass / total } else { 0.0 }
        };
        let track = mix_rgba(weights.iter().map(|(chrome, weight)| (chrome.track, *weight)));
        let fill = mix_rgba(weights.iter().map(|(chrome, weight)| (chrome.fill, *weight)));
        let foreground = mix_rgb(weights.iter().map(|(chrome, weight)| (chrome.foreground, *weight)));
        let detail_phase = now.duration_since(self.detail_epoch).as_secs_f32() / 2.4;
        let neon_pulse = 0.825 + 0.175 * (std::f32::consts::TAU * now.duration_since(self.detail_epoch).as_secs_f32() / 4.0).cos();
        let shadow = match chrome.shadow {
            Shadow::Soft => None,
            Shadow::Drop(color, y, blur) => Some(paint_shadow((0.0, y), blur, color, scale)),
            Shadow::Hard(color, x, y) => Some(paint_shadow((x, y), 0.0, color, scale)),
            Shadow::Halo(mut color, blur) => {
                if chrome.halo_pulse {
                    color.a *= neon_pulse;
                }
                Some(paint_shadow((0.0, 0.0), blur, color, scale))
            }
        };
        let glyphs: Vec<(SharedString, u32, f32)> = self
            .glyphs
            .iter()
            .map(|glyph| (glyph.path.clone(), glyph.color, glyph.opacity.value(now)))
            .filter(|(_, _, opacity)| *opacity > 0.01)
            .collect();
        let captions: Vec<(SharedString, f32)> = self
            .captions
            .iter()
            .map(|caption| (caption.text.clone(), caption.opacity.value(now)))
            .filter(|(_, opacity)| *opacity > 0.01)
            .collect();
        let detail_layers: Vec<(VolumeChrome, f32)> = weights
            .iter()
            .filter(|(_, opacity)| *opacity > 0.01)
            .cloned()
            .collect();
        // Room the meter row does not need. A device name takes it; with the
        // meter shown (under a wider now-playing strip), the bar does.
        let spare = (width - base).max(0.0);
        let name_slot = if device_mode { spare } else { 0.0 };
        let bar_w = if device_mode { bar_w } else { bar_w + spare };
        let strip_h = self.media_height.value(now).clamp(0.0, height);
        let row_h = (height - strip_h).max(1.0);
        let media: Vec<(MediaLayer, f32)> = self
            .media
            .iter()
            .map(|layer| (layer.clone(), layer.opacity.value(now)))
            .filter(|(_, opacity)| *opacity > 0.01)
            .collect();
        let level = self.level.value(now).clamp(0.0, 1.0);
        let percent = self.content.percent();
        let label = percent.to_string();

        // Real acrylic: the glass follows the card's bounds, fade and theme
        // crossfade; GPUI only lays a thin tint over it.
        let glass = match &mut self.backdrop {
            Some(backdrop) if backdrop.usable() && self.over_fullscreen => {
                backdrop.set_opacity(0.0);
                false
            }
            Some(backdrop) if backdrop.usable() => {
                backdrop.set_luminosity(glass_luminosity(self.system.light));
                backdrop.update(
                    [card_x, card_y, width, height].map(|v| (v * window_scale).round()),
                    radius * window_scale,
                    alpha * chrome.opacity * glass_share,
                );
                backdrop.usable()
            }
            _ => false,
        };
        if glass {
            surface.a -= glass_share * (1.0 - glass_tint(self.system.light));
        }
        // Fallback: sample the wallpaper where the card settles, not mid-slide.
        // Over a fullscreen app the wallpaper is not what is behind the card.
        let mica = if chrome.mica && !glass && !self.over_fullscreen {
            let final_w = (self.width.to * window_scale).round() as i32;
            let final_h = (self.height.to * window_scale).round() as i32;
            crate::volume_osd::card_rect(final_w, final_h)
                .and_then(|(monitor, rect)| super::mica::crop(monitor, rect))
        } else {
            None
        };
        if chrome.mica && !glass && mica.is_none() && super::mica::pending() {
            // Still blurring in the background; repaint once it is ready.
            window.request_animation_frame();
        }
        let mut card = div()
            .absolute()
            .left(px(card_x))
            .top(px(card_y))
            .w(px(width))
            .h(px(height))
            .rounded(px(radius))
            .opacity(alpha * chrome.opacity);
        card = match &mica {
            Some(wallpaper) => {
                let mut tint = surface;
                tint.a = if self.system.light { 0.62 } else { 0.72 };
                card.child(
                    img(wallpaper.clone())
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(width))
                        .h(px(height))
                        .rounded(px(radius))
                        .object_fit(ObjectFit::Fill),
                )
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(width))
                        .h(px(height))
                        .rounded(px(radius))
                        .bg(tint),
                )
            }
            None => card.bg(surface),
        };
        // GPUI shadows are filled, not hollow: under a translucent card one
        // would darken the glass. The acrylic card's is painted only outside
        // the card's rectangle, in four strips around it. (A shadow layer
        // under the glass in the backdrop left a ghost rectangle on screen.)
        let on_glass = glass && chrome.mica;
        let mut outer_shadow = Vec::new();
        card = match shadow {
            Some(shadow) if on_glass => {
                let viewport = window.viewport_size();
                let (view_w, view_h) = (viewport.width.as_f32(), viewport.height.as_f32());
                let (right, bottom) = (card_x + width, card_y + height);
                // (left, top, width, height) of each strip, in window pixels.
                let strips = [
                    (0.0, 0.0, view_w, card_y),
                    (0.0, bottom, view_w, view_h - bottom),
                    (0.0, card_y, card_x, height),
                    (right, card_y, view_w - right, height),
                ];
                for (left, top, strip_w, strip_h) in strips {
                    if strip_w <= 0.0 || strip_h <= 0.0 {
                        continue;
                    }
                    outer_shadow.push(
                        div()
                            .absolute()
                            .left(px(left))
                            .top(px(top))
                            .w(px(strip_w))
                            .h(px(strip_h))
                            .overflow_hidden()
                            .opacity(alpha * chrome.opacity)
                            .child(
                                div()
                                    .absolute()
                                    .left(px(card_x - left))
                                    .top(px(card_y - top))
                                    .w(px(width))
                                    .h(px(height))
                                    .rounded(px(radius))
                                    .shadow(vec![shadow.clone()]),
                            ),
                    );
                }
                card
            }
            _ if on_glass => card,
            Some(shadow) => card.shadow(vec![shadow]),
            None => card.shadow_md(),
        };
        // Cute Sticker's die-cut contour: the pill plus a round badge bulging
        // around the icon and two sparkles hanging over the edge.
        let sticker_weight = detail_layers
            .iter()
            .filter(|(layer, _)| layer.sticker)
            .map(|(_, weight)| *weight)
            .sum::<f32>()
            .min(1.0);
        // The music widget has no glyph to ring: its sparkles hang off the cover.
        let (badge, badge_x, badge_y) = if media_mode {
            (widget_cover, widget_inset, (height - widget_cover) / 2.0)
        } else {
            let badge = icon_slot + STICKER_RIM * 2.0 * scale;
            (
                badge,
                lead + (icon_slot - badge) / 2.0,
                strip_h + (row_h - badge) / 2.0,
            )
        };
        let paper_badge = sticker_weight * if media_mode { 0.0 } else { 1.0 };
        // (left, top, size, tilt): one over the top right edge, a smaller one
        // on the badge's lower left.
        let stars = [
            if media_mode {
                // Over the cover's corner; the end belongs to the badge.
                (media_inset + widget_cover * 0.62, -11.0 * scale, 22.0 * scale, 0.21)
            } else {
                (width - 34.0 * scale, -11.0 * scale, 22.0 * scale, 0.21)
            },
            (
                badge_x + badge * if media_mode { 0.0 } else { 0.14 } - 7.0 * scale,
                badge_y + badge * 0.86 - 7.0 * scale,
                14.0 * scale,
                -0.3,
            ),
        ];
        let star = |id: &'static str, (left, top, size, tilt): (f32, f32, f32, f32), drop: f32| {
            svg()
                .path(id)
                .absolute()
                .left(px(left))
                .top(px(top + drop))
                .size(px(size))
                .with_transformation(Transformation::rotate(radians(tilt)))
        };
        if sticker_weight > 0.01 {
            // Every shadow first, then the paper: no piece shades another.
            if let Shadow::Drop(color, y, blur) = chrome.shadow {
                card = card.child(
                    div()
                        .absolute()
                        .left(px(badge_x))
                        .top(px(badge_y))
                        .size(px(badge))
                        .rounded(px(badge / 2.0))
                        .shadow(vec![paint_shadow((0.0, y), blur, color, scale)])
                        .opacity(paper_badge),
                );
                let mut soft = color;
                soft.a *= 0.6;
                for sparkle in stars {
                    card = card.child(
                        star("sticker-star-paper", sparkle, y * scale)
                            .text_color(soft)
                            .opacity(sticker_weight),
                    );
                }
            }
            card = card
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(width))
                        .h(px(height))
                        .rounded(px(radius))
                        .bg(rgb(0xffffff))
                        .opacity(sticker_weight),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(badge_x))
                        .top(px(badge_y))
                        .size(px(badge))
                        .rounded(px(badge / 2.0))
                        .bg(rgb(0xffffff))
                        .opacity(paper_badge),
                );
            for sparkle in stars {
                card = card.child(
                    star("sticker-star-paper", sparkle, 0.0)
                        .text_color(rgb(0xffffff))
                        .opacity(sticker_weight),
                );
            }
        }
        let mut contents = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .rounded(px(radius));
        if chrome.sheen > 0.0 {
            contents = contents.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .size_full()
                    .rounded(px(radius))
                    .bg(linear_gradient(
                        180.0,
                        linear_color_stop(color_alpha(0xffffff, chrome.sheen * surface.a), 0.0),
                        linear_color_stop(color_alpha(0xffffff, 0.0), 0.55),
                    )),
            );
        }
        for (layer, weight) in &detail_layers {
            if !matches!(layer.detail, Detail::None) {
                contents = contents.child(details::render(
                    &detail_look(layer),
                    width,
                    height,
                    scale,
                    *weight,
                    detail_phase,
                    radius,
                ));
            }
        }
        let mut icon = div()
            .relative()
            .flex_shrink_0()
            .w(px(icon_slot))
            .h(px(icon_slot));
        for (layer, weight) in &detail_layers {
            if let Some(icon_box) = layer.icon_box {
                let box_size = icon_box.size * scale;
                let corner = icon_box
                    .radius
                    .map(|corner| corner * scale)
                    .unwrap_or_else(|| (radius * box_size / height.max(1.0)).min(box_size / 2.0));
                let mut holder = div()
                    .absolute()
                    .left(px((icon_slot - box_size) / 2.0))
                    .top(px((icon_slot - box_size) / 2.0))
                    .w(px(box_size))
                    .h(px(box_size))
                    .rounded(px(corner.min(box_size / 2.0)))
                    .bg(icon_box.fill)
                    .opacity(*weight);
                if let Some((color, line)) = icon_box.border {
                    holder = border_width(holder, (line * scale).max(1.0)).border_color(color);
                }
                icon = icon.child(holder);
            }
        }
        for (path, color, opacity) in &glyphs {
            icon = icon.child(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(*opacity)
                    .child(match path.strip_prefix("fluent:") {
                        Some(code) => {
                            let glyph = u32::from_str_radix(code, 16)
                                .ok()
                                .and_then(char::from_u32)
                                .unwrap_or(' ');
                            div()
                                .font_family(fluent_font())
                                .text_size(px(chrome.icon_size * scale))
                                .line_height(px(chrome.icon_size * scale))
                                .text_color(rgb(*color))
                                .child(glyph.to_string())
                                .into_any_element()
                        }
                        None => {
                            let mut glyph = svg()
                                .path(path.clone())
                                .size(px(chrome.icon_size * scale))
                                .text_color(rgb(*color));
                            if chrome.icon_tilt != 0.0 {
                                glyph = glyph.with_transformation(Transformation::rotate(
                                    radians(chrome.icon_tilt),
                                ));
                            }
                            glyph.into_any_element()
                        }
                    }),
            );
        }
        if strip_h > 0.5 && !media.is_empty() {
            contents = contents.child(media_strip(
                &media,
                &chrome,
                MediaFrame {
                    foreground,
                    accent: fill,
                    track,
                    scale,
                    card_radius: radius,
                    inset: media_inset,
                    cover,
                    text_w: (width - media_text_x - text_pad).max(0.0),
                    gap,
                },
                width,
                strip_h,
            ));
        }
        if media_mode {
            let frame = MediaFrame {
                foreground,
                accent: fill,
                track,
                scale,
                card_radius: radius,
                inset: widget_inset.min((height - widget_cover) / 2.0).max(0.0),
                cover: widget_cover,
                text_w: (width - widget_base).max(0.0),
                gap,
            };
            let mut ring = surface;
            ring.a = 1.0;
            contents = contents.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(width))
                    .h(px(height))
                    .flex()
                    .items_center()
                    .pl(px(widget_inset))
                    .child(
                        div()
                            .relative()
                            .flex_shrink_0()
                            .size(px(widget_cover))
                            .child(media_art(&media, &chrome, &frame))
                            .child(media_badge(&glyphs, &chrome, fill, ring, widget_cover, scale)),
                    )
                    .child(div().flex_shrink_0().w(px(gap)))
                    .child(media_lines(&media, &chrome, &frame, height, now).h(px(height))),
            );
        } else {
            let mut row = div()
                .absolute()
                .left_0()
                .top(px(strip_h))
                .w(px(width))
                .h(px(row_h))
                .flex()
                .items_center()
                .pl(px(lead))
                // With the meter shown, the number's area carries the right edge.
                .when(device_mode, |row| row.pr(px(text_pad)))
                .child(icon);
            if name_slot > 0.5 {
                let mut name = div()
                    .relative()
                    .h_full()
                    .w(px(name_slot))
                    .overflow_hidden();
                for (text, opacity) in &captions {
                    name = name.child(
                        div()
                            .absolute()
                            .left(px(gap))
                            .top_0()
                            .h_full()
                            .flex()
                            .items_center()
                            .opacity(*opacity)
                            .whitespace_nowrap()
                            .text_size(px(chrome.text_size * scale))
                            .font_family(chrome.font.clone())
                            .font_weight(FontWeight(chrome.weight as f32))
                            .text_color(rgb(foreground))
                            .child(text.clone()),
                    );
                }
                row = row.child(name);
            }
            if !device_mode {
                row = row.child(
                    div().pl(px(gap)).flex_shrink_0().child(
                        div()
                            .w(px(bar_w))
                            .h(px(bar_h.max(1.0)))
                            .rounded(px(bar_r))
                            .bg(track)
                            // GPUI clips to the rectangle, not the radius: round the
                            // fill itself so its start and end match the rail's.
                            .when(level > 0.0, |rail| {
                                rail.child(
                                    div()
                                        .h(px(bar_h.max(1.0)))
                                        .w(px((bar_w * level).max(bar_r * 2.0).max(1.0)))
                                        .rounded(px(bar_r))
                                        .bg(fill),
                                )
                            }),
                    ),
                );
                row = row.child(
                    div()
                        .pl(px(number_lead))
                        .pr(px(number_trail))
                        .w(px(number_w))
                        .flex_shrink_0()
                        .flex()
                        .justify_center()
                        .items_center()
                        .line_height(px(chrome.text_size * scale))
                        .whitespace_nowrap()
                        .text_size(px(chrome.text_size * scale))
                        .font_family(chrome.font.clone())
                        .font_weight(FontWeight(chrome.weight as f32))
                        .text_color(rgb(foreground))
                        .child(label),
                );
            }
            contents = contents.child(row);
        }
        card = card.child(contents);
        if sticker_weight > 0.01 {
            for sparkle in stars {
                card = card.child(
                    star("sticker-star-ink", sparkle, 0.0)
                        .text_color(fill)
                        .opacity(sticker_weight),
                );
            }
        }
        if let Some(color) = chrome.outline {
            let line = 1.0 / window_scale;
            card = card.child(
                border_width(
                    div()
                        .absolute()
                        .left(px(-line))
                        .top(px(-line))
                        .w(px(width + line * 2.0))
                        .h(px(height + line * 2.0))
                        .rounded(px(radius + line)),
                    line,
                )
                .border_color(color),
            );
        }
        if let Some((color, border)) = chrome.border.filter(|_| !on_glass) {
            card = card.child(
                border_width(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(width))
                        .h(px(height))
                        .rounded(px(radius)),
                    (border * scale).max(1.0),
                )
                .border_color(color),
            );
        }
        div()
            .relative()
            .size_full()
            .children(outer_shadow)
            .child(card)
    }
}

fn paint_shadow(offset: (f32, f32), blur: f32, color: Rgba, scale: f32) -> BoxShadow {
    BoxShadow {
        color: color.into(),
        offset: point(px(offset.0 * scale), px(offset.1 * scale)),
        blur_radius: px(blur * scale),
        spread_radius: px(0.0),
        inset: false,
    }
}

/// Lightness the acrylic backdrop is blended to: WinUI's default acrylic
/// luminosity colors.
fn glass_luminosity(light: bool) -> u8 {
    if light { 243 } else { 44 }
}

/// Opacity of the `surface` tint over real acrylic.
fn glass_tint(light: bool) -> f32 {
    if light { 0.3 } else { 0.15 }
}

fn border_width<E: Styled>(mut element: E, width: f32) -> E {
    let widths = &mut element.style().border_widths;
    widths.top = Some(px(width).into());
    widths.right = Some(px(width).into());
    widths.bottom = Some(px(width).into());
    widths.left = Some(px(width).into());
    element
}

fn measure_text(text: &str, chrome: &VolumeChrome, scale: f32, window: &Window) -> f32 {
    measure_styled(text, &chrome.font, chrome.weight, chrome.text_size * scale, window)
}

fn measure_styled(
    text: &str,
    family: &SharedString,
    weight: u16,
    size_px: f32,
    window: &Window,
) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let text: SharedString = text.to_string().into();
    let mut text_font = font(family.clone());
    text_font.weight = FontWeight(weight as f32);
    let run = TextRun {
        len: text.len(),
        font: text_font,
        color: rgb(0xffffff).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .shape_line(text, px(size_px), &[run], None);
    f32::from(line.width)
}

fn fit_name(name: &str, max_width: f32, measure: impl Fn(&str) -> f32) -> String {
    let name: String = name.chars().take(64).collect();
    let name = if name.is_empty() { "Output" } else { &name };
    fit_text(name, max_width, measure)
}

/// Cut `text` with an ellipsis to fit `max_width`. Empty stays empty.
fn fit_text(text: &str, max_width: f32, measure: impl Fn(&str) -> f32) -> String {
    let name: String = text.chars().take(96).collect();
    if name.is_empty() || max_width <= 1.0 || measure(&name) <= max_width {
        return name;
    }
    let mut kept = String::new();
    for ch in name.chars() {
        let mut next = kept.clone();
        next.push(ch);
        next.push('…');
        if measure(&next) > max_width {
            break;
        }
        kept.push(ch);
    }
    if kept.is_empty() {
        "…".to_string()
    } else {
        format!("{kept}…")
    }
}

/// Track title: the theme's face, a step bolder than its numbers so it leads.
/// Archivo Black ships a single weight and would only be smeared.
fn title_type(chrome: &VolumeChrome, scale: f32) -> (u16, f32) {
    let weight = if chrome.font.as_ref() == "Archivo Black" {
        chrome.weight
    } else {
        chrome.weight.max(600)
    };
    (weight, chrome.text_size * scale)
}

/// Artist line: same face, lighter and a little smaller than the title.
fn artist_type(chrome: &VolumeChrome, scale: f32) -> (u16, f32) {
    let weight = if chrome.font.as_ref() == "Archivo Black" {
        chrome.weight
    } else {
        chrome.weight.min(500)
    };
    (weight, (chrome.text_size - 2.0).max(10.0) * scale)
}

/// Size and colors shared by the strip and the music widget, in window pixels.
struct MediaFrame {
    foreground: u32,
    /// "Up next" ink: the theme's meter fill.
    accent: Rgba,
    track: Rgba,
    scale: f32,
    card_radius: f32,
    /// Distance from the card's edge to the cover.
    inset: f32,
    cover: f32,
    /// Width for the title and artist.
    text_w: f32,
    gap: f32,
}

/// What just happened, as a small disc on the cover's lower right corner. It
/// wears the theme's icon container where it has one (Material You's tonal
/// circle, Brutalism's black square tile is drawn round here), else the meter
/// fill, and a card-colored ring cuts it out of the art.
fn media_badge(
    glyphs: &[(SharedString, u32, f32)],
    chrome: &VolumeChrome,
    fill: Rgba,
    ring: Rgba,
    cover: f32,
    scale: f32,
) -> Div {
    let (disc, ink) = match chrome.icon_box {
        Some(icon_box) if icon_box.fill.a > 0.9 => (icon_box.fill, rgb(chrome.icon_color)),
        _ => {
            let luminance = 0.2126 * fill.r + 0.7152 * fill.g + 0.0722 * fill.b;
            let ink = if luminance > 0.55 { 0x1b1b1b } else { 0xffffff };
            (fill, rgb(ink))
        }
    };
    let size = MEDIA_BADGE * scale;
    let outer = size + MEDIA_BADGE_RING * 2.0 * scale;
    // A quarter of the badge hangs past the art, into the card's padding.
    let offset = cover - outer * 0.75;
    let mut badge = div()
        .absolute()
        .left(px(offset))
        .top(px(offset))
        .size(px(outer))
        .rounded(px(outer / 2.0))
        .bg(ring)
        .flex()
        .items_center()
        .justify_center()
        .child(div().size(px(size)).rounded(px(size / 2.0)).bg(disc));
    for (path, _, opacity) in glyphs {
        badge = badge.child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .opacity(*opacity)
                // Actions swap like covers: shrink to half, grow from half.
                .child(
                    svg()
                        .path(path.clone())
                        .size(px(size * 0.56 * (0.5 + 0.5 * opacity.clamp(0.0, 1.0))))
                        .text_color(ink),
                ),
        );
    }
    badge
}

/// Name of the eyebrow over a track the playlist moved on to.
const UP_NEXT: &str = "Up next";

/// The "Up next" intro label: same face, bold, a step over the title.
fn intro_type(chrome: &VolumeChrome, scale: f32) -> (u16, f32) {
    let weight = if chrome.font.as_ref() == "Archivo Black" {
        chrome.weight
    } else {
        chrome.weight.max(700)
    };
    (weight, (chrome.text_size + 2.0) * scale)
}

/// Cover, title and artist above the meter row, in the theme's own colors.
fn media_strip(
    layers: &[(MediaLayer, f32)],
    chrome: &VolumeChrome,
    frame: MediaFrame,
    width: f32,
    height: f32,
) -> impl IntoElement {
    div()
        .absolute()
        .left_0()
        .top_0()
        .w(px(width))
        .h(px(height))
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .left(px(frame.inset))
                .top(px(frame.inset))
                .child(media_art(layers, chrome, &frame)),
        )
        .child(
            media_lines(layers, chrome, &frame, frame.cover, Instant::now())
                .absolute()
                .left(px(frame.inset + frame.cover + frame.gap))
                .top(px(frame.inset))
                .h(px(frame.cover)),
        )
}

/// The covers of every visible track, stacked for the crossfade.
fn media_art(layers: &[(MediaLayer, f32)], chrome: &VolumeChrome, frame: &MediaFrame) -> Div {
    let MediaFrame {
        foreground,
        track,
        scale,
        card_radius,
        inset,
        cover,
        ..
    } = *frame;
    // Concentric with the card's corner, so a pill card gets a soft cover and
    // a square one a square cover; never fully sharp on a rounded card.
    let cover_radius = (card_radius - inset)
        .max(card_radius.min(4.0 * scale))
        .min(cover / 2.0);
    // Heavy theme borders (Brutalism, Cute, Arcade, Neon) frame the art the
    // same way; the rest get a hairline so dark art keeps its edge.
    let (edge, edge_width) = match chrome.border {
        Some((color, line)) if line >= 1.5 => (color, line * scale),
        _ => (color_alpha(foreground, 0.12), (0.75 * scale).max(1.0)),
    };
    let mut stack = div().relative().flex_shrink_0().size(px(cover));
    for (layer, opacity) in layers {
        // A track change shrinks the old cover to half and grows the new one
        // from half, around the cover's center.
        let (zoom, _) = layer.swipe(*opacity);
        let edge_len = cover * zoom;
        let corner = cover_radius * zoom;
        let art = match &layer.cover {
            Some(image) => img(image.clone())
                .absolute()
                .left_0()
                .top_0()
                .size(px(edge_len))
                .rounded(px(corner))
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            // No art from the player: the theme's track tone with a note.
            None => div()
                .absolute()
                .left_0()
                .top_0()
                .size(px(edge_len))
                .rounded(px(corner))
                .bg(track)
                .flex()
                .items_center()
                .justify_center()
                .child(if chrome.fluent {
                    div()
                        .font_family(fluent_font())
                        .text_size(px(edge_len * 0.42))
                        .line_height(px(edge_len * 0.42))
                        .text_color(rgb(chrome.ink))
                        .child('\u{EC4F}'.to_string())
                        .into_any_element()
                } else {
                    svg()
                        .path("media-note")
                        .size(px(edge_len * 0.45))
                        .text_color(rgb(chrome.ink))
                        .into_any_element()
                })
                .into_any_element(),
        };
        let outline = border_width(
            div()
                .absolute()
                .left_0()
                .top_0()
                .size(px(edge_len))
                .rounded(px(corner)),
            edge_width,
        )
        .border_color(edge);
        stack = stack.child(
            div()
                .absolute()
                .left(px((cover - edge_len) / 2.0))
                .top(px((cover - edge_len) / 2.0))
                .size(px(edge_len))
                .opacity(*opacity)
                .child(art)
                .child(outline),
        );
    }
    stack
}

/// Title, artist and the "Up next" eyebrow of every visible track, stacked
/// for the crossfade and centered in the column's height.
fn media_lines(
    layers: &[(MediaLayer, f32)],
    chrome: &VolumeChrome,
    frame: &MediaFrame,
    height: f32,
    now: Instant,
) -> Div {
    let MediaFrame {
        foreground,
        accent,
        scale,
        text_w,
        ..
    } = *frame;
    let (title_weight, title_size) = title_type(chrome, scale);
    let (artist_weight, artist_size) = artist_type(chrome, scale);
    let (intro_weight, intro_size) = intro_type(chrome, scale);
    let mut stack = div()
        .relative()
        .flex_shrink_0()
        .w(px(text_w))
        .overflow_hidden();
    for (layer, opacity) in layers {
        // Track change: in from the right, out to the left.
        let (_, shift) = layer.swipe(*opacity);
        let dx = shift * TRACK_SHIFT * scale;
        // "Up next" first fills the column, then lifts away while the track
        // rises in under it.
        let lift = layer.intro_progress(now);
        if lift < 1.0 {
            stack = stack.child(up_next_panel(
                chrome,
                accent,
                (intro_weight, intro_size),
                text_w,
                height,
                layer.intro_elapsed(now),
                dx,
                -lift * height,
                *opacity * (1.0 - lift),
                scale,
            ));
        }
        let mut lines = div()
            .absolute()
            .left(px(dx))
            .top(px((1.0 - lift) * height * 0.6))
            .w(px(text_w))
            .h(px(height))
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(MEDIA_LINE_GAP * scale))
            .whitespace_nowrap()
            .font_family(chrome.font.clone())
            .opacity(*opacity * lift);
        lines = lines.child(
            div()
                .font_weight(FontWeight(title_weight as f32))
                .text_size(px(title_size))
                .line_height(px(title_size * 1.25))
                .text_color(rgb(foreground))
                .child(layer.title.clone()),
        );
        if !layer.artist.is_empty() {
            lines = lines.child(
                div()
                    .font_weight(FontWeight(artist_weight as f32))
                    .text_size(px(artist_size))
                    .line_height(px(artist_size * 1.25))
                    .text_color(color_alpha(foreground, 0.68))
                    .child(layer.artist.clone()),
            );
        }
        stack = stack.child(lines);
    }
    stack
}

/// The "Up next" intro: the label over a row of faint double chevrons that
/// keep drifting forward, the way the playlist just moved.
#[allow(clippy::too_many_arguments)]
fn up_next_panel(
    chrome: &VolumeChrome,
    accent: Rgba,
    (weight, size): (u16, f32),
    width: f32,
    height: f32,
    elapsed_ms: f32,
    dx: f32,
    dy: f32,
    opacity: f32,
    scale: f32,
) -> Div {
    let chevron = (height * 0.62).max(12.0 * scale);
    let spacing = chevron * 0.8;
    let drift = (elapsed_ms / 1000.0 * UP_NEXT_DRIFT * scale) % spacing;
    let mut tint = accent;
    tint.a *= 0.16;
    let mut chevrons = div()
        .absolute()
        .left_0()
        .top_0()
        .w(px(width))
        .h(px(height))
        .overflow_hidden();
    let count = (width / spacing).ceil() as usize + 2;
    for index in 0..count {
        chevrons = chevrons.child(
            svg()
                .path("media-up-next")
                .absolute()
                .left(px(drift + (index as f32 - 1.0) * spacing))
                .top(px((height - chevron) / 2.0))
                .size(px(chevron))
                .text_color(tint),
        );
    }
    div()
        .absolute()
        .left(px(dx))
        .top(px(dy))
        .w(px(width))
        .h(px(height))
        .opacity(opacity)
        .child(chevrons)
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .size_full()
                .flex()
                .items_center()
                .whitespace_nowrap()
                .font_family(chrome.font.clone())
                .font_weight(FontWeight(weight as f32))
                .text_size(px(size))
                .line_height(px(size * 1.2))
                .text_color(accent)
                .child(UP_NEXT),
        )
}

fn mix_rgba(parts: impl Iterator<Item = (Rgba, f32)>) -> Rgba {
    let mut mixed = Rgba::default();
    let mut weight_sum = 0.0;
    for (color, weight) in parts {
        if weight <= 0.0 {
            continue;
        }
        weight_sum += weight;
        mixed.r += color.r * weight;
        mixed.g += color.g * weight;
        mixed.b += color.b * weight;
        mixed.a += color.a * weight;
    }
    if weight_sum <= 0.001 {
        return mixed;
    }
    mixed.r /= weight_sum;
    mixed.g /= weight_sum;
    mixed.b /= weight_sum;
    mixed.a /= weight_sum;
    mixed
}

fn mix_rgb(parts: impl Iterator<Item = (u32, f32)>) -> u32 {
    let mut red = 0.0;
    let mut green = 0.0;
    let mut blue = 0.0;
    let mut weight_sum = 0.0;
    for (color, weight) in parts {
        if weight <= 0.0 {
            continue;
        }
        weight_sum += weight;
        red += ((color >> 16) & 0xff) as f32 * weight;
        green += ((color >> 8) & 0xff) as f32 * weight;
        blue += (color & 0xff) as f32 * weight;
    }
    if weight_sum <= 0.001 {
        return 0xffffff;
    }
    let channel = |value: f32| (value / weight_sum).round().clamp(0.0, 255.0) as u32;
    channel(red) << 16 | channel(green) << 8 | channel(blue)
}

fn detail_look(chrome: &VolumeChrome) -> Look {
    let slot = chrome
        .icon_box
        .map(|icon_box| icon_box.size)
        .unwrap_or(chrome.icon_size);
    Look {
        detail: chrome.detail,
        height: chrome.height,
        radius: chrome.radius,
        pad_icon: chrome.lead(),
        pad: chrome.pad_x,
        gap: chrome.gap,
        icon_size: slot,
        icon_path: "speaker-none".into(),
        icon: chrome.ink,
        icon_box: None,
        surface: chrome.surface,
        foreground: chrome.foreground,
        border: chrome.border,
        sheen: chrome.sheen,
        glow: 0.0,
        shadow: chrome.shadow,
        gutter: chrome.gutter,
        acrylic: false,
        font: chrome.font.clone(),
        weight: chrome.weight,
        text_size: chrome.text_size,
        label: "".into(),
        content_opacity: 1.0,
        has_icon: true,
        has_text: true,
        dot: false,
    }
}

fn device_caption(content: &VolumeOsdContent) -> Option<String> {
    let VolumeOsdContent::Device { name, .. } = content else {
        return None;
    };
    let name = name.replace(['\r', '\n', '\t'], " ");
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(if name.is_empty() {
        "Output".to_string()
    } else {
        name
    })
}

fn content_glyph(content: &VolumeOsdContent) -> &'static str {
    if content.is_muted() {
        return "speaker-mute";
    }
    match content {
        VolumeOsdContent::Level { percent, .. } => speaker_icon(*percent, false),
        VolumeOsdContent::Device { kind, .. } => match kind {
            DeviceKind::Headphones => "device-headphones",
            DeviceKind::Display => "device-monitor",
            DeviceKind::Microphone => "device-microphone",
            DeviceKind::Speakers | DeviceKind::Other => "device-speakers",
        },
        VolumeOsdContent::Media { action, .. } => match action {
            MediaAction::Play | MediaAction::UpNext => "media-play",
            MediaAction::Pause => "media-pause",
            MediaAction::Next => "media-next",
            MediaAction::Previous => "media-previous",
        },
    }
}

/// Segoe Fluent Icons code points, as `fluent:<hex>` glyph ids.
fn fluent_glyph(content: &VolumeOsdContent) -> &'static str {
    if content.is_muted() {
        return "fluent:E74F";
    }
    match content {
        VolumeOsdContent::Level { percent, .. } => match (*percent).min(100) {
            0 => "fluent:E992",
            1..=33 => "fluent:E993",
            34..=66 => "fluent:E994",
            _ => "fluent:E995",
        },
        VolumeOsdContent::Device { kind, .. } => match kind {
            DeviceKind::Headphones => "fluent:E7F6",
            DeviceKind::Display => "fluent:E7F4",
            DeviceKind::Microphone => "fluent:E720",
            DeviceKind::Speakers | DeviceKind::Other => "fluent:E7F5",
        },
        VolumeOsdContent::Media { action, .. } => match action {
            MediaAction::Play | MediaAction::UpNext => "fluent:E768",
            MediaAction::Pause => "fluent:E769",
            MediaAction::Next => "fluent:E893",
            MediaAction::Previous => "fluent:E892",
        },
    }
}

/// Segoe Fluent Icons on Windows 11, Segoe MDL2 Assets (same code points) on 10.
pub(super) fn fluent_font() -> &'static str {
    static FAMILY: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    FAMILY.get_or_init(|| {
        let fonts = std::env::var_os("WINDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("C:\\Windows"))
            .join("Fonts");
        if fonts.join("SegoeIcons.ttf").exists() {
            "Segoe Fluent Icons"
        } else {
            "Segoe MDL2 Assets"
        }
    })
}

/// Solar has no three-wave speaker. 34–100 share `volume-loud-bold`.
pub(crate) fn speaker_icon(percent: u8, muted: bool) -> &'static str {
    if muted {
        return "speaker-mute";
    }
    match percent.min(100) {
        0 => "speaker-none",
        1..=33 => "speaker-low",
        _ => "speaker-loud",
    }
}

/// Corner radius a theme draws with when the user has not set one.
pub(crate) fn preset_radius(theme: &str) -> f32 {
    chrome_for(theme, System::load(), false, &VolumeOsdStyle::default()).radius
}

pub(crate) fn normalize_theme(theme: &str) -> &'static str {
    let theme = theme.trim().to_ascii_lowercase();
    match theme.as_str() {
        "windows" => "Windows",
        "materialyou" | "material you" | "material-you" => "MaterialYou",
        "cute" => "Cute",
        "cutesticker" | "cute sticker" | "cute-sticker" => "CuteSticker",
        "neon" => "Neon",
        "brutalism" => "Brutalism",
        "terminal" => "Terminal",
        "blueprint" => "Blueprint",
        "cassette" => "Cassette",
        "arcade" => "Arcade",
        "paper" => "Paper",
        "frosted" => "Frosted",
        "porcelain" => "Porcelain",
        "radar" => "Radar",
        "custom" => "Custom",
        _ => "Windows",
    }
}

fn mute_sensitive(theme: &str) -> bool {
    // Custom only changes when muted in its Colored style, but keeping it
    // sensitive is harmless: the chrome is rebuilt from the same settings.
    !matches!(theme, "Windows")
}

pub(super) fn icon_svg(id: &str) -> Option<&'static str> {
    Some(match id {
        "speaker-none" => include_str!("../../assets/icons/volume/volume-bold.svg"),
        "speaker-low" => include_str!("../../assets/icons/volume/volume-small-bold.svg"),
        "speaker-loud" => include_str!("../../assets/icons/volume/volume-loud-bold.svg"),
        "speaker-mute" => include_str!("../../assets/icons/volume/volume-cross-bold.svg"),
        "device-speakers" => include_str!("../../assets/icons/volume/speaker-bold.svg"),
        "device-headphones" => include_str!("../../assets/icons/volume/headphones-round-bold.svg"),
        "device-monitor" => include_str!("../../assets/icons/volume/monitor-bold.svg"),
        "device-microphone" => include_str!("../../assets/icons/microphone-3-bold.svg"),
        // Pulldown-screen glyph. Display endpoints use the monitor; this stays
        // in the binary because the task asked for the Solar display icon.
        "device-screen" => include_str!("../../assets/icons/volume/display-bold.svg"),
        // Now-playing stand-in when the player publishes no art.
        "media-note" => include_str!("../../assets/icons/music-note-2-bold.svg"),
        "media-play" => include_str!("../../assets/icons/play-bold.svg"),
        "media-pause" => include_str!("../../assets/icons/pause-bold.svg"),
        "media-next" => include_str!("../../assets/icons/volume/skip-next-bold.svg"),
        "media-previous" => include_str!("../../assets/icons/volume/skip-previous-bold.svg"),
        // Drifting behind the "Up next" intro.
        "media-up-next" => include_str!("../../assets/icons/volume/double-alt-arrow-right-bold.svg"),
        // Cute Sticker's sparkle: the cut paper edge and the ink inside it.
        "sticker-star-paper" => {
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M12 5Q13.75 10.25 19 12Q13.75 13.75 12 19Q10.25 13.75 5 12Q10.25 10.25 12 5Z" fill="#000" stroke="#000" stroke-width="7" stroke-linejoin="round"/></svg>"##
        }
        "sticker-star-ink" => {
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M12 5Q13.75 10.25 19 12Q13.75 13.75 12 19Q10.25 13.75 5 12Q10.25 10.25 12 5Z" fill="#000" stroke="#000" stroke-width="1.5" stroke-linejoin="round"/></svg>"##
        }
        _ => return None,
    })
}

fn chrome_for(theme: &str, system: System, muted: bool, style: &VolumeOsdStyle) -> VolumeChrome {
    let theme = normalize_theme(theme);
    let mut chrome = windows_chrome(system);
    if theme != "Windows" {
        // Other themes start from the previous neutral base, not the
        // flyout-specific metrics; they keep the Fluent glyphs.
        chrome.mica = false;
        chrome.number_slot = None;
        chrome.outline = None;
        chrome.height = 49.0;
        chrome.pad_x = 16.0;
        chrome.gap = 12.0;
        chrome.bar_width = 84.0;
        chrome.border = None;
        chrome.font = fonts::DEFAULT.into();
        chrome.surface = color_alpha(0x2c2c2c, 1.0);
        chrome.foreground = 0xffffff;
        chrome.icon_color = 0xffffff;
        chrome.ink = 0xffffff;
        chrome.track = color_alpha(0xffffff, 0.18);
    }
    match theme {
        "MaterialYou" => material_you(&mut chrome, system, muted),
        "Cute" => cute(&mut chrome, muted),
        "CuteSticker" => cute_sticker(&mut chrome, muted),
        "Neon" => neon(&mut chrome, muted),
        "Brutalism" => brutalism(&mut chrome, muted),
        "Terminal" => terminal(&mut chrome, muted),
        "Blueprint" => blueprint(&mut chrome, muted),
        "Cassette" => cassette(&mut chrome, muted),
        "Arcade" => arcade(&mut chrome, muted),
        "Paper" => paper(&mut chrome, muted),
        "Frosted" => frosted(&mut chrome, muted),
        "Porcelain" => porcelain(&mut chrome, muted),
        "Radar" => radar(&mut chrome, muted),
        "Custom" => custom(&mut chrome, system, &style.custom, muted),
        _ => {}
    }
    if let Some(radius) = style.corner_radius
        && super::theme::supports_corner_radius(theme)
    {
        chrome.radius = radius.min(32.0).min(chrome.height / 2.0);
    }
    chrome
}

/// Measured against the Windows 11 volume flyout: 192x48 acrylic card,
/// Fluent glyph, 110x4 rail, bare number centered in the 40px after it, in
/// Segoe UI Variable, dark hairline around the card.
fn windows_chrome(system: System) -> VolumeChrome {
    let (surface, foreground, track, stroke) = if system.light {
        (0xf9f9f9, 0x1b1b1b, color_alpha(0x000000, 0.45), color_alpha(0x000000, 0.07))
    } else {
        (0x2c2c2c, 0xffffff, color_alpha(0xffffff, 0.54), color_alpha(0xffffff, 0.08))
    };
    VolumeChrome {
        entrance: Entrance::Nudge,
        detail: Detail::None,
        animates: false,
        halo_pulse: false,
        height: 48.0,
        radius: 8.0,
        pad_x: 12.0,
        gap: 14.0,
        icon_size: 16.0,
        icon_box: None,
        icon_color: foreground,
        surface: color_alpha(surface, 1.0),
        foreground,
        track,
        fill: color_alpha(system.accent(), 1.0),
        border: Some((stroke, 1.0)),
        // Measured on the flyout: nothing above, ~10% at the sides, ~21%
        // right under the card, gone about 20px below it.
        shadow: Shadow::Drop(color_alpha(0x000000, if system.light { 0.12 } else { 0.21 }), 10.0, 8.0),
        gutter: 22.0,
        font: "Segoe UI Variable Text".into(),
        weight: 400,
        text_size: 14.0,
        sheen: 0.0,
        bar_width: 110.0,
        bar_height: 4.0,
        ink: foreground,
        fluent: true,
        mica: true,
        number_slot: Some(40.0),
        outline: Some(color_alpha(0x000000, if system.light { 0.10 } else { 0.20 })),
        opacity: 1.0,
        bar_radius: None,
        sticker: false,
        icon_tilt: 0.0,
    }
}

fn material_you(chrome: &mut VolumeChrome, system: System, muted: bool) {
    let (hue, saturation, _) = hsl(system.accent());
    let tone = |s: f32, l: f32| from_hsl(hue, s.min(saturation.max(0.18)), l);
    let (surface, foreground, container, on_container) = match (system.light, muted) {
        (false, false) => (tone(0.12, 0.13), tone(0.10, 0.90), tone(0.45, 0.30), tone(0.80, 0.90)),
        (false, true) => (tone(0.12, 0.13), tone(0.10, 0.90), 0x8c1d18, 0xf9dedc),
        (true, false) => (tone(0.40, 0.95), tone(0.10, 0.12), tone(0.80, 0.88), tone(0.60, 0.18)),
        (true, true) => (tone(0.40, 0.95), tone(0.10, 0.12), 0xf9dedc, 0x410e0b),
    };
    chrome.entrance = Entrance::Fade;
    chrome.height = 56.0;
    chrome.radius = 28.0;
    chrome.pad_x = 12.0;
    chrome.icon_size = 20.0;
    chrome.icon_color = on_container;
    chrome.icon_box = Some(IconBox {
        size: 40.0,
        radius: Some(20.0),
        fill: color_alpha(container, 1.0),
        border: None,
    });
    chrome.surface = color_alpha(surface, 0.98);
    chrome.foreground = foreground;
    chrome.track = color_alpha(foreground, 0.16);
    chrome.fill = color_alpha(if muted { container } else { on_container }, 1.0);
    chrome.border = None;
    chrome.shadow = Shadow::Drop(color_alpha(0x000000, 0.28), 2.0, 8.0);
    chrome.gutter = 16.0;
    chrome.font = "Google Sans".into();
    chrome.weight = 500;
    chrome.text_size = 15.0;
    chrome.bar_width = 72.0;
    chrome.bar_height = 5.0;
    chrome.ink = on_container;
}

fn cute(chrome: &mut VolumeChrome, muted: bool) {
    let signal = if muted { 0xff4f93 } else { 0x1fb383 };
    let bubble = if muted { 0xffd3e5 } else { 0xc8f4e3 };
    chrome.entrance = Entrance::Fade;
    chrome.height = 52.0;
    chrome.radius = 26.0;
    chrome.pad_x = 12.0;
    chrome.icon_size = 20.0;
    chrome.icon_color = signal;
    // Solar's filled glyphs suit the bubble better than Fluent's thin lines.
    chrome.fluent = false;
    chrome.icon_box = Some(IconBox {
        size: 36.0,
        radius: Some(18.0),
        fill: color_alpha(bubble, 1.0),
        border: None,
    });
    chrome.surface = color_alpha(0xfff2f8, 1.0);
    chrome.foreground = 0x9c3b6c;
    chrome.track = color_alpha(0xffb3d3, 0.55);
    chrome.fill = color_alpha(signal, 1.0);
    chrome.border = Some((color_alpha(0xffb3d3, 1.0), 2.0));
    chrome.shadow = Shadow::Hard(color_alpha(0xffb3d3, 1.0), 0.0, 4.0);
    chrome.gutter = 8.0;
    chrome.font = "Nunito".into();
    chrome.weight = 700;
    chrome.text_size = 15.0;
    chrome.bar_width = 72.0;
    chrome.bar_height = 6.0;
    chrome.ink = signal;
}

/// The mic overlay's sticker family as a meter: white die-cut paper with no
/// outline, a round badge bulging around a pastel icon disc, a fat bar and
/// two sparkles over the edge. The soft shadow is the paper's own.
fn cute_sticker(chrome: &mut VolumeChrome, muted: bool) {
    cute(chrome, muted);
    // The mic sticker's pastel ink, and a deeper shade that small text can
    // carry on white.
    let (ink, deep) = if muted {
        (0xf49ac2, 0xde5c98)
    } else {
        (0x73cbb1, 0x35a586)
    };
    chrome.entrance = Entrance::Nudge;
    chrome.sticker = true;
    chrome.height = 46.0;
    chrome.radius = 23.0;
    chrome.gap = 14.0;
    chrome.icon_size = 22.0;
    chrome.icon_tilt = -0.14;
    chrome.icon_color = 0xffffff;
    chrome.icon_box = Some(IconBox {
        size: 40.0,
        radius: Some(20.0),
        fill: color_alpha(ink, 1.0),
        border: None,
    });
    chrome.surface = color_alpha(0xffffff, 1.0);
    chrome.foreground = deep;
    chrome.track = color_alpha(ink, 0.26);
    chrome.fill = color_alpha(ink, 1.0);
    chrome.border = None;
    chrome.shadow = Shadow::Drop(color_alpha(0x68475a, 0.30), 3.0, 6.0);
    chrome.gutter = 22.0;
    chrome.weight = 900;
    chrome.text_size = 16.0;
    chrome.bar_width = 78.0;
    chrome.bar_height = 10.0;
    chrome.ink = ink;
}

fn neon(chrome: &mut VolumeChrome, muted: bool) {
    let tube = if muted { 0xff2e88 } else { 0x19f0ff };
    chrome.entrance = Entrance::Fade;
    chrome.detail = Detail::None;
    chrome.animates = true;
    chrome.halo_pulse = true;
    chrome.height = 48.0;
    chrome.radius = 12.0;
    chrome.icon_size = 18.0;
    chrome.icon_color = tube;
    chrome.icon_box = None;
    chrome.surface = color_alpha(0x0b0614, 0.94);
    chrome.foreground = tube;
    chrome.track = color_alpha(tube, 0.22);
    chrome.fill = color_alpha(tube, 1.0);
    chrome.border = Some((color_alpha(tube, 1.0), 1.5));
    chrome.shadow = Shadow::Halo(color_alpha(tube, 0.70), 16.0);
    chrome.gutter = 52.0;
    chrome.font = "Orbitron".into();
    chrome.weight = 600;
    chrome.bar_width = 78.0;
    chrome.ink = tube;
}

fn brutalism(chrome: &mut VolumeChrome, muted: bool) {
    let slab = if muted { 0xff5a5f } else { 0xc6ff3d };
    chrome.entrance = Entrance::Slide;
    chrome.height = 50.0;
    chrome.radius = 0.0;
    chrome.pad_x = 10.0;
    chrome.icon_size = 16.0;
    chrome.icon_color = slab;
    chrome.icon_box = Some(IconBox {
        size: 30.0,
        radius: Some(0.0),
        fill: color_alpha(0x000000, 1.0),
        border: None,
    });
    chrome.surface = color_alpha(slab, 1.0);
    chrome.foreground = 0x000000;
    chrome.track = color_alpha(0x000000, 0.28);
    chrome.fill = color_alpha(0x000000, 1.0);
    chrome.border = Some((color_alpha(0x000000, 1.0), 3.0));
    chrome.shadow = Shadow::Hard(color_alpha(0x000000, 1.0), 5.0, 5.0);
    chrome.gutter = 8.0;
    chrome.font = "Archivo Black".into();
    chrome.weight = 400;
    chrome.text_size = 14.0;
    chrome.bar_width = 70.0;
    chrome.bar_height = 6.0;
    chrome.bar_radius = Some(0.0);
    chrome.ink = 0x000000;
}

fn terminal(chrome: &mut VolumeChrome, muted: bool) {
    let signal = if muted { 0xffbd69 } else { 0x7cf7b4 };
    instrument(chrome, 0x0d1914, signal, signal, Detail::Terminal, muted);
    chrome.height = 44.0;
    chrome.radius = 3.0;
    chrome.font = "Orbitron".into();
    chrome.text_size = 12.0;
    chrome.border = Some((color_alpha(signal, 0.35), 1.0));
    chrome.shadow = Shadow::Hard(color_alpha(0x06100b, 1.0), 3.0, 3.0);
    chrome.gutter = 6.0;
    chrome.bar_radius = Some(1.0);
    chrome.animates = true;
}

fn blueprint(chrome: &mut VolumeChrome, muted: bool) {
    let signal = if muted { 0xffcf9c } else { 0xa7e5ff };
    instrument(chrome, 0x153c72, 0xebf5ff, signal, Detail::Blueprint, muted);
    chrome.height = 50.0;
    chrome.radius = 0.0;
    chrome.font = "Orbitron".into();
    chrome.text_size = 12.0;
    chrome.border = Some((color_alpha(0xa7d6ff, 0.65), 1.0));
    chrome.shadow = Shadow::Hard(color_alpha(0x082244, 0.9), 4.0, 4.0);
    chrome.gutter = 8.0;
    chrome.bar_radius = Some(0.0);
    chrome.ink = 0xa7d6ff;
}

fn cassette(chrome: &mut VolumeChrome, muted: bool) {
    let signal = if muted { 0xa6463d } else { 0x2c726e };
    instrument(chrome, 0xe9dcc1, 0x39352e, signal, Detail::Cassette, muted);
    chrome.height = 56.0;
    chrome.radius = 9.0;
    chrome.font = "Google Sans".into();
    chrome.weight = 500;
    chrome.border = Some((color_alpha(0x75664e, 0.8), 1.0));
    chrome.track = color_alpha(0x75664e, 0.28);
}

fn arcade(chrome: &mut VolumeChrome, muted: bool) {
    let signal = if muted { 0xff7fa9 } else { 0x8fe8ff };
    instrument(chrome, 0x30204d, 0xffefab, signal, Detail::Arcade, muted);
    chrome.entrance = Entrance::Slide;
    chrome.height = 48.0;
    chrome.radius = 0.0;
    chrome.font = "Orbitron".into();
    chrome.weight = 800;
    chrome.text_size = 13.0;
    chrome.border = Some((color_alpha(0xb997ed, 1.0), 2.0));
    chrome.shadow = Shadow::Hard(color_alpha(0x170e2c, 1.0), 5.0, 5.0);
    chrome.gutter = 8.0;
    chrome.bar_radius = Some(0.0);
}

fn paper(chrome: &mut VolumeChrome, muted: bool) {
    let signal = if muted { 0xa23748 } else { 0x28634f };
    instrument(chrome, 0xfffdf5, 0x343b48, signal, Detail::Paper, muted);
    chrome.height = 52.0;
    chrome.radius = 2.0;
    // Clear of the red margin line at 10.
    chrome.pad_x = 20.0;
    chrome.font = "Nunito".into();
    chrome.weight = 700;
    chrome.shadow = Shadow::Hard(color_alpha(0xd6d0be, 1.0), 2.0, 3.0);
    chrome.gutter = 6.0;
    chrome.track = color_alpha(0x6c91bc, 0.25);
}

fn frosted(chrome: &mut VolumeChrome, muted: bool) {
    let signal = if muted { 0x9a476d } else { 0x245c8a };
    instrument(chrome, 0xdcecf7, 0x253e58, signal, Detail::Frosted, muted);
    chrome.height = 54.0;
    chrome.radius = 16.0;
    chrome.surface = color_alpha(0xdcecf7, 0.94);
    chrome.sheen = 0.8;
    chrome.icon_box = Some(IconBox {
        size: 32.0,
        radius: Some(10.0),
        fill: color_alpha(0xffffff, 0.62),
        border: Some((color_alpha(0xffffff, 0.9), 1.0)),
    });
    chrome.border = Some((color_alpha(0xffffff, 0.9), 1.0));
    chrome.font = "Google Sans".into();
    chrome.weight = 500;
    chrome.shadow = Shadow::Drop(color_alpha(0x4b779d, 0.28), 4.0, 10.0);
    chrome.gutter = 28.0;
}

fn porcelain(chrome: &mut VolumeChrome, muted: bool) {
    let signal = if muted { 0x9d3b43 } else { 0x315cad };
    instrument(chrome, 0xf5f6f9, 0x283d69, signal, Detail::Porcelain, muted);
    chrome.height = 58.0;
    chrome.radius = 29.0;
    chrome.pad_x = 12.0;
    chrome.icon_box = Some(IconBox {
        size: 36.0,
        radius: Some(18.0),
        fill: color_alpha(0xe5eaf5, 1.0),
        border: Some((color_alpha(0x8ca1c9, 0.6), 1.0)),
    });
    chrome.font = "Nunito".into();
    chrome.weight = 800;
    chrome.border = Some((color_alpha(0x8ca1c9, 0.7), 1.0));
    chrome.shadow = Shadow::Drop(color_alpha(0x253c65, 0.20), 4.0, 8.0);
    chrome.gutter = 24.0;
}

fn radar(chrome: &mut VolumeChrome, muted: bool) {
    let signal = if muted { 0xefb775 } else { 0x98dba0 };
    instrument(chrome, 0x182d29, 0xdae9d9, signal, Detail::Radar, muted);
    chrome.height = 52.0;
    chrome.radius = 8.0;
    chrome.font = "Orbitron".into();
    chrome.text_size = 12.0;
    chrome.border = Some((color_alpha(0x7a9c86, 0.4), 1.0));
    chrome.animates = true;
}

/// The mic overlay's Custom knobs: background, icon color style, font,
/// border and opacity.
fn custom(
    chrome: &mut VolumeChrome,
    system: System,
    look: &crate::volume_osd::CustomLook,
    muted: bool,
) {
    let (background, foreground) = if look.light_background {
        (0xf4f5f7, 0x1b1c21)
    } else {
        (0x1b1c21, 0xf1f4f8)
    };
    let accent = match look.icon_style.as_str() {
        "Colored" => {
            if muted {
                0xdc3545
            } else {
                0x28a745
            }
        }
        "Monochrome" => foreground,
        _ => system.accent(),
    };
    chrome.entrance = Entrance::Fade;
    chrome.detail = Detail::None;
    chrome.height = 48.0;
    chrome.radius = 16.0;
    chrome.surface = color_alpha(background, look.background_opacity.clamp(0.0, 1.0));
    chrome.foreground = foreground;
    chrome.icon_color = accent;
    chrome.track = color_alpha(foreground, if look.light_background { 0.18 } else { 0.14 });
    chrome.fill = color_alpha(accent, 1.0);
    chrome.border = look
        .border
        .then(|| (color_alpha(foreground, if look.light_background { 0.12 } else { 0.10 }), 1.0));
    chrome.shadow = Shadow::Drop(
        color_alpha(0x000000, if look.light_background { 0.18 } else { 0.35 }),
        3.0,
        10.0,
    );
    chrome.gutter = 18.0;
    chrome.font = if look.font.trim().is_empty() {
        fonts::DEFAULT.into()
    } else {
        look.font.clone().into()
    };
    chrome.weight = look.weight.clamp(100, 900);
    chrome.sheen = if look.light_background { 0.0 } else { 0.06 };
    chrome.ink = accent;
    chrome.opacity = look.content_opacity.clamp(0.2, 1.0);
}

fn instrument(
    chrome: &mut VolumeChrome,
    surface: u32,
    foreground: u32,
    signal: u32,
    detail: Detail,
    _muted: bool,
) {
    chrome.entrance = Entrance::Fade;
    chrome.detail = detail;
    chrome.animates = false;
    chrome.halo_pulse = false;
    chrome.height = 50.0;
    chrome.radius = 10.0;
    chrome.pad_x = 14.0;
    chrome.gap = 12.0;
    chrome.icon_size = 18.0;
    chrome.icon_box = None;
    chrome.icon_color = signal;
    chrome.surface = color_alpha(surface, 1.0);
    chrome.foreground = foreground;
    chrome.track = color_alpha(foreground, 0.16);
    chrome.fill = color_alpha(signal, 1.0);
    chrome.border = None;
    chrome.shadow = Shadow::Drop(color_alpha(0x000000, 0.24), 3.0, 8.0);
    chrome.gutter = 18.0;
    chrome.font = fonts::DEFAULT.into();
    chrome.weight = 600;
    chrome.text_size = 14.0;
    chrome.sheen = 0.0;
    chrome.bar_width = 76.0;
    chrome.bar_height = 5.0;
    chrome.ink = signal;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaker_glyphs_follow_level_and_mute() {
        assert_eq!(speaker_icon(0, false), "speaker-none");
        assert_eq!(speaker_icon(1, false), "speaker-low");
        assert_eq!(speaker_icon(33, false), "speaker-low");
        assert_eq!(speaker_icon(34, false), "speaker-loud");
        assert_eq!(speaker_icon(100, false), "speaker-loud");
        assert_eq!(speaker_icon(80, true), "speaker-mute");
    }

    #[test]
    fn theme_ids_match_the_catalogue() {
        assert_eq!(normalize_theme("Material You"), "MaterialYou");
        assert_eq!(normalize_theme("cute sticker"), "CuteSticker");
        assert_eq!(normalize_theme("nope"), "Windows");
    }
}
