//! Overlay themes. "Custom" maps the user's appearance controls; every other
//! theme is a fixed look that only borrows placement, scale and visibility.
use super::Snapshot;
use crate::OverlayConfig;
use gpui::{Rgba, SharedString, rgba};
use std::sync::OnceLock;

pub(crate) const CUSTOM: &str = "Custom";

/// Theme ids and labels in picker order; Custom always comes first.
pub(crate) const THEMES: &[(&str, &str)] = &[
    (CUSTOM, "Custom"),
    ("Windows", "Windows"),
    ("MaterialYou", "Material You"),
    ("Cute", "Cute"),
    ("Neon", "Neon"),
    ("Brutalism", "Brutalism"),
];

/// Windows-provided colors that the native-looking themes follow.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct System {
    pub accent: (u8, u8, u8),
    pub light: bool,
}

impl System {
    pub fn load() -> Self {
        Self {
            accent: crate::WindowsAccent::load().accent,
            light: crate::windows_uses_light_system_theme(),
        }
    }
    fn accent(self) -> u32 {
        let (r, g, b) = self.accent;
        (r as u32) << 16 | (g as u32) << 8 | b as u32
    }
}

pub(super) enum Shadow {
    /// The original subtle drop shadow, clipped to the surface.
    Soft,
    /// Blurred drop shadow: (color, y offset, blur).
    Drop(Rgba, f32, f32),
    /// Solid, unblurred offset copy of the card: (color, x, y).
    Hard(Rgba, f32, f32),
    /// Colored halo around the card: (color, blur).
    Halo(Rgba, f32),
}

pub(super) struct IconBox {
    pub size: f32,
    /// Logical radius; `None` keeps the box concentric with the card.
    pub radius: Option<f32>,
    pub fill: Rgba,
    pub border: Option<(Rgba, f32)>,
}

/// Everything the renderer needs, in logical pixels before overlay scale.
pub(super) struct Look {
    pub height: f32,
    pub radius: f32,
    pub pad_icon: f32,
    pub pad: f32,
    pub gap: f32,
    pub icon_size: f32,
    pub icon_path: SharedString,
    pub icon: u32,
    pub icon_box: Option<IconBox>,
    pub surface: Rgba,
    pub foreground: u32,
    pub border: Option<(Rgba, f32)>,
    pub sheen: f32,
    pub glow: f32,
    pub shadow: Shadow,
    /// Transparent margin around the card so shadows are not clipped.
    pub gutter: f32,
    /// Fill, corners and shadow come from the HWND (DWM acrylic), not a painted card.
    pub acrylic: bool,
    pub font: SharedString,
    pub weight: u16,
    pub text_size: f32,
    pub label: SharedString,
    pub content_opacity: f32,
    pub has_icon: bool,
    pub has_text: bool,
    pub dot: bool,
}

impl Look {
    pub fn resolve(state: &Snapshot, system: System) -> Self {
        let settings = &state.settings;
        // The mute-failure warning always keeps its own forced look.
        if settings.icon_pair == crate::MUTE_FAILURE_ICON_PAIR {
            return custom(state, system);
        }
        match settings.theme.as_str() {
            "Windows" => windows(state, system),
            "MaterialYou" => material_you(state, system),
            "Cute" => cute(state),
            "Neon" => neon(state),
            "Brutalism" => brutalism(state),
            _ => custom(state, system),
        }
    }
}

