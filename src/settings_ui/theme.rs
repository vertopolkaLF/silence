//! Design tokens ported from `assets/styles/global.css` (`:root` and the
//! `.mica-enabled` override). Values derive from the Windows accent exactly as
//! the stylesheet's `color-mix(in oklch, ...)` expressions do.
use super::color::{Oklch, TRANSPARENT, gray, mix, mix_pct, oklch};
use gpui::{Global, Hsla};
use std::time::Duration;

/// `--ui-transition: 0.2s ease`.
pub(crate) const UI_TRANSITION: Duration = Duration::from_millis(200);

pub(crate) const FONT_TEXT: &str = "Plus Jakarta Sans";
pub(crate) const FONT_HEADING: &str = "Bricolage Grotesque";
/// Bricolage Grotesque at the optical size used for 24px page titles.
pub(crate) const FONT_DISPLAY: &str = "Bricolage Grotesque Display";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Palette {
    pub mica: bool,
    pub windows_accent: Oklch,
    pub neutral_bg: Oklch,
    pub bg: Oklch,
    pub chrome: Oklch,
    pub surface: Oklch,
    pub window_bg: Oklch,
    pub content_bg: Oklch,
    pub select_menu_bg: Oklch,
    pub hotkey_panel_bg: Oklch,
    pub surface_2: Oklch,
    pub surface_3: Oklch,
    pub surface_hover: Oklch,
    pub surface_color: [Oklch; 5],
    pub border: Oklch,
    pub border_strong: Oklch,
    pub text: Oklch,
    pub muted: Oklch,
    pub faint: Oklch,
    pub accent: Oklch,
    pub accent_text: Oklch,
    pub accent_strong: Oklch,
    pub accent_soft: Oklch,
    pub success: Oklch,
    pub danger: Oklch,
    pub danger_hover: Oklch,
    pub surface_danger: Oklch,
    pub surface_danger_hover: Oklch,
    pub focus: Oklch,
    pub shadow_soft: Oklch,
    pub shadow_medium: Oklch,
    pub shadow_faint: Oklch,
    pub highlight_line: Oklch,
    pub on_accent: Oklch,
}

impl Global for Palette {}

impl Palette {
    pub fn load(mica: bool) -> Self {
        let (r, g, b) = crate::WindowsAccent::load().accent;
        Self::new(Oklch::from_srgb8(r, g, b), mica)
    }

    pub fn new(windows_accent: Oklch, mica: bool) -> Self {
        let wa = windows_accent;
        let neutral_bg = oklch(14.08, 0.0044, 285.82);
        let neutral_surface = oklch(17.31, 0.004, 285.99);
        let neutral_surface_2 = oklch(21.03, 0.0059, 285.89);
        let neutral_surface_3 = oklch(27.39, 0.0055, 286.03);
        let text = gray(98.51, 100.);
        let danger = oklch(63.68, 0.2078, 25.33);

        let bg = mix_pct(neutral_bg, 97., wa);
        let surface = mix_pct(neutral_surface, 96., wa);
        let surface_2 = mix_pct(neutral_surface_2, 95., wa);
        let surface_3 = mix_pct(neutral_surface_3, 94., wa);
        let accent_text = mix_pct(text, 65., wa);
        let mut palette = Self {
            mica,
            windows_accent: wa,
            neutral_bg,
            bg,
            chrome: mix_pct(neutral_bg, 96., wa),
            surface,
            window_bg: bg,
            content_bg: surface,
            select_menu_bg: surface_2,
            hotkey_panel_bg: surface,
            surface_2,
            surface_3,
            surface_hover: mix_pct(surface_2, 82., accent_text),
            surface_color: [
                wa.relative(18.5, 0.18),
                wa.relative(21.5, 0.26),
                wa.relative(24.5, 0.34),
                wa.relative(28., 0.42),
                wa.relative(31.5, 0.5),
            ],
            border: mix_pct(oklch(20.39, 0.0055, 286.03), 85., wa),
            border_strong: mix_pct(oklch(37.03, 0.0119, 285.81), 82., wa),
            text,
            muted: oklch(71.18, 0.0129, 286.07),
            faint: oklch(44.19, 0.0146, 285.79),
            accent: wa,
            accent_text,
            accent_strong: mix(wa, None, neutral_bg, Some(18.)),
            accent_soft: mix(wa, None, bg, Some(78.)),
            success: oklch(69.59, 0.1491, 162.48),
            danger,
            danger_hover: oklch(67., 0.22, 29.),
            surface_danger: mix_pct(danger, 10., surface_2),
            surface_danger_hover: mix_pct(danger, 18., surface_3),
            focus: wa,
            shadow_soft: gray(0., 10.),
            shadow_medium: gray(0., 20.),
            shadow_faint: gray(0., 5.),
            highlight_line: gray(100., 3.),
            on_accent: gray(100., 100.),
        };
        if mica {
            palette.apply_mica();
        }
        palette
    }

    fn apply_mica(&mut self) {
        let wa = self.windows_accent;
        let tint = |l: f32, c: f32, alpha: f32| mix_pct(wa.relative(l, c), alpha, TRANSPARENT);
        self.bg = mix(TRANSPARENT, Some(100.), wa, Some(1.));
        self.chrome = mix_pct(self.neutral_bg, 28., TRANSPARENT);
        self.surface = mix(gray(0., 20.), Some(98.), wa, Some(5.));
        self.window_bg = self.bg;
        self.content_bg = self.surface;
        self.select_menu_bg = mix_pct(oklch(25.5, 0.006, 286.), 99., wa);
        self.hotkey_panel_bg = mix_pct(oklch(17.5, 0.006, 286.), 99., wa);
        self.surface_2 = mix_pct(oklch(40., 0.006, 286.), 15., TRANSPARENT);
        self.surface_3 = mix_pct(oklch(48., 0.007, 286.), 20., TRANSPARENT);
        self.surface_color = [
            tint(34., 0.18, 10.),
            tint(39., 0.24, 15.),
            tint(44., 0.3, 20.),
            tint(49., 0.36, 25.),
            tint(54., 0.42, 30.),
        ];
        self.border = mix_pct(oklch(58., 0.012, 286.), 20., TRANSPARENT);
        self.border_strong = mix_pct(oklch(70., 0.014, 286.), 40., TRANSPARENT);
        self.muted = oklch(80.18, 0.0129, 286.07);
        self.faint = oklch(70.19, 0.0146, 285.79);
        self.accent_text = mix_pct(self.text, 40., wa);
        self.surface_hover = mix_pct(self.surface_2, 82., self.accent_text);
        self.accent_soft = mix_pct(wa, 18., TRANSPARENT);
        self.surface_danger = mix_pct(self.danger, 10., self.surface_2);
        self.surface_danger_hover = mix_pct(self.danger, 18., self.surface_3);
        self.highlight_line = gray(100., 2.);
    }

    /// `--surface-color-N` (1-based like the stylesheet).
    pub fn surface_color(&self, n: usize) -> Oklch {
        self.surface_color[n.clamp(1, 5) - 1]
    }
}

/// Converts a token to a GPUI color.
pub(crate) fn hsla(color: Oklch) -> Hsla {
    color.hsla()
}
