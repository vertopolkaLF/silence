//! Overlay themes. "Custom" maps the user's appearance controls; every other
//! theme supplies its look while sharing labels, content mode and placement.
use super::Snapshot;
use crate::OverlayConfig;
use gpui::{Rgba, SharedString, rgba};

pub(crate) const CUSTOM: &str = "Custom";

/// Theme ids and labels in picker order; Custom always comes first.
pub(crate) const THEMES: &[(&str, &str)] = &[
    (CUSTOM, "Custom"),
    ("Windows", "Windows"),
    ("MaterialYou", "Material You"),
    ("Cute", "Cute"),
    ("CuteSticker", "Cute Sticker"),
    ("Neon", "Neon"),
    ("Brutalism", "Brutalism"),
    ("Terminal", "Terminal"),
    ("Blueprint", "Blueprint"),
    ("Cassette", "Cassette"),
    ("Arcade", "Arcade"),
    ("Paper", "Paper"),
    ("Frosted", "Frosted"),
    ("Porcelain", "Porcelain"),
    ("Radar", "Radar"),
];

/// Applied on each explicit theme selection; later icon edits remain independent.
pub(crate) fn default_icon_pair(theme: &str) -> &'static str {
    match theme {
        "Windows" | CUSTOM => "fluent",
        "MaterialYou" => "material",
        "Cute" => "mingcute-fill",
        "CuteSticker" | "Neon" => "lucide",
        "Brutalism" | "Arcade" => "mdi",
        "Cassette" => "phosphor",
        "Paper" => "tabler",
        _ => "solar",
    }
}

pub(crate) fn supports_corner_radius(theme: &str) -> bool {
    !matches!(theme, "Windows" | "CuteSticker")
}

/// UI fallback matches the preset geometry; it does not alter untouched themes.
pub(crate) fn corner_radius(settings: &OverlayConfig) -> u8 {
    if let Some(radius) = settings.corner_radii.get(&settings.theme) {
        return (*radius).min(32);
    }
    match settings.theme.as_str() {
        "MaterialYou" => 28,
        "Cute" => 25,
        "Neon" => 12,
        "Brutalism" | "Blueprint" | "Arcade" => 0,
        "Terminal" => 3,
        "Cassette" => 9,
        "Paper" => 2,
        "Frosted" => 18,
        "Porcelain" => 30,
        "Radar" => 8,
        _ => settings.border_radius.min(24),
    }
}

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