fn custom(state: &Snapshot, system: System) -> Look {
    let settings = &state.settings;
    let light = settings.background_style == "Light";
    let has_icon = matches!(settings.variant.as_str(), "MicIcon" | "IconText");
    let has_text = matches!(settings.variant.as_str(), "IconText" | "Text")
        || (settings.variant == "MicIcon" && settings.show_text);
    let dot = settings.variant == "Dot";
    let icon = custom_icon_color(state, system);
    let foreground = if light { 0x202631 } else { 0xf1f4f8 };
    let surface = settings.background_opacity.min(100) as f32 / 100.;
    let surface = if dot {
        color_alpha(
            mic_state_color(state.muted),
            settings.content_opacity.clamp(20, 100) as f32 / 100.,
        )
    } else {
        color_alpha(if light { 0xf7f9fc } else { 0x1b1c21 }, surface)
    };
    Look {
        height: if dot { 24. } else { 48. },
        radius: settings.border_radius.min(24) as f32,
        pad_icon: 8.,
        pad: 14.,
        gap: 10.,
        icon_size: 24.,
        icon_path: icon_path(state, &settings.icon_pair),
        icon,
        icon_box: Some(IconBox {
            size: 32.,
            radius: None,
            fill: color_alpha(icon, 0.14),
            border: Some((color_alpha(icon, 0.22), 1.)),
        }),
        surface,
        foreground,
        border: settings.show_border.then(|| {
            (
                if light {
                    color_alpha(0x0f172a, 0.12)
                } else {
                    color_alpha(0xffffff, 0.10)
                },
                1.,
            )
        }),
        sheen: if dot { 0. } else if light { 0.55 } else { 0.06 },
        glow: if light { 0.10 } else { 0.16 },
        shadow: Shadow::Soft,
        gutter: 0.,
        acrylic: false,
        font: if settings.text_font.trim().is_empty() {
            "Segoe UI".into()
        } else {
            settings.text_font.clone().into()
        },
        weight: settings.text_font_weight.clamp(100, 900),
        text_size: 14.,
        label: user_label(settings, state.muted),
        content_opacity: settings.content_opacity.clamp(20, 100) as f32 / 100.,
        has_icon,
        has_text,
        dot,
    }
}

/// Mirrors the Windows 11 volume/brightness flyout: the HWND itself is the
/// surface (DWM acrylic, rounding and shadow); GPUI only paints a tint over it.
fn windows(state: &Snapshot, system: System) -> Look {
    // DWM's transient acrylic is lighter than the shell flyout's. Pure black
    // darkens it without graying out the wallpaper hue that shows through,
    // which is what makes the shell flyout read as "deep".
    let (tint, foreground) = if system.light {
        (color_alpha(0xfcfcfc, 0.5), 0x1b1b1b)
    } else {
        (color_alpha(0x000000, 0.45), 0xffffff)
    };
    Look {
        height: 48.,
        radius: 8.,
        pad_icon: 15.,
        pad: 18.,
        gap: 14.,
        icon_size: 18.,
        icon_path: icon_path(state, "fluent"),
        // The flyout reserves the accent for its active control.
        icon: if state.muted { foreground } else { system.accent() },
        icon_box: None,
        surface: tint,
        foreground,
        border: None,
        sheen: 0.,
        glow: 0.,
        shadow: Shadow::Soft,
        gutter: 0.,
        acrylic: true,
        font: pick_font(&["Segoe UI Variable Text", "Segoe UI"]),
        weight: 400,
        text_size: 14.,
        label: label(state, "Microphone muted", "Microphone on"),
        content_opacity: 1.,
        has_icon: true,
        has_text: true,
        dot: false,
    }
}

