//! A GPU-composited overlay on its own Windows UI thread.
//! Dioxus settings and the existing audio/tray message loop remain independent.
// Rounded decoration clipping also handles coincident arc endpoints at full pill radius.
mod clip;
mod details;
pub(crate) mod fonts;
mod motion;
mod sticker;
pub(crate) mod theme;

use crate::{OverlayConfig, native_overlay};
use anyhow::{Context as _, Result};
use gpui::{
    App, AssetSource, Bounds, BoxShadow, Context, FontWeight, IntoElement, Render, SharedString,
    TextRun, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    WindowingRequest, div, font, linear_color_stop, linear_gradient, point, prelude::*, px, rgb,
    size, svg,
};
use motion::Motion;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    borrow::Cow,
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use theme::{Look, Shadow, System, color_alpha};
use windows::Win32::Foundation::HWND;

pub(super) enum Command {
    Refresh,
    SurfaceReady,
    Shutdown,
}

#[derive(Clone)]
pub(super) struct Snapshot {
    pub muted: bool,
    pub settings: OverlayConfig,
    pub visible: bool,
    pub positioning: bool,
}

/// Embed the existing Iconify assets; installed apps need no source-tree paths.
struct OverlayAssets;
impl AssetSource for OverlayAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == "warning" {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../../assets/icons/solar-danger-triangle-linear.svg"
            ))));
        }
        let Some((pair, state)) = path.split_once('/') else {
            return Ok(None);
        };
        Ok(Some(Cow::Borrowed(
            crate::overlay_icons::overlay_icon_svg(pair, state == "muted").as_bytes(),
        )))
    }
    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

pub(super) fn start(muted: bool, settings: OverlayConfig) -> Result<()> {
    let (sender, receiver) = flume::unbounded();
    native_overlay::prepare(muted, settings, sender.clone())?;
    thread::Builder::new()
        .name("silence-gpui-overlay".into())
        .spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let checkpoint = Arc::new(Mutex::new(None));
                loop {
                    // An enabled overlay stays warm between visibility changes.
                    // Only disabling it releases the platform/DirectX resources;
                    // rebuilding them on every show delays the first pixels.
                    let Some(snapshot) = native_overlay::snapshot() else {
                        break;
                    };
                    if !snapshot.visible && !snapshot.settings.enabled {
                        match receiver.recv() {
                            Ok(Command::Refresh) => continue,
                            _ => break,
                        }
                    }
                    let resumed = checkpoint.lock().unwrap().take();
                    let surface_checkpoint = checkpoint.clone();
                    let surface_sender = sender.clone();
                    let surface_receiver = receiver.clone();
                    // Finish the OS thread too: COM/DirectWrite/driver TLS caches
                    // must not survive into the next theme's renderer.
                    thread::Builder::new()
                        .name("silence-gpui-surface".into())
                        .spawn(move || {
                            run_surface(
                                snapshot,
                                resumed,
                                surface_checkpoint,
                                surface_sender,
                                surface_receiver,
                            )
                        })
                        .expect("start GPUI surface thread")
                        .join()
                        .expect("GPUI surface thread panicked");
                    native_overlay::suspend_surface();
                    release_unused_heap_pages();
                    if checkpoint.lock().unwrap().is_none() {
                        break;
                    }
                }
                checkpoint.lock().unwrap().take();
                sticker::clear_cache();
            }));
            if outcome.is_err() {
                eprintln!("GPUI overlay initialization panicked; see stderr");
            }
            native_overlay::detach();
        })
        .context("start GPUI overlay UI thread")?;
    Ok(())
}

