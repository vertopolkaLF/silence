//! Volume flyout renderer. Runs on `silence-gpui-volume-osd`, not the mic thread.
//!
//! Solar Bold only ships three speaker glyphs (no waves, one wave, two waves)
//! plus a crossed speaker. Level 0 uses the bare speaker, 1–33 the single wave,
//! and both mid and high (34–100) use `volume-loud-bold`, the fullest glyph.
//! Cute Sticker's die-cut peel does not fit a meter, so that theme is the same
//! pastel family drawn as a compact card.

use super::{
    details, fonts,
    motion::Motion,
    theme::{Detail, IconBox, Look, Shadow, System, color_alpha, from_hsl, hsl},
};
use crate::volume_osd::{Command, DeviceKind, VolumeOsdContent, VolumeOsdStyle};
use anyhow::Result;
use gpui::{
    App, AssetSource, Bounds, BoxShadow, Context, FontWeight, IntoElement, Render, Rgba,
    SharedString, Task, TextRun, Window, WindowBackgroundAppearance, WindowBounds, WindowKind,
    WindowOptions, WindowingRequest, div, font, linear_color_stop, linear_gradient, point,
    prelude::*, px, rgb, size, svg,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    borrow::Cow,
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use windows::Win32::Foundation::HWND;

const LEVEL_MS: u64 = 120;
const CROSSFADE_MS: u64 = 200;
const SIZE_MS: u64 = 240;
const FADE_MS: u64 = 180;
const SLIDE_MS: u64 = 280;
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

pub(crate) fn run(receiver: flume::Receiver<Command>) {
    let commands: Rc<RefCell<Option<Task<()>>>> = Rc::new(RefCell::new(None));
    let app_commands = commands.clone();
    let platform = Rc::new(
        gpui_windows::WindowsPlatform::new(false).expect("initialize volume OSD platform"),
    );
    gpui::Application::with_platform(platform)
        .with_quit_mode(gpui::QuitMode::Explicit)
        .with_assets(VolumeAssets)
        .run(move |cx: &mut App| {
            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(80.), px(80.)),
                        size(px(220.), px(80.)),
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
                handle.update(cx, |_view, window, _| -> Result<()> {
                    let RawWindowHandle::Win32(raw) =
                        HasWindowHandle::window_handle(window)?.as_raw()
                    else {
                        anyhow::bail!("volume OSD requires a Windows window");
                    };
                    crate::volume_osd::attach(HWND(raw.hwnd.get() as *mut _))?;
                    Ok(())
                })??;
                *app_commands.borrow_mut() = Some(cx.spawn(async move |cx| {
                        while let Ok(command) = receiver.recv_async().await {
                            let mut batch = vec![command];
                            while let Ok(command) = receiver.try_recv() {
                                batch.push(command);
                            }
                            let quit = batch.iter().any(|command| {
                                matches!(command, Command::Shutdown)
                            });
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
                eprintln!("volume OSD startup: {error:#}");
                cx.quit();
            }
        });
    commands.borrow_mut().take();
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Entrance {
    Fade,
    Slide,
}

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
}

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

struct VolumeView {
    system: System,
    content: VolumeOsdContent,
    style: VolumeOsdStyle,
    device_name: Option<String>,
    caption_dirty: bool,
    layers: Vec<ChromeLayer>,
    glyphs: Vec<Glyph>,
    captions: Vec<Caption>,
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
    surface_ready: bool,
    fallback_armed: bool,
    background_set: bool,
    last_scale: f32,
    hide_wait: Option<Task<()>>,
    ambient_frame: Option<Task<()>>,
    detail_epoch: Instant,
}

impl VolumeView {
    fn new() -> Self {
        let system = System::load();
        let chrome = chrome_for("Windows", system, false);
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
            loaded_fonts: Vec::new(),
            needs_measure: true,
            measured: false,
            level_ready: false,
            width: Motion::new(194.0, SIZE_MS),
            height: Motion::new(49.0, SIZE_MS),
            radius: Motion::new(8.0, SIZE_MS),
            level: Motion::new(0.0, LEVEL_MS),
            visibility: Motion::new(0.0, FADE_MS),
            slide: Motion::decelerate(1.0, SLIDE_MS),
            want_visible: false,
            surface_ready: false,
            fallback_armed: false,
            background_set: false,
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
            Command::Shutdown => {}
        }
    }

    fn show(&mut self, content: VolumeOsdContent, style: VolumeOsdStyle, cx: &mut Context<Self>) {
        let now = Instant::now();
        let appearing = !self.want_visible;
        if appearing {
            self.system = System::load();
        }
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
        if !self.surface_ready {
            return;
        }
        self.visibility.retarget(1.0, now);
        if self.entrance() == Entrance::Slide {
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
        if self.entrance() == Entrance::Slide {
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
        if self.style.while_muted && self.content.is_muted() {
            self.hide_wait = None;
            return;
        }
        if self.style.duration_ms == 0 {
            self.hide_wait = None;
            self.begin_exit(now);
            return;
        }
        let wait = self.style.duration_ms as u64;
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
                layer.chrome = chrome_for(&key.0, system, muted_bit);
                matched = true;
            }
        }
        if !matched {
            let chrome = chrome_for(&key.0, system, muted_bit);
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
        let id = content_glyph(&self.content);
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
        let scale = monitor_scale / window_scale;
        if (self.last_scale - scale).abs() > 0.01 {
            self.last_scale = scale;
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

        if self.caption_dirty {
            self.caption_dirty = false;
            let chrome = self.incoming().chrome.clone();
            let measure_window = &*window;
            let fitted = self.device_name.clone().map(|name| {
                fit_name(&name, 180.0 * scale, |text| {
                    measure_text(text, &chrome, scale, measure_window)
                })
            });
            self.sync_captions(fitted, now);
            self.needs_measure = true;
        }

        let chrome = self.incoming().chrome.clone();
        let percent_slot = measure_text("100%", &chrome, scale, window).max(28.0 * scale);
        let name_width = self
            .captions
            .iter()
            .find(|caption| caption.opacity.to > 0.5)
            .map(|caption| measure_text(&caption.text, &chrome, scale, window))
            .unwrap_or(0.0);
        let pad = chrome.pad_x * scale;
        let gap = chrome.gap * scale;
        let icon_slot = chrome
            .icon_box
            .map(|icon_box| icon_box.size)
            .unwrap_or(chrome.icon_size)
            * scale;
        let bar_w = chrome.bar_width * scale;
        let bar_h = chrome.bar_height * scale;
        let base = pad * 2.0 + icon_slot + gap + bar_w + gap + percent_slot;
        let extra = if name_width > 0.0 {
            gap + name_width
        } else {
            0.0
        };
        let target_w = (base + extra).max(icon_slot + pad * 2.0);
        let target_h = (chrome.height * scale).max(1.0);
        let target_r = chrome.radius * scale;
        if self.needs_measure && target_w >= 8.0 {
            if self.measured {
                self.width.retarget(target_w, now);
                self.height.retarget(target_h, now);
                self.radius.retarget(target_r, now);
            } else {
                self.width = Motion::new(target_w, SIZE_MS);
                self.height = Motion::new(target_h, SIZE_MS);
                self.radius = Motion::new(target_r, SIZE_MS);
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
        let slide_theme = chrome.entrance == Entrance::Slide;
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

        let gutter = chrome.gutter * scale;
        crate::volume_osd::set_geometry(
            (width * window_scale).round() as i32,
            (height * window_scale).round() as i32,
            ((width + gutter * 2.0) * window_scale).round() as i32,
            ((height + gutter * 2.0) * window_scale).round() as i32,
            (gutter * window_scale).round() as i32,
            (gutter * window_scale).round() as i32,
            slide,
        );
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
        let surface = mix_rgba(weights.iter().map(|(chrome, weight)| (chrome.surface, *weight)));
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
        let name_slot = (width - base).max(0.0);
        let level = self.level.value(now).clamp(0.0, 1.0);
        let percent = self.content.percent();
        let label = format!("{percent}%");

        let mut card = div()
            .absolute()
            .left(px(gutter))
            .top(px(gutter))
            .w(px(width))
            .h(px(height))
            .rounded(px(radius))
            .opacity(alpha)
            .bg(surface);
        card = match shadow {
            Some(shadow) => card.shadow(vec![shadow]),
            None => card.shadow_md(),
        };
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
                icon = icon.child(
                    div()
                        .absolute()
                        .left(px((icon_slot - box_size) / 2.0))
                        .top(px((icon_slot - box_size) / 2.0))
                        .w(px(box_size))
                        .h(px(box_size))
                        .rounded(px(corner.min(box_size / 2.0)))
                        .bg(icon_box.fill)
                        .opacity(*weight),
                );
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
                    .child(
                        svg()
                            .path(path.clone())
                            .size(px(chrome.icon_size * scale))
                            .text_color(rgb(*color)),
                    ),
            );
        }
        let mut row = div()
            .flex()
            .items_center()
            .size_full()
            .pl(px(pad))
            .pr(px(pad))
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
        row = row.child(
            div().pl(px(gap)).flex_shrink_0().child(
                div()
                    .w(px(bar_w))
                    .h(px(bar_h.max(1.0)))
                    .rounded(px(bar_h / 2.0))
                    .bg(track)
                    .overflow_hidden()
                    .child(
                        div()
                            .h(px(bar_h.max(1.0)))
                            .w(px(bar_w * level))
                            .bg(fill),
                    ),
            ),
        );
        row = row.child(
            div()
                .pl(px(gap))
                .w(px(percent_slot))
                .flex_shrink_0()
                .flex()
                .justify_end()
                .items_center()
                .whitespace_nowrap()
                .text_size(px(chrome.text_size * scale))
                .font_family(chrome.font.clone())
                .font_weight(FontWeight(chrome.weight as f32))
                .text_color(rgb(foreground))
                .child(label),
        );
        contents = contents.child(row);
        card = card.child(contents);
        if let Some((color, border)) = chrome.border {
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
        div().relative().size_full().child(card)
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

fn border_width<E: Styled>(mut element: E, width: f32) -> E {
    let widths = &mut element.style().border_widths;
    widths.top = Some(px(width).into());
    widths.right = Some(px(width).into());
    widths.bottom = Some(px(width).into());
    widths.left = Some(px(width).into());
    element
}

fn measure_text(text: &str, chrome: &VolumeChrome, scale: f32, window: &Window) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let text: SharedString = text.to_string().into();
    let mut text_font = font(chrome.font.clone());
    text_font.weight = FontWeight(chrome.weight as f32);
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
        .shape_line(text, px(chrome.text_size * scale), &[run], None);
    f32::from(line.width)
}

fn fit_name(name: &str, max_width: f32, measure: impl Fn(&str) -> f32) -> String {
    let mut name: String = name.chars().take(64).collect();
    if name.is_empty() {
        name = "Output".to_string();
    }
    if max_width <= 1.0 || measure(&name) <= max_width {
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
        pad_icon: chrome.pad_x,
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
            DeviceKind::Speakers | DeviceKind::Other => "device-speakers",
        },
    }
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
    !matches!(theme, "Windows" | "Custom")
}

fn icon_svg(id: &str) -> Option<&'static str> {
    Some(match id {
        "speaker-none" => include_str!("../../assets/icons/volume/volume-bold.svg"),
        "speaker-low" => include_str!("../../assets/icons/volume/volume-small-bold.svg"),
        "speaker-loud" => include_str!("../../assets/icons/volume/volume-loud-bold.svg"),
        "speaker-mute" => include_str!("../../assets/icons/volume/volume-cross-bold.svg"),
        "device-speakers" => include_str!("../../assets/icons/volume/speaker-bold.svg"),
        "device-headphones" => include_str!("../../assets/icons/volume/headphones-round-bold.svg"),
        "device-monitor" => include_str!("../../assets/icons/volume/monitor-bold.svg"),
        // Pulldown-screen glyph. Display endpoints use the monitor; this stays
        // in the binary because the task asked for the Solar display icon.
        "device-screen" => include_str!("../../assets/icons/volume/display-bold.svg"),
        _ => return None,
    })
}

fn chrome_for(theme: &str, system: System, muted: bool) -> VolumeChrome {
    let theme = normalize_theme(theme);
    let mut chrome = windows_chrome(system);
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
        "Custom" => custom(&mut chrome, system),
        _ => {}
    }
    chrome
}

fn windows_chrome(system: System) -> VolumeChrome {
    VolumeChrome {
        entrance: Entrance::Slide,
        detail: Detail::None,
        animates: false,
        halo_pulse: false,
        height: 49.0,
        radius: 8.0,
        pad_x: 16.0,
        gap: 12.0,
        icon_size: 16.0,
        icon_box: None,
        icon_color: 0xffffff,
        surface: color_alpha(0x2c2c2c, 1.0),
        foreground: 0xffffff,
        track: color_alpha(0xffffff, 0.18),
        fill: color_alpha(system.accent(), 1.0),
        border: None,
        shadow: Shadow::Drop(color_alpha(0x000000, 0.42), 4.0, 12.0),
        gutter: 22.0,
        font: fonts::DEFAULT.into(),
        weight: 400,
        text_size: 14.0,
        sheen: 0.0,
        bar_width: 84.0,
        bar_height: 4.0,
        ink: 0xffffff,
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
    chrome.radius = 18.0;
    chrome.pad_x = 12.0;
    chrome.icon_size = 18.0;
    chrome.icon_color = on_container;
    chrome.icon_box = Some(IconBox {
        size: 36.0,
        radius: Some(18.0),
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
    chrome.radius = 18.0;
    chrome.pad_x = 12.0;
    chrome.icon_size = 18.0;
    chrome.icon_color = signal;
    chrome.icon_box = Some(IconBox {
        size: 32.0,
        radius: Some(16.0),
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
    chrome.bar_width = 72.0;
    chrome.ink = signal;
}

fn cute_sticker(chrome: &mut VolumeChrome, muted: bool) {
    cute(chrome, muted);
    let ink = if muted { 0xf49ac2 } else { 0x73cbb1 };
    chrome.surface = color_alpha(0xffffff, 1.0);
    chrome.foreground = ink;
    chrome.icon_color = ink;
    chrome.fill = color_alpha(ink, 1.0);
    chrome.icon_box = None;
    chrome.border = Some((color_alpha(ink, 1.0), 3.0));
    chrome.shadow = Shadow::Hard(color_alpha(0xf3c6db, 1.0), 3.0, 4.0);
    chrome.gutter = 8.0;
    chrome.radius = 16.0;
    chrome.height = 54.0;
    chrome.weight = 800;
    chrome.text_size = 15.0;
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
}

fn paper(chrome: &mut VolumeChrome, muted: bool) {
    let signal = if muted { 0xa23748 } else { 0x28634f };
    instrument(chrome, 0xfffdf5, 0x343b48, signal, Detail::Paper, muted);
    chrome.height = 52.0;
    chrome.radius = 2.0;
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
    chrome.radius = 22.0;
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

fn custom(chrome: &mut VolumeChrome, system: System) {
    // User custom colors are not wired yet. This is the default dark custom card.
    chrome.entrance = Entrance::Fade;
    chrome.detail = Detail::None;
    chrome.height = 48.0;
    chrome.radius = 16.0;
    chrome.surface = color_alpha(0x1b1c21, 0.96);
    chrome.foreground = 0xf1f4f8;
    chrome.icon_color = system.accent();
    chrome.track = color_alpha(0xffffff, 0.14);
    chrome.fill = color_alpha(system.accent(), 1.0);
    chrome.border = Some((color_alpha(0xffffff, 0.10), 1.0));
    chrome.shadow = Shadow::Drop(color_alpha(0x000000, 0.35), 3.0, 10.0);
    chrome.gutter = 18.0;
    chrome.font = fonts::DEFAULT.into();
    chrome.weight = 500;
    chrome.sheen = 0.06;
    chrome.ink = system.accent();
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
