//! A GPU-composited overlay on its own Windows UI thread.
//! Dioxus settings and the existing audio/tray message loop remain independent.
mod motion;

use crate::{OverlayConfig, native_overlay};
use anyhow::{Context as _, Result};
use gpui::{
    App, AssetSource, Bounds, Context, FontWeight, IntoElement, Render, SharedString, TextRun,
    Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, font, point,
    prelude::*, px, rgb, rgba, size, svg,
};
use motion::Motion;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{borrow::Cow, rc::Rc, sync::mpsc, thread, time::Instant};
use windows::Win32::Foundation::HWND;

pub(super) enum Command {
    Refresh,
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
    let (ready, startup) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("silence-gpui-overlay".into())
        .spawn(move || {
            let failure = ready.clone();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                gpui::Application::with_platform(Rc::new(
                    gpui_windows::WindowsPlatform::new(false)
                        .expect("initialize GPUI Windows platform"),
                ))
                .with_assets(OverlayAssets)
                .run(move |cx: &mut App| {
                    let initial = Snapshot {
                        muted,
                        settings: settings.clone(),
                        visible: false,
                        positioning: false,
                    };
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
                        |window, cx| cx.new(|_| OverlayView::new(initial, window)),
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
                                muted,
                                settings,
                                sender.clone(),
                            )?;
                            let _ = sender.send(Command::Refresh);
                            Ok(())
                        })??;
                        cx.spawn(async move |cx| {
                            while let Ok(command) = receiver.recv_async().await {
                                let mut shutdown = matches!(command, Command::Shutdown);
                                // Coalesce bursts of slider/settings updates into the latest state.
                                while let Ok(command) = receiver.try_recv() {
                                    shutdown |= matches!(command, Command::Shutdown);
                                }
                                if shutdown {
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
                                        view.accept(snapshot);
                                        cx.notify();
                                    })
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        })
                        .detach();
                        Ok(())
                    });
                    let failed = result.is_err();
                    let _ = ready.send(result.map_err(|error| format!("{error:#}")));
                    if failed {
                        cx.quit();
                    }
                });
            }));
            if outcome.is_err() {
                let _ = failure.send(Err(
                    "GPUI overlay initialization panicked; see stderr".into()
                ));
            }
            native_overlay::detach();
        })
        .context("start GPUI overlay UI thread")?;
    startup
        .recv()
        .context("GPUI overlay stopped during startup")?
        .map_err(anyhow::Error::msg)
}

struct ContentLayer {
    state: Snapshot,
    opacity: Motion,
}

struct OverlayView {
    state: Snapshot,
    layers: Vec<ContentLayer>,
    needs_measure: bool,
    measured: bool,
    width: Motion,
    height: Motion,
    visibility: Motion,
    surface_width: f32,
    surface_height: f32,
    last_dpi: f32,
    accent: (u8, u8, u8),
}

impl OverlayView {
    fn new(state: Snapshot, window: &Window) -> Self {
        Self {
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
            surface_width: 48.,
            surface_height: 48.,
            last_dpi: window.scale_factor(),
            accent: crate::WindowsAccent::load().accent,
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
            self.accent = crate::WindowsAccent::load().accent;
        }
        self.visibility
            .retarget(if next.visible { 1. } else { 0. }, now);
        self.state = next;
    }