fn run_surface(
    initial: Snapshot,
    resumed: Option<ResumeState>,
    checkpoint: Arc<Mutex<Option<ResumeState>>>,
    sender: flume::Sender<Command>,
    receiver: flume::Receiver<Command>,
) {
    // Dropping this task cancels its pending recv before the next platform starts.
    // A detached receiver could otherwise steal commands from the new surface.
    let commands = Rc::new(RefCell::new(None));
    let app_commands = commands.clone();
    let platform = Rc::new(
        gpui_windows::WindowsPlatform::new(false).expect("initialize GPUI Windows platform"),
    );
    gpui::Application::with_platform(platform)
        .with_quit_mode(gpui::QuitMode::Explicit)
        .with_assets(OverlayAssets)
        .run(move |cx: &mut App| {
            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(100.), px(100.)),
                        size(px(72.), px(72.)),
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
                |window, cx| {
                    cx.new(|_| {
                        let mut view =
                            OverlayView::new(initial.clone(), window, checkpoint.clone());
                        if let Some(resumed) = resumed {
                            view.state = resumed.state;
                            view.layers = resumed.layers;
                            view.width = resumed.width;
                            view.height = resumed.height;
                            view.visibility = resumed.visibility;
                            view.slide = resumed.slide;
                            view.measured = resumed.measured;
                            view.detail_epoch = resumed.detail_epoch;
                        }
                        view.accept(initial.clone());
                        view.runtime_theme = initial.settings.theme.clone();
                        view.runtime_font = required_font(&Look::resolve(&initial, view.system));
                        view.loaded_fonts.clear();
                        view.recycle_requested = false;
                        view.needs_measure = true;
                        view.last_chrome = None;
                        view
                    })
                },
            );
            let result = opened.and_then(|handle| {
                handle.update(cx, |_, window, _| -> Result<()> {
                    let RawWindowHandle::Win32(raw) =
                        HasWindowHandle::window_handle(window)?.as_raw()
                    else {
                        anyhow::bail!("GPUI overlay requires a Windows window");
                    };
                    native_overlay::attach(
                        HWND(raw.hwnd.get() as *mut _),
                        initial.muted,
                        initial.settings,
                        sender.clone(),
                    )?;
                    // A hidden GPUI window may not receive its first paint. Present
                    // the native surface before requesting the initial render.
                    if native_overlay::snapshot().is_some_and(|snapshot| snapshot.visible) {
                        native_overlay::present(true);
                    }
                    let _ = sender.send(Command::Refresh);
                    Ok(())
                })??;
                *app_commands.borrow_mut() = Some(cx.spawn(async move |cx| {
                    while let Ok(command) = receiver.recv_async().await {
                        let mut surface_ready = matches!(command, Command::SurfaceReady);
                        let mut shutdown = matches!(command, Command::Shutdown);
                        while let Ok(command) = receiver.try_recv() {
                            shutdown |= matches!(command, Command::Shutdown);
                            surface_ready |= matches!(command, Command::SurfaceReady);
                        }
                        if shutdown {
                            checkpoint.lock().unwrap().take();
                            let _ = cx.update(|cx| cx.quit());
                            break;
                        }
                        let Some(snapshot) = native_overlay::snapshot() else {
                            break;
                        };
                        if snapshot.visible {
                            native_overlay::present(true);
                        }
                        if handle
                            .update(cx, |view, _, cx| {
                                view.surface_ready |= surface_ready;
                                view.accept(snapshot);
                                cx.notify();
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }));
                Ok(())
            });
            if let Err(error) = result {
                eprintln!("GPUI overlay startup: {error:#}");
                cx.quit();
            }
        });
    commands.borrow_mut().take();
}

fn required_font(look: &Look) -> Option<SharedString> {
    look.has_text.then(|| look.font.clone())
}

fn release_unused_heap_pages() {
    use windows::Win32::{
        Foundation::HANDLE,
        System::{
            Memory::{HeapOptimizeResources, HeapSetInformation},
            SystemServices::{
                HEAP_OPTIMIZE_RESOURCES_CURRENT_VERSION, HEAP_OPTIMIZE_RESOURCES_INFORMATION,
            },
        },
    };
    // Return genuinely freed LFH allocations after destroying a renderer/font
    // collection. This decommits unused heap pages; it does not trim the working
    // set or page out live resources to make Task Manager look smaller.
    let info = HEAP_OPTIMIZE_RESOURCES_INFORMATION {
        Version: HEAP_OPTIMIZE_RESOURCES_CURRENT_VERSION,
        Flags: 0,
    };
    unsafe {
        let _ = HeapSetInformation(
            HANDLE::default(),
            HeapOptimizeResources,
            Some((&info as *const HEAP_OPTIMIZE_RESOURCES_INFORMATION).cast()),
            std::mem::size_of_val(&info),
        );
    }
}

#[derive(Clone)]
struct ContentLayer {
    state: Snapshot,
    opacity: Motion,
}

struct ResumeState {
    state: Snapshot,
    layers: Vec<ContentLayer>,
    width: Motion,
    height: Motion,
    visibility: Motion,
    slide: Motion,
    measured: bool,
    detail_epoch: Instant,
}

struct OverlayView {
    checkpoint: Arc<Mutex<Option<ResumeState>>>,
    runtime_theme: String,
    runtime_font: Option<SharedString>,
    loaded_fonts: Vec<SharedString>,
    recycle_requested: bool,
    surface_ready: bool,
    state: Snapshot,
    layers: Vec<ContentLayer>,
    needs_measure: bool,
    measured: bool,
    width: Motion,
    height: Motion,
    visibility: Motion,
    /// Acrylic's stand-in for fading: 0 = in place, 1 = past the screen edge.
    slide: Motion,
    last_dpi: f32,
    system: System,
    last_chrome: Option<(bool, bool)>,
    /// A stable clock for ambient details; state updates never restart the pulse.
    detail_epoch: Instant,
}

impl OverlayView {
    fn new(state: Snapshot, window: &Window, checkpoint: Arc<Mutex<Option<ResumeState>>>) -> Self {
        Self {
            checkpoint,
            runtime_theme: state.settings.theme.clone(),
            runtime_font: required_font(&Look::resolve(&state, System::load())),
            loaded_fonts: Vec::new(),
            recycle_requested: false,
            surface_ready: false,
            layers: vec![ContentLayer {
                state: state.clone(),
                opacity: Motion::new(1., 220),
            }],
            state,
            needs_measure: true,
            measured: false,
            width: Motion::new(48., 260),
            height: Motion::new(48., 260),
            visibility: Motion::new(0., 180),
            slide: Motion::decelerate(1., 420),
            last_dpi: window.scale_factor(),
            system: System::load(),
            last_chrome: None,
            detail_epoch: Instant::now(),
        }
    }

    fn accept(&mut self, next: Snapshot) {
        let now = Instant::now();
        self.needs_measure = true;
        if self.state.muted != next.muted || self.state.settings != next.settings {
            if content_changed(&self.state, &next) {
                let mut matched = false;
                for layer in &mut self.layers {
                    let current = !content_changed(&layer.state, &next);
                    layer.opacity.retarget(if current { 1. } else { 0. }, now);
                    if current {
                        layer.state = next.clone();
                        matched = true;
                    }
                }
                if !matched {
                    let mut opacity = Motion::new(0., 220);
                    opacity.retarget(1., now);
                    self.layers.push(ContentLayer {
                        state: next.clone(),
                        opacity,
                    });
                }
            } else {
                for layer in &mut self.layers {
                    if layer.opacity.to == 1. {
                        layer.state = next.clone();
                    }
                }
            }
            self.needs_measure = true;
            self.system = System::load();
        }
        if self.surface_ready {
            self.visibility
                .retarget(if next.visible { 1. } else { 0. }, now);
            // Preserve the current interpolated lift if visibility reverses mid-peel.
            self.slide.retarget_with_duration(
                if next.visible { 0. } else { 1. },
                now,
                if next.settings.theme == "CuteSticker" {
                    if next.visible { 560 } else { 440 }
                } else {
                    420
                },
            );
        }
        self.state = next;
    }

    fn content(
        &self,
        look: &Look,
        scale: f32,
        height: f32,
        radius: f32,
        opacity: f32,
        window: &Window,
    ) -> gpui::Div {
        if look.icon_path.starts_with("cute-sticker/") {
            return sticker::render(
                look,
                self.width.value(Instant::now()),
                height,
                scale,
                0.,
                opacity,
            );
        }
        let icon_only = look.has_icon && !look.has_text;
        let terminal = matches!(look.detail, theme::Detail::Terminal);
        let padding = if terminal {
            0.
        } else if look.has_text {
            if look.has_icon {
                look.pad_icon * scale
            } else {
                look.pad * scale
            }
        } else {
            0.
        };
        div()
            .absolute()
            .left(px(padding))
            .when(icon_only || terminal, |row| {
                row.w(px(self.width.value(Instant::now()))).justify_center()
            })
            .top_0()
            .h(px(height))
            .flex()
            .items_center()
            .gap(px(look.gap * scale))
            .opacity(opacity * look.content_opacity)
            .when(look.has_icon, |row| {
                let glyph = svg()
                    .path(look.icon_path.clone())
                    .size(px(look.icon_size * scale))
                    .text_color(rgb(look.icon));
                let holder = div().flex_shrink_0().flex().items_center().justify_center();
                row.child(match &look.icon_box {
                    Some(icon_box) => {
                        let size = icon_box.size * scale;
                        let corner = match icon_box.radius {
                            Some(corner) => corner * scale,
                            // Concentric with the card around it.
                            None => (radius * size / height.max(1.)).clamp(0., 16. * scale),
                        };
                        let mut holder = holder
                            .size(px(size))
                            .rounded(px(corner.min(size / 2.)))
                            .bg(icon_box.fill);
                        if let Some((color, width)) = icon_box.border {
                            holder = border_width(holder, width * scale).border_color(color);
                        }
                        holder.child(glyph)
                    }
                    None => holder.size(px(look.icon_size * scale)).child(glyph),
                })
            })
            .when(look.has_text, |row| {
                row.child(
                    div()
                        .relative()
                        .top(px(text_center_offset(look, scale, window)))
                        .whitespace_nowrap()
                        .text_size(px(look.text_size * scale))
                        .line_height(px(look.text_size * scale * 20. / 14.))
                        .font_family(look.font.clone())
                        .font_weight(FontWeight(look.weight as f32))
                        .text_color(rgb(look.foreground))
                        .when(terminal, |text| {
                            // Include the cursor in the flex item's width so the
                            // complete icon + label + cursor group is centered.
                            text.w(px(terminal_text_width(look, scale, window)))
                                .flex_shrink_0()
                        })
                        .child(look.label.clone())
                        .when(terminal, |text| {
                            // The cursor follows the shaped label, not the card edge.
                            // Keep its slot while hidden so blinking never resizes the card.
                            let label_width = f32::from(shape_label(look, scale, window).width);
                            let visible = self.detail_epoch.elapsed().as_millis() % 1000 < 500;
                            text.child(
                                div()
                                    .absolute()
                                    .left(px(label_width))
                                    .top_0()
                                    .opacity(if visible { 1. } else { 0. })
                                    .child("_"),
                            )
                        }),
                )
            })
    }

    /// State-tinted wash behind the icon side; it fades with its content layer.
    fn glow(&self, look: &Look, opacity: f32, radius: f32) -> Option<gpui::Div> {
        if !look.has_icon || !look.has_text || look.glow == 0. || look.surface.a == 0. {
            return None;
        }
        let alpha = look.glow * look.surface.a;
        Some(
            div()
                .absolute()
                .left_0()
                .top_0()
                .size_full()
                .rounded(px(radius))
                .opacity(opacity)
                .bg(linear_gradient(
                    90.,
                    linear_color_stop(color_alpha(look.icon, alpha), 0.),
                    linear_color_stop(color_alpha(look.icon, 0.), 0.7),
                )),
        )
    }
}

impl Render for OverlayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let dpi = window.scale_factor();
        // Respect selected-monitor DPI even before WM_DPICHANGED reaches the new window.
        let scale = native_overlay::scale() / dpi;
        if self.last_dpi != dpi {
            self.last_dpi = dpi;
            self.needs_measure = true;
        }
        // Only live crossfade layers may need fonts. Icon-only themes need none.
        for layer in &self.layers {
            let look = Look::resolve(&layer.state, self.system);
            if let Some(family) = required_font(&look) {
                if !self.loaded_fonts.contains(&family) {
                    if let Some(data) = fonts::embedded(&family) {
                        if let Err(error) = cx.text_system().add_fonts(vec![data]) {
                            eprintln!("load overlay font {family}: {error:#}");
                        }
                    }
                    self.loaded_fonts.push(family);
                }
            }
        }
        let look = Look::resolve(&self.state, self.system);
        let chrome = (look.acrylic, !self.system.light);
        if self.last_chrome != Some(chrome) {
            if look.acrylic {
                // GPUI's Blurred accent ignores DWM corners and paints a square;
                // clear the accent and let DWM's own backdrop show through instead.
                window.set_background_appearance(WindowBackgroundAppearance::Opaque);
                window.set_background_appearance(WindowBackgroundAppearance::MicaBackdrop);
            } else {
                window.set_background_appearance(WindowBackgroundAppearance::Transparent);
            }
            // The first frame renders before the HWND is attached; retry until it lands.
            if native_overlay::set_chrome(look.acrylic, !self.system.light) {
                self.last_chrome = Some(chrome);
            }
        }
        if self.needs_measure {
            let target_width = measure_width(&look, scale, window);
            let target_height = look.height * scale
                + if look.icon_path.starts_with("cute-sticker/") {
                    (target_width - 132. * scale).max(0.) * 0.18
                } else {
                    0.
                };
            if self.measured {
                self.width.retarget(target_width, now);
                self.height.retarget(target_height, now);
            } else {
                self.width = Motion::new(target_width, 260);
                self.height = Motion::new(target_height, 260);
                self.measured = true;
            }
            self.needs_measure = false;
        }

        let width = self.width.value(now);
        let height = self.height.value(now);
        // Prepare the complete scene at full opacity offscreen. A transparent
        // warm-up frame skips GPU primitives, leaving their cold allocation to
        // consume the entrance animation instead.
        let alpha = if self.surface_ready {
            self.visibility.value(now)
        } else {
            1.
        };
        let is_sticker = look.icon_path.starts_with("cute-sticker/");
        let slide = if !self.surface_ready {
            1.
        } else if look.acrylic {
            self.slide.value(now)
        } else {
            0.
        };
        self.layers
            .retain(|layer| layer.opacity.to > 0. || layer.opacity.active(now));
        let content_animating = self.layers.iter().any(|layer| layer.opacity.active(now));
        let detail_animating = (self.state.visible || (self.surface_ready && alpha > 0.001))
            && self.layers.iter().any(|layer| {
                matches!(
                    layer.state.settings.theme.as_str(),
                    "Radar" | "Terminal" | "Neon"
                ) && layer.opacity.value(now) > 0.001
            });
        let detail_seconds = now.duration_since(self.detail_epoch).as_secs_f32();
        let detail_phase = detail_seconds / 2.4;
        // Four-second breathing cycle, never extinguishing the neon halo.
        let neon_pulse = 0.825 + 0.175 * (std::f32::consts::TAU * detail_seconds / 4.).cos();
        if self.width.active(now)
            || self.height.active(now)
            || self.visibility.active(now)
            || ((look.acrylic || is_sticker) && self.slide.active(now))
            || content_animating
            || detail_animating
        {
            window.request_animation_frame();
        }
        // The surface follows the animated card plus the theme's shadow gutter.
        let gutter = if is_sticker {
            (look.gutter * scale).max(width * 0.20)
        } else {
            look.gutter * scale
        };
        native_overlay::set_geometry(
            (width * dpi).round() as i32,
            (height * dpi).round() as i32,
            ((width + gutter * 2.) * dpi).round() as i32,
            ((height + gutter * 2.) * dpi).round() as i32,
            (gutter * dpi).round() as i32,
            (gutter * dpi).round() as i32,
            slide,
        );
        let should_present = self.state.visible
            || (self.surface_ready
                && if look.acrylic {
                    slide < 0.999
                } else if is_sticker {
                    self.slide.value(now) < 0.999
                } else {
                    alpha > 0.001
                });
        native_overlay::present(should_present);
        // A hidden, enabled surface stays ready for its next entrance. A disabled
        // overlay can retire as soon as its exit stops contributing pixels.
        let settled = !should_present
            || (!self.width.active(now)
                && !self.height.active(now)
                && !self.visibility.active(now)
                && (!(look.acrylic || is_sticker) || !self.slide.active(now))
                && !content_animating);
        if settled && (self.surface_ready || !self.state.settings.enabled) {
            let sticker_active = self.layers.iter().any(|layer| {
                Look::resolve(&layer.state, self.system)
                    .icon_path
                    .starts_with("cute-sticker/")
            });
            if !sticker_active {
                sticker::clear_cache();
            }
            // DirectWrite has no font removal API in GPUI. Retire the whole platform
            // after the crossfade, retaining geometry and animation clocks only.
            if !self.recycle_requested
                && ((!should_present && !self.state.settings.enabled)
                    || self.runtime_theme != self.state.settings.theme
                    || self.runtime_font != required_font(&look))
            {
                self.recycle_requested = true;
                cx.defer_in(window, |view, window, cx| {
                    *view.checkpoint.lock().unwrap() = Some(ResumeState {
                        state: view.state.clone(),
                        layers: view.layers.clone(),
                        width: view.width.clone(),
                        height: view.height.clone(),
                        visibility: view.visibility.clone(),
                        slide: view.slide.clone(),
                        measured: view.measured,
                        detail_epoch: view.detail_epoch,
                    });
                    // WindowsWindow::drop schedules RevokeDragDrop/DestroyWindow
                    // on GPUI's executor. Keep pumping it before quitting; quitting
                    // first strands the HWND and its renderer behind COM references.
                    window.remove_window();
                    cx.defer(|cx| {
                        cx.spawn(async move |cx| {
                            cx.background_executor()
                                .timer(Duration::from_millis(100))
                                .await;
                            let _ = cx.update(|cx| {
                                cx.request_windowing(WindowingRequest::Headless).detach();
                                cx.quit();
                            });
                        })
                        .detach();
                    });
                });
            }
        }

        // Blend the actual on-screen layers, so quick toggles cannot flash a full old state.
        let layers = self
            .layers
            .iter()
            .map(|layer| {
                (
                    Look::resolve(&layer.state, self.system),
                    layer.opacity.value(now),
                )
            })
            .collect::<Vec<_>>();
        if is_sticker {
            let lift = if self.surface_ready {
                self.slide.value(now)
            } else {
                0.
            };
            let mut artwork = div()
                .absolute()
                .left(px(gutter))
                .top(px(gutter))
                .w(px(width))
                .h(px(height));
            for (layer_look, weight) in &layers {
                if layer_look.icon_path.starts_with("cute-sticker/") {
                    artwork = artwork.child(sticker::render(
                        layer_look, width, height, scale, lift, *weight,
                    ));
                } else {
                    artwork =
                        artwork.child(self.content(layer_look, scale, height, 0., *weight, window));
                }
            }
            if self.state.positioning {
                artwork = artwork.child(
                    div()
                        .absolute()
                        .size_full()
                        .border_1()
                        .border_color(color_alpha(0x78a8ff, 0.9))
                        .rounded(px(12. * scale)),
                );
            }
            return div().relative().size_full().child(artwork);
        }
        let blend = |pick: &dyn Fn(&Look) -> Option<gpui::Rgba>| {
            let mut mixed = gpui::Rgba::default();
            for (look, weight) in &layers {
                if let Some(color) = pick(look) {
                    mixed.r += color.r * weight;
                    mixed.g += color.g * weight;
                    mixed.b += color.b * weight;
                    mixed.a += color.a * weight;
                }
            }
            mixed
        };
        let bg = blend(&|look| Some(look.surface));
        // DWM already clips acrylic windows to its own corner radius.
        let radius = if look.acrylic {
            0.
        } else {
            (look.radius * scale).min(height / 2.).min(width / 2.)
        };
        let shadow =
            |offset: (f32, f32), blur: f32, pick: &dyn Fn(&Shadow) -> Option<gpui::Rgba>| {
                BoxShadow {
                    color: blend(&|look| pick(&look.shadow)).into(),
                    offset: point(px(offset.0 * scale), px(offset.1 * scale)),
                    blur_radius: px(blur * scale),
                    spread_radius: px(0.),
                    inset: false,
                }
            };
        let shadow = match look.shadow {
            Shadow::Soft => None,
            Shadow::Drop(_, y, blur) => Some(shadow((0., y), blur, &|shadow| match shadow {
                Shadow::Drop(color, ..) => Some(*color),
                _ => None,
            })),
            Shadow::Hard(_, x, y) => Some(shadow((x, y), 0., &|shadow| match shadow {
                Shadow::Hard(color, ..) => Some(*color),
                _ => None,
            })),
            Shadow::Halo(_, blur) => Some(shadow((0., 0.), blur, &|shadow| match shadow {
                Shadow::Halo(color, _) => {
                    let mut color = *color;
                    color.a *= neon_pulse;
                    Some(color)
                }
                _ => None,
            })),
        };
        let mut card = div()
            .absolute()
            .left(px(gutter))
            .top(px(gutter))
            .w(px(width))
            .h(px(height))
            .rounded(px(radius))
            // Acrylic slides away whole instead (see set_geometry); it cannot fade.
            .opacity(if look.acrylic { 1. } else { alpha });
        if look.surface.a > 0. {
            card = card.bg(bg);
        }
        card = match shadow {
            Some(shadow) => card.shadow(vec![shadow]),
            None if look.surface.a > 0. && !look.acrylic => card.shadow_md(),
            None => card,
        };
        let mut contents = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .rounded(px(radius));
        if look.sheen > 0. && look.surface.a > 0. {
            // Soft top sheen gives the flat surface some depth.
            contents = contents.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .size_full()
                    .rounded(px(radius))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(color_alpha(0xffffff, look.sheen * look.surface.a), 0.),
                        linear_color_stop(color_alpha(0xffffff, 0.), 0.6),
                    )),
            );
        }
        for (layer_look, weight) in &layers {
            if let Some(glow) = self.glow(layer_look, *weight, radius) {
                contents = contents.child(glow);
            }
        }
        for (layer_look, weight) in &layers {
            if !layer_look.dot {
                contents = contents.child(details::render(
                    layer_look,
                    width,
                    height,
                    scale,
                    *weight,
                    detail_phase,
                    radius,
                ));
            }
        }
        for (layer_look, weight) in &layers {
            if !layer_look.dot {
                contents = contents
                    .child(self.content(layer_look, scale, height, radius, *weight, window));
            }
        }
        card = card.child(contents);
        let border = if self.state.positioning {
            Some((color_alpha(0x78a8ff, 0.9), 1.))
        } else {
            look.border
                .map(|(_, width)| (blend(&|look| look.border.map(|(color, _)| color)), width))
        };
        if let Some((color, border_px)) = border {
            // Paint inside the existing bounds, independently of content layout.
            card = card.child(
                border_width(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(width))
                        .h(px(height))
                        .rounded(px(radius)),
                    (border_px * scale).max(1.),
                )
                .border_color(color),
            );
        }
        div().relative().size_full().child(card)
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