#[derive(Clone, Copy)]
pub(super) enum Detail {
    None,
    Terminal,
    Blueprint,
    Cassette,
    Arcade,
    Paper,
    Frosted,
    Porcelain,
    Radar,
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
    pub detail: Detail,
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
        let warning = settings.icon_pair == crate::MUTE_FAILURE_ICON_PAIR;
        let mut look = match settings.theme.as_str() {
            "Windows" => windows(state, system),
            "MaterialYou" => material_you(state, system),
            "Cute" => cute(state),
            "CuteSticker" if warning => cute(state),
            "CuteSticker" => cute_sticker(state),
            "Neon" => neon(state),
            "Brutalism" => brutalism(state),
            "Terminal" => terminal(state),
            "Blueprint" => blueprint(state),
            "Cassette" => cassette(state),
            "Arcade" => arcade(state),
            "Paper" => paper(state),
            "Frosted" => frosted(state),
            "Porcelain" => porcelain(state),
            "Radar" => radar(state),
            _ => custom(state, system),
        };
        if supports_corner_radius(&settings.theme) {
            if let Some(radius) = settings.corner_radii.get(&settings.theme) {
                look.radius = f32::from((*radius).min(32));
            }
        }
        // Content is a persistent user preference, independent of the theme.
        if settings.theme != CUSTOM && settings.variant != "Dot" {
            look.has_icon = matches!(settings.variant.as_str(), "MicIcon" | "IconText");
            look.has_text = matches!(settings.variant.as_str(), "IconText" | "Text")
                || (settings.variant == "MicIcon" && settings.show_text);
        }
        if warning {
            // Keep theme chrome and typography, but never hide or mislabel a failure.
            look.icon_path = "warning".into();
            look.label = user_label(settings, state.muted);
            look.has_icon = true;
            look.has_text = true;
            look.dot = false;
            // Use the themed foreground for reliable contrast on every surface.
            look.icon = look.foreground;
            if let Some(icon_box) = &mut look.icon_box {
                icon_box.fill = color_alpha(look.foreground, 0.10);
                icon_box.border = None;
            }
        }
        look
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
        detail: Detail::None,
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
        sheen: if dot {
            0.
        } else if light {
            0.55
        } else {
            0.06
        },
        glow: if light { 0.10 } else { 0.16 },
        shadow: Shadow::Soft,
        gutter: 0.,
        acrylic: false,
        font: if settings.text_font.trim().is_empty() {
            super::fonts::DEFAULT.into()
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
        detail: Detail::None,
        height: 48.,
        radius: 8.,
        pad_icon: 15.,
        pad: 18.,
        gap: 14.,
        icon_size: 18.,
        icon_path: icon_path(state, &state.settings.icon_pair),
        // The flyout reserves the accent for its active control.
        icon: if state.muted {
            foreground
        } else {
            system.accent()
        },
        icon_box: None,
        surface: tint,
        foreground,
        border: None,
        sheen: 0.,
        glow: 0.,
        shadow: Shadow::Soft,
        gutter: 0.,
        acrylic: true,
        font: super::fonts::DEFAULT.into(),
        weight: 400,
        text_size: 14.,
        label: user_label(&state.settings, state.muted),
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
        (false, false) => (
            tone(0.12, 0.13),
            tone(0.10, 0.90),
            tone(0.45, 0.30),
            tone(0.80, 0.90),
        ),
        (false, true) => (tone(0.12, 0.13), tone(0.10, 0.90), 0x8c1d18, 0xf9dedc),
        (true, false) => (
            tone(0.40, 0.95),
            tone(0.10, 0.12),
            tone(0.80, 0.88),
            tone(0.60, 0.18),
        ),
        (true, true) => (tone(0.40, 0.95), tone(0.10, 0.12), 0xf9dedc, 0x410e0b),
    };
    Look {
        detail: Detail::None,
        height: 56.,
        radius: 28.,
        pad_icon: 8.,
        pad: 24.,
        gap: 12.,
        icon_size: 22.,
        icon_path: icon_path(state, &state.settings.icon_pair),
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
        font: "Google Sans".into(),
        weight: 500,
        text_size: 15.,
        label: user_label(&state.settings, state.muted),
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
        detail: Detail::None,
        height: 50.,
        radius: 25.,
        pad_icon: 7.,
        pad: 20.,
        gap: 10.,
        icon_size: 20.,
        icon_path: icon_path(state, &state.settings.icon_pair),
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
        font: "Nunito".into(),
        weight: 700,
        text_size: 14.,
        label: user_label(&state.settings, state.muted),
        content_opacity: 1.,
        has_icon: true,
        has_text: true,
        dot: false,
    }
}

/// Die-cut artwork has its own stacked composition rather than a card surface.
fn cute_sticker(state: &Snapshot) -> Look {
    let mut look = cute(state);
    look.height = 146.;
    look.icon_size = 88.;
    look.icon_path = format!(
        "cute-sticker/{}/{}",
        crate::overlay_icons::overlay_icon_pair(&state.settings.icon_pair).id,
        if state.muted { "muted" } else { "live" },
    )
    .into();
    look.surface = color_alpha(0xffffff, 0.);
    look.border = None;
    look.gutter = 44.;
    look.text_size = 26.;
    look.weight = 900;
    look.icon = if state.muted { 0xf49ac2 } else { 0x73cbb1 };
    look.foreground = look.icon;
    // Theme copy follows the reference; explicitly customized labels still win.
    if state.muted && state.settings.muted_label == "Microphone muted" {
        look.label = "silence!".into();
    } else if !state.muted && state.settings.unmuted_label == "Microphone on" {
        look.label = "on air!".into();
    }
    look
}