/// Material 3 tonal palette seeded by the Windows accent (the wallpaper color
/// when Windows picks it automatically), just like dynamic color on Android.
fn material_you(state: &Snapshot, system: System) -> Look {
    let (hue, saturation, _) = hsl(system.accent());
    let tone = |s: f32, l: f32| from_hsl(hue, s.min(saturation.max(0.18)), l);
    let (surface, foreground, container, on_container) = match (system.light, state.muted) {
        (false, false) => (tone(0.12, 0.13), tone(0.10, 0.90), tone(0.45, 0.30), tone(0.80, 0.90)),
        (false, true) => (tone(0.12, 0.13), tone(0.10, 0.90), 0x8c1d18, 0xf9dedc),
        (true, false) => (tone(0.40, 0.95), tone(0.10, 0.12), tone(0.80, 0.88), tone(0.60, 0.18)),
        (true, true) => (tone(0.40, 0.95), tone(0.10, 0.12), 0xf9dedc, 0x410e0b),
    };
    Look {
        height: 56.,
        radius: 28.,
        pad_icon: 8.,
        pad: 24.,
        gap: 12.,
        icon_size: 22.,
        icon_path: icon_path(state, "material"),
        icon: on_container,
        icon_box: Some(IconBox {
            size: 40.,
            radius: Some(20.),
            fill: color_alpha(container, 1.),
            border: None,
        }),
        surface: color_alpha(surface, 0.98),
        foreground,
        border: None,
        sheen: 0.,
        glow: 0.,
        shadow: Shadow::Drop(color_alpha(0x000000, 0.30), 2., 8.),
        gutter: 10.,
        acrylic: false,
        font: pick_font(&["Google Sans", "Roboto", "Segoe UI Variable Text", "Segoe UI"]),
        weight: 500,
        text_size: 15.,
        label: label(state, "Microphone muted", "Microphone on"),
        content_opacity: 1.,
        has_icon: true,
        has_text: true,
        dot: false,
    }
}

/// Pastel sticker: pill shape, chunky outline and a solid drop.
fn cute(state: &Snapshot) -> Look {
    let (bubble, icon) = if state.muted {
        (0xffd3e5, 0xff4f93)
    } else {
        (0xc8f4e3, 0x1fb383)
    };
    Look {
        height: 50.,
        radius: 25.,
        pad_icon: 7.,
        pad: 20.,
        gap: 10.,
        icon_size: 20.,
        icon_path: icon_path(state, "mingcute-fill"),
        icon,
        icon_box: Some(IconBox {
            size: 36.,
            radius: Some(18.),
            fill: color_alpha(bubble, 1.),
            border: None,
        }),
        surface: color_alpha(0xfff2f8, 1.),
        foreground: 0x9c3b6c,
        border: Some((color_alpha(0xffb3d3, 1.), 2.)),
        sheen: 0.,
        glow: 0.,
        shadow: Shadow::Hard(color_alpha(0xffb3d3, 1.), 0., 4.),
        gutter: 5.,
        acrylic: false,
        font: pick_font(&["Nunito", "Quicksand", "Varela Round", "Comic Sans MS"]),
        weight: 700,
        text_size: 14.,
        label: label(state, "Shh\u{2026} muted", "Mic is on!"),
        content_opacity: 1.,
        has_icon: true,
        has_text: true,
        dot: false,
    }
}

/// Glowing tube sign: hot pink while muted, cyan on air.
fn neon(state: &Snapshot) -> Look {
    let tube = if state.muted { 0xff2e88 } else { 0x19f0ff };
    Look {
        height: 46.,
        radius: 12.,
        pad_icon: 14.,
        pad: 18.,
        gap: 10.,
        icon_size: 22.,
        icon_path: icon_path(state, "lucide"),
        icon: tube,
        icon_box: None,
        surface: color_alpha(0x0b0614, 0.94),
        foreground: tube,
        border: Some((color_alpha(tube, 1.), 1.5)),
        sheen: 0.,
        glow: 0.22,
        // GPUI draws the gaussian tail 3× blur outside the card, so the HWND
        // has to be that much larger or the bloom gets shaved off.
        shadow: Shadow::Halo(color_alpha(tube, 0.70), 18.),
        gutter: 58.,
        acrylic: false,
        font: pick_font(&["Orbitron", "Bahnschrift", "Segoe UI"]),
        weight: 600,
        text_size: 14.,
        label: label(state, "MIC MUTED", "ON AIR"),
        content_opacity: 1.,
        has_icon: true,
        has_text: true,
        dot: false,
    }
}