fn measure_width(look: &Look, scale: f32, window: &mut Window) -> f32 {
    if look.icon_path.starts_with("cute-sticker/") {
        return if look.has_text {
            let padding = if look.has_icon { 90. } else { 30. };
            (f32::from(shape_label(look, scale, window).width) + padding * scale).max(132. * scale)
        } else {
            118. * scale
        };
    }
    let height = look.height * scale;
    if !look.has_text {
        return height;
    }
    let text = if matches!(look.detail, theme::Detail::Terminal) {
        terminal_text_width(look, scale, window)
    } else {
        f32::from(shape_label(look, scale, window).width)
    };
    let chrome = if look.has_icon {
        let icon = look
            .icon_box
            .as_ref()
            .map_or(look.icon_size, |icon_box| icon_box.size);
        look.pad_icon + icon + look.gap + look.pad
    } else {
        look.pad * 2.
    };
    (chrome * scale + text).max(height)
}

fn terminal_text_width(look: &Look, scale: f32, window: &Window) -> f32 {
    f32::from(shape_label(look, scale, window).width)
        + f32::from(shape_text(look, "_".into(), scale, window).width)
}

fn shape_label(look: &Look, scale: f32, window: &Window) -> gpui::ShapedLine {
    shape_text(look, look.label.clone(), scale, window)
}