/// Glowing tube sign: hot pink while muted, cyan on air.
fn neon(state: &Snapshot) -> Look {
    let tube = if state.muted { 0xff2e88 } else { 0x19f0ff };
    Look {
        detail: Detail::None,
        height: 46.,
        radius: 12.,
        pad_icon: 14.,
        pad: 18.,
        gap: 10.,
        icon_size: 22.,
        icon_path: icon_path(state, &state.settings.icon_pair),
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
        font: "Orbitron".into(),
        weight: 600,
        text_size: 14.,
        label: user_label(&state.settings, state.muted),
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
        detail: Detail::None,
        height: 50.,
        radius: 0.,
        pad_icon: 8.,
        pad: 16.,
        gap: 12.,
        icon_size: 22.,
        icon_path: icon_path(state, &state.settings.icon_pair),
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
        font: "Archivo Black".into(),
        weight: 400, // Archivo Black's regular face already has black-weight outlines.
        text_size: 15.,
        label: user_label(&state.settings, state.muted),
        content_opacity: 1.,
        has_icon: true,
        has_text: true,
        dot: false,
    }
}

/// Presets share content semantics, but never inherit Custom appearance controls.
fn preset(state: &Snapshot, surface: u32, foreground: u32, signal: u32) -> Look {
    let mut look = cute(state);
    look.height = 48.;
    look.radius = 10.;
    look.pad_icon = 12.;
    look.pad = 20.;
    look.gap = 12.;
    look.icon_size = 22.;
    look.icon_path = icon_path(state, &state.settings.icon_pair);
    look.icon = signal;
    look.icon_box = None;
    look.surface = color_alpha(surface, 1.);
    look.foreground = foreground;
    look.border = None;
    look.shadow = Shadow::Drop(color_alpha(0x000000, 0.24), 3., 6.);
    look.gutter = 22.;
    look.font = super::fonts::DEFAULT.into();
    look.weight = 600;
    look.text_size = 14.;
    look
}

fn terminal(state: &Snapshot) -> Look {
    let signal = if state.muted { 0xffbd69 } else { 0x7cf7b4 };
    let mut look = preset(state, 0x0d1914, signal, signal);
    look.detail = Detail::Terminal;
    look.height = 42.;
    look.radius = 3.;
    look.font = "Orbitron".into();
    look.text_size = 12.;
    look.pad = 26.;
    look.border = Some((color_alpha(signal, 0.35), 1.));
    look.shadow = Shadow::Hard(color_alpha(0x06100b, 1.), 3., 3.);
    look.gutter = 5.;
    look
}

fn blueprint(state: &Snapshot) -> Look {
    let signal = if state.muted { 0xffcf9c } else { 0xa7e5ff };
    let mut look = preset(state, 0x153c72, 0xebf5ff, signal);
    look.detail = Detail::Blueprint;
    look.height = 50.;
    look.radius = 0.;
    look.font = "Orbitron".into();
    look.text_size = 12.;
    look.pad_icon = 16.;
    look.border = Some((color_alpha(0xa7d6ff, 0.65), 1.));
    look.shadow = Shadow::Hard(color_alpha(0x082244, 0.9), 4., 4.);
    look.gutter = 6.;
    look
}

fn cassette(state: &Snapshot) -> Look {
    let signal = if state.muted { 0xa6463d } else { 0x2c726e };
    let mut look = preset(state, 0xe9dcc1, 0x39352e, signal);
    look.detail = Detail::Cassette;
    look.height = 58.;
    look.radius = 9.;
    look.pad_icon = 16.;
    look.pad = 24.;
    look.icon_path = icon_path(state, &state.settings.icon_pair);
    look.icon_box = None;
    look.border = Some((color_alpha(0x75664e, 0.8), 1.));
    look.font = "Google Sans".into();
    look
}