/// Raw slab: flat signal color, thick black rule, hard offset shadow.
fn brutalism(state: &Snapshot) -> Look {
    let slab = if state.muted { 0xff5a5f } else { 0xc6ff3d };
    Look {
        height: 50.,
        radius: 0.,
        pad_icon: 8.,
        pad: 16.,
        gap: 12.,
        icon_size: 22.,
        icon_path: icon_path(state, "mdi"),
        icon: slab,
        icon_box: Some(IconBox {
            size: 34.,
            radius: Some(0.),
            fill: color_alpha(0x000000, 1.),
            border: None,
        }),
        surface: color_alpha(slab, 1.),
        foreground: 0x000000,
        border: Some((color_alpha(0x000000, 1.), 3.)),
        sheen: 0.,
        glow: 0.,
        shadow: Shadow::Hard(color_alpha(0x000000, 1.), 5., 5.),
        gutter: 6.,
        acrylic: false,
        font: pick_font(&["Arial Black", "Impact", "Segoe UI"]),
        weight: 900,
        text_size: 15.,
        label: label(state, "MUTED", "LIVE"),
        content_opacity: 1.,
        has_icon: true,
        has_text: true,
        dot: false,
    }
}

fn icon_path(state: &Snapshot, pair: &str) -> SharedString {
    if state.settings.icon_pair == crate::MUTE_FAILURE_ICON_PAIR {
        return "warning".into();
    }
    format!("{pair}/{}", if state.muted { "muted" } else { "live" }).into()
}

fn custom_icon_color(state: &Snapshot, system: System) -> u32 {
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
        "SystemColor" => system.accent(),
        _ => mic_state_color(state.muted),
    }
}

fn mic_state_color(muted: bool) -> u32 {
    // Match the settings UI's --danger / --success palette.
    if muted { 0xef4444 } else { 0x10b981 }
}

fn user_label(settings: &OverlayConfig, muted: bool) -> SharedString {
    let text = if muted {
        &settings.muted_label
    } else {
        &settings.unmuted_label
    };
    // GPUI shape_line requires one line; custom labels may contain pasted line breaks.
    text.replace(['\r', '\n'], " ").into()
}

fn label(state: &Snapshot, muted: &'static str, live: &'static str) -> SharedString {
    if state.settings.icon_pair == crate::MUTE_FAILURE_ICON_PAIR {
        return user_label(&state.settings, state.muted);
    }
    if state.muted { muted } else { live }.into()
}

/// First installed family, so themes degrade gracefully on bare systems.
fn pick_font(candidates: &[&'static str]) -> SharedString {
    static INSTALLED: OnceLock<Vec<String>> = OnceLock::new();
    let installed = INSTALLED.get_or_init(|| {
        crate::system_fonts()
            .into_iter()
            .map(|font| font.family.to_ascii_lowercase())
            .collect()
    });
    candidates
        .iter()
        .find(|family| installed.contains(&family.to_ascii_lowercase()))
        .or(candidates.last())
        .copied()
        .unwrap_or("Segoe UI")
        .into()
}

pub(super) fn color_alpha(color: u32, alpha: f32) -> Rgba {
    rgba((color << 8) | (alpha.clamp(0., 1.) * 255.).round() as u32)
}

fn hsl(color: u32) -> (f32, f32, f32) {
    let r = ((color >> 16) & 0xff) as f32 / 255.;
    let g = ((color >> 8) & 0xff) as f32 / 255.;
    let b = (color & 0xff) as f32 / 255.;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.;
    let d = max - min;
    if d == 0. {
        return (0., 0., l);
    }
    let s = d / (1. - (2. * l - 1.).abs());
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.)
    } else if max == g {
        (b - r) / d + 2.
    } else {
        (r - g) / d + 4.
    } * 60.;
    (h, s, l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> u32 {
    let c = (1. - (2. * l - 1.).abs()) * s;
    let x = c * (1. - ((h / 60.).rem_euclid(2.) - 1.).abs());
    let m = l - c / 2.;
    let (r, g, b) = match (h / 60.) as u32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    let channel = |v: f32| ((v + m).clamp(0., 1.) * 255.).round() as u32;
    channel(r) << 16 | channel(g) << 8 | channel(b)
}