fn shape_text(look: &Look, text: SharedString, scale: f32, window: &Window) -> gpui::ShapedLine {
    let mut text_font = font(look.font.clone());
    text_font.weight = FontWeight(look.weight as f32);
    let run = TextRun {
        len: text.len(),
        font: text_font,
        color: rgb(0xffffff).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(text, px(look.text_size * scale), &[run], None)
}

fn text_center_offset(look: &Look, scale: f32, window: &Window) -> f32 {
    let line = shape_label(look, scale, window);
    let size = px(look.text_size * scale);
    let mut bottom = f32::INFINITY;
    let mut top = f32::NEG_INFINITY;
    for (index, ch) in line
        .text
        .char_indices()
        .filter(|(_, ch)| !ch.is_whitespace())
    {
        let Some(font_id) = line.font_id_for_index(index) else {
            continue;
        };
        let Ok(bounds) = window.text_system().typographic_bounds(font_id, size, ch) else {
            continue;
        };
        if bounds.size.height <= px(0.) {
            continue;
        }
        // DirectWrite typographic bounds use a baseline origin with Y upwards.
        // Descenders hang below the baseline, like in system UI, so they do not
        // pull the letters off the icon's center line.
        bottom = bottom.min(f32::from(bounds.origin.y).max(0.));
        top = top.max(f32::from(bounds.origin.y + bounds.size.height));
    }
    if !bottom.is_finite() || !top.is_finite() {
        return 0.;
    }
    // GPUI centers ascent + descent in the line box. Move that baseline so the
    // visible letters above it, including accents, are centered instead.
    (top + bottom - f32::from(line.ascent) + f32::from(line.descent)) / 2.
}

fn content_changed(old: &Snapshot, new: &Snapshot) -> bool {
    let a = &old.settings;
    let b = &new.settings;
    old.muted != new.muted
        || a.theme != b.theme
        || a.variant != b.variant
        || a.show_text != b.show_text
        || a.icon_pair != b.icon_pair
        || a.icon_style != b.icon_style
        || a.muted_label != b.muted_label
        || a.unmuted_label != b.unmuted_label
        || a.text_font != b.text_font
        || a.text_font_weight != b.text_font_weight
}