fn arcade(state: &Snapshot) -> Look {
    let signal = if state.muted { 0xff7fa9 } else { 0x8fe8ff };
    let mut look = preset(state, 0x30204d, 0xffefab, signal);
    look.detail = Detail::Arcade;
    look.height = 46.;
    look.radius = 0.;
    look.font = "Orbitron".into();
    look.weight = 800;
    look.text_size = 13.;
    look.icon_path = icon_path(state, &state.settings.icon_pair);
    look.pad_icon = 14.;
    look.border = Some((color_alpha(0xb997ed, 1.), 2.));
    look.shadow = Shadow::Hard(color_alpha(0x170e2c, 1.), 5., 5.);
    look.gutter = 7.;
    look
}

fn paper(state: &Snapshot) -> Look {
    let signal = if state.muted { 0xa23748 } else { 0x28634f };
    let mut look = preset(state, 0xfffdf5, 0x343b48, signal);
    look.detail = Detail::Paper;
    look.height = 54.;
    look.radius = 2.;
    look.pad_icon = 20.;
    look.font = "Nunito".into();
    look.weight = 700;
    look.icon_path = icon_path(state, &state.settings.icon_pair);
    look.shadow = Shadow::Hard(color_alpha(0xd6d0be, 1.), 2., 3.);
    look.gutter = 5.;
    look
}

fn frosted(state: &Snapshot) -> Look {
    let signal = if state.muted { 0x9a476d } else { 0x245c8a };
    let mut look = preset(state, 0xdcecf7, 0x253e58, signal);
    look.detail = Detail::Frosted;
    look.height = 56.;
    look.radius = 18.;
    look.surface = color_alpha(0xdcecf7, 0.94);
    look.sheen = 0.8;
    look.icon_box = Some(IconBox {
        size: 34.,
        radius: Some(10.),
        fill: color_alpha(0xffffff, 0.6),
        border: Some((color_alpha(0xffffff, 0.9), 1.)),
    });
    look.border = Some((color_alpha(0xffffff, 0.9), 1.));
    look.font = "Google Sans".into();
    look.weight = 500;
    look.shadow = Shadow::Drop(color_alpha(0x4b779d, 0.28), 4., 10.);
    look.gutter = 34.;
    look
}

fn porcelain(state: &Snapshot) -> Look {
    let signal = if state.muted { 0x9d3b43 } else { 0x315cad };
    let mut look = preset(state, 0xf5f6f9, 0x283d69, signal);
    look.detail = Detail::Porcelain;
    look.height = 60.;
    look.radius = 30.;
    look.pad_icon = 10.;
    look.pad = 26.;
    look.gap = 14.;
    look.icon_box = Some(IconBox {
        size: 40.,
        radius: Some(20.),
        fill: color_alpha(0xe5eaf5, 1.),
        border: Some((color_alpha(0x8ca1c9, 0.6), 1.)),
    });
    look.font = "Nunito".into();
    look.weight = 800;
    look.border = Some((color_alpha(0x8ca1c9, 0.7), 1.));
    look.shadow = Shadow::Drop(color_alpha(0x253c65, 0.20), 4., 8.);
    look.gutter = 28.;
    look
}

fn radar(state: &Snapshot) -> Look {
    let signal = if state.muted { 0xefb775 } else { 0x98dba0 };
    let mut look = preset(state, 0x182d29, 0xdae9d9, signal);
    look.detail = Detail::Radar;
    look.height = 54.;
    look.radius = 8.;
    look.pad_icon = 12.;
    look.pad = 24.;
    look.icon_box = None;
    look.font = "Orbitron".into();
    look.text_size = 12.;
    look.border = Some((color_alpha(0x7a9c86, 0.4), 1.));
    look
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