    fn content(
        &self,
        state: &Snapshot,
        scale: f32,
        height: f32,
        radius: f32,
        opacity: f32,
        offset: f32,
    ) -> gpui::Div {
        let settings = &state.settings;
        let has_icon = has_icon(settings);
        let has_text = has_text(settings);
        let icon_size = 24. * scale;
        let icon_only = has_icon && !has_text;
        let padding = if has_text {
            if has_icon { 8. * scale } else { 14. * scale }
        } else {
            0.
        };
        let foreground = if settings.background_style == "Light" {
            0x202631
        } else {
            0xf1f4f8
        };
        let accent = icon_color(state, self.accent);
        let path = if settings.icon_pair == crate::MUTE_FAILURE_ICON_PAIR {
            "warning".into()
        } else {
            format!(
                "{}/{}",
                settings.icon_pair,
                if state.muted { "muted" } else { "live" }
            )
        };
        div()
            .absolute()
            .left(px(padding))
            .when(icon_only, |row| row.right_0())
            .top(px(offset))
            .h(px(height))
            .flex()
            .items_center()
            .gap(px(10. * scale))
            .opacity(opacity * settings.content_opacity.clamp(20, 100) as f32 / 100.)
            .when(has_icon, |row| {
                row.child(
                    div()
                        .when(icon_only, |icon| icon.size_full().rounded(px(radius)))
                        .when(!icon_only, |icon| {
                            icon.size(px(32. * scale)).rounded(px(10. * scale))
                        })
                        .flex_shrink_0()
                        .bg(color_alpha(accent, 0.10))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(svg().path(path).size(px(icon_size)).text_color(rgb(accent))),
                )
            })
            .when(has_text, |row| {
                row.child(
                    div()
                        .whitespace_nowrap()
                        .text_size(px(14. * scale))
                        .line_height(px(20. * scale))
                        .font_family(font_family(settings))
                        .font_weight(FontWeight(settings.text_font_weight.clamp(100, 900) as f32))
                        .text_color(rgb(foreground))
                        .child(label(state)),
                )
            })
    }
}

impl Render for OverlayView {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let dpi = window.scale_factor();
        // Respect selected-monitor DPI even before WM_DPICHANGED reaches the new window.
        let scale = native_overlay::scale() / dpi;
        if self.last_dpi != dpi {
            self.last_dpi = dpi;
            self.needs_measure = true;
        }
        if self.needs_measure {
            let target_width = measure_width(&self.state, scale, window);
            let target_height = card_height(&self.state.settings) * scale;
            if self.measured {
                self.width.retarget(target_width, now);
                self.height.retarget(target_height, now);
            } else {
                self.width = Motion::new(target_width, 260);
                self.height = Motion::new(target_height, 260);
                self.measured = true;
            }
            let mut alternate = self.state.clone();
            alternate.muted = !alternate.muted;
            self.surface_width = target_width
                .max(measure_width(&alternate, scale, window))
                .max(self.width.from);
            self.surface_height = target_height.max(self.height.from);
            self.needs_measure = false;
        }

        let width = self.width.value(now);
        let height = self.height.value(now);
        let alpha = self.visibility.value(now);
        self.layers
            .retain(|layer| layer.opacity.to > 0. || layer.opacity.active(now));
        let content_animating = self.layers.iter().any(|layer| layer.opacity.active(now));
        if self.width.active(now)
            || self.height.active(now)
            || self.visibility.active(now)
            || content_animating
        {
            window.request_animation_frame();
        }
        // A stable GPU surface avoids resizing the swapchain on every animation frame.
        let gutter = 14. * scale;
        let left = gutter
            + (self.surface_width - width) * self.state.settings.position_x.clamp(0., 100.) as f32
                / 100.;
        let top = gutter
            + (self.surface_height - height)
                * self.state.settings.position_y.clamp(0., 100.) as f32
                / 100.;
        native_overlay::set_geometry(
            (width * dpi).round() as i32,
            (height * dpi).round() as i32,
            ((self.surface_width + gutter * 2.) * dpi).ceil() as i32,
            ((self.surface_height + gutter * 2.) * dpi).ceil() as i32,
            (left * dpi).round() as i32,
            (top * dpi).round() as i32,
        );
        let should_present = self.state.visible || alpha > 0.001;
        native_overlay::present(should_present);

        let settings = &self.state.settings;
        let dot = settings.variant == "Dot";
        // Blend the actual on-screen layers, so quick toggles cannot flash a full old state.
        let mut bg = gpui::Rgba::default();
        for layer in &self.layers {
            let color = background_color(&layer.state);
            let weight = layer.opacity.value(now);
            bg.r += color.r * weight;
            bg.g += color.g * weight;
            bg.b += color.b * weight;
            bg.a += color.a * weight;
        }
        let radius = (settings.border_radius.min(24) as f32 * scale).min(height / 2.);
        let border = if self.state.positioning {
            color_alpha(0x78a8ff, 0.9)
        } else if settings.background_style == "Light" {
            color_alpha(0x25354b, 0.16)
        } else {
            color_alpha(0xe7efff, 0.18)
        };
        let mut card = div()
            .absolute()
            .left(px(left))
            .top(px(top + (1. - alpha) * 6. * scale))
            .w(px(width))
            .h(px(height))
            .rounded(px(radius))
            .overflow_hidden()
            .bg(bg)
            .opacity(alpha)
            .when(settings.background_opacity > 0 || dot, |card| {
                card.shadow_md()
            })
            .when(settings.show_border || self.state.positioning, |card| {
                card.border_1().border_color(border)
            });
        let mut contents = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .rounded(px(radius));
        for layer in &self.layers {
            if layer.state.settings.variant != "Dot" {
                contents = contents.child(self.content(
                    &layer.state,
                    scale,
                    height,
                    radius,
                    layer.opacity.value(now),
                    0.,
                ));
            }
        }
        card = card.child(contents);
        div().relative().size_full().child(card)
    }
}

fn has_icon(settings: &OverlayConfig) -> bool {
    matches!(settings.variant.as_str(), "MicIcon" | "IconText")
}
fn has_text(settings: &OverlayConfig) -> bool {
    matches!(settings.variant.as_str(), "IconText" | "Text")
        || (settings.variant == "MicIcon" && settings.show_text)
}
fn card_height(settings: &OverlayConfig) -> f32 {
    if settings.variant == "Dot" { 24. } else { 48. }
}
fn font_family(settings: &OverlayConfig) -> SharedString {
    if settings.text_font.trim().is_empty() {
        "Segoe UI".into()
    } else {
        settings.text_font.clone().into()
    }
}
fn label(state: &Snapshot) -> SharedString {
    let text = if state.muted {
        &state.settings.muted_label
    } else {
        &state.settings.unmuted_label
    };
    // GPUI shape_line requires one line; custom labels may contain pasted line breaks.
    text.replace(['\r', '\n'], " ").into()
}
fn measure_width(state: &Snapshot, scale: f32, window: &mut Window) -> f32 {
    let settings = &state.settings;
    let height = card_height(settings) * scale;
    if !has_text(settings) {
        return height;
    }
    let text = label(state);
    let mut text_font = font(font_family(settings));
    text_font.weight = FontWeight(settings.text_font_weight.clamp(100, 900) as f32);
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
        .shape_line(text, px(14. * scale), &[run], None);
    // Icon + text: 8px left matches the icon's vertical inset; 14px right.
    (if has_icon(settings) {
        64. * scale
    } else {
        28. * scale
    } + f32::from(line.width))
    .max(height)
}
fn icon_color(state: &Snapshot, system: (u8, u8, u8)) -> u32 {
    if state.settings.icon_pair == crate::MUTE_FAILURE_ICON_PAIR {
        return 0xffb454;
    }
    match state.settings.icon_style.as_str() {
        "Monochrome" => {
            if state.settings.background_style == "Light" {
                0x202631
            } else {
                0xf1f4f8
            }
        }
        "SystemColor" => (system.0 as u32) << 16 | (system.1 as u32) << 8 | system.2 as u32,
        _ => mic_state_color(state.muted),
    }
}
fn mic_state_color(muted: bool) -> u32 {
    if muted { 0xf07987 } else { 0x6ed6b1 }
}
fn content_changed(old: &Snapshot, new: &Snapshot) -> bool {
    let a = &old.settings;
    let b = &new.settings;
    old.muted != new.muted
        || a.variant != b.variant
        || a.show_text != b.show_text
        || a.icon_pair != b.icon_pair
        || a.icon_style != b.icon_style
        || a.muted_label != b.muted_label
        || a.unmuted_label != b.unmuted_label
        || a.text_font != b.text_font
        || a.text_font_weight != b.text_font_weight
}
fn background_color(state: &Snapshot) -> gpui::Rgba {
    let settings = &state.settings;
    if settings.variant == "Dot" {
        color_alpha(
            mic_state_color(state.muted),
            settings.content_opacity.clamp(20, 100) as f32 / 100.,
        )
    } else {
        color_alpha(
            if settings.background_style == "Light" {
                0xf7f9fc
            } else {
                0x202020
            },
            settings.background_opacity.min(100) as f32 / 100.,
        )
    }
}
fn color_alpha(color: u32, alpha: f32) -> gpui::Rgba {
    rgba((color << 8) | (alpha.clamp(0., 1.) * 255.).round() as u32)
}
