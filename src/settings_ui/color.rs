//! CSS Color 4/5 math used by the settings stylesheet: `oklch()`, relative
//! `oklch(from ...)` colors and `color-mix(in oklch, ...)`.
use gpui::{Hsla, Rgba};

/// An OKLCH color with alpha. `h` is `None` for achromatic ("powerless") hues.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Oklch {
    pub l: f32,
    pub c: f32,
    pub h: Option<f32>,
    pub alpha: f32,
}

pub(crate) const TRANSPARENT: Oklch = Oklch {
    l: 0.,
    c: 0.,
    h: None,
    alpha: 0.,
};

/// `oklch(L% C H)` with L given in percent, as written in the stylesheet.
pub(crate) const fn oklch(l_percent: f32, c: f32, h: f32) -> Oklch {
    Oklch {
        l: l_percent / 100.,
        c,
        h: Some(h),
        alpha: 1.,
    }
}

/// Achromatic `oklch(L% 0 0 / A%)`.
pub(crate) const fn gray(l_percent: f32, alpha_percent: f32) -> Oklch {
    Oklch {
        l: l_percent / 100.,
        c: 0.,
        h: None,
        alpha: alpha_percent / 100.,
    }
}

impl Oklch {
    pub fn from_srgb8(r: u8, g: u8, b: u8) -> Self {
        Self::from_srgb(r as f32 / 255., g as f32 / 255., b as f32 / 255., 1.)
    }

    pub fn from_srgb(r: f32, g: f32, b: f32, alpha: f32) -> Self {
        let lin = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let (r, g, b) = (lin(r), lin(g), lin(b));
        let l = 0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b;
        let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
        let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
        let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
        let lightness = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
        let a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
        let bb = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
        let chroma = (a * a + bb * bb).sqrt();
        let hue = (chroma > 1e-4).then(|| bb.atan2(a).to_degrees().rem_euclid(360.));
        Self {
            l: lightness,
            c: chroma,
            h: hue,
            alpha,
        }
    }

    /// `oklch(from self L C*factor h)`.
    pub fn relative(self, l_percent: f32, chroma_factor: f32) -> Self {
        Self {
            l: l_percent / 100.,
            c: self.c * chroma_factor,
            h: self.h,
            alpha: 1.,
        }
    }

    pub fn with_alpha(self, alpha: f32) -> Self {
        Self { alpha, ..self }
    }

    pub fn opacity(self, factor: f32) -> Self {
        Self {
            alpha: self.alpha * factor,
            ..self
        }
    }

    pub fn to_rgba(self) -> Rgba {
        let h = self.h.unwrap_or(0.).to_radians();
        let (a, b) = (self.c * h.cos(), self.c * h.sin());
        let l_ = self.l + 0.396_337_78 * a + 0.215_803_76 * b;
        let m_ = self.l - 0.105_561_346 * a - 0.063_854_17 * b;
        let s_ = self.l - 0.089_484_18 * a - 1.291_485_5 * b;
        let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
        let r = 4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s;
        let g = -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s;
        let bl = -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s;
        let encode = |v: f32| {
            let v = v.clamp(0., 1.);
            if v <= 0.003_130_8 {
                v * 12.92
            } else {
                1.055 * v.powf(1. / 2.4) - 0.055
            }
        };
        Rgba {
            r: encode(r),
            g: encode(g),
            b: encode(bl),
            a: self.alpha.clamp(0., 1.),
        }
    }

    pub fn hsla(self) -> Hsla {
        self.to_rgba().into()
    }
}

impl From<Oklch> for Hsla {
    fn from(color: Oklch) -> Self {
        color.hsla()
    }
}

/// `color-mix(in oklch, a pa%, b pb%)`. Pass `None` for an omitted percentage.
pub(crate) fn mix(a: Oklch, pa: Option<f32>, b: Oklch, pb: Option<f32>) -> Oklch {
    let (pa, pb) = match (pa, pb) {
        (Some(pa), Some(pb)) => (pa, pb),
        (Some(pa), None) => (pa, 100. - pa),
        (None, Some(pb)) => (100. - pb, pb),
        (None, None) => (50., 50.),
    };
    let sum = pa + pb;
    if sum <= 0. {
        return TRANSPARENT;
    }
    // Percentages summing below 100% scale the result's alpha.
    let alpha_scale = (sum / 100.).min(1.);
    let (wa, wb) = (pa / sum, pb / sum);
    let alpha = a.alpha * wa + b.alpha * wb;
    let premul = |va: f32, vb: f32| {
        if alpha <= 0. {
            0.
        } else {
            (va * a.alpha * wa + vb * b.alpha * wb) / alpha
        }
    };
    // Missing hues and fully transparent colors take the other color's channel.
    let (al, ac) = if a.alpha <= 0. { (b.l, b.c) } else { (a.l, a.c) };
    let (bl, bc) = if b.alpha <= 0. { (a.l, a.c) } else { (b.l, b.c) };
    let hue = match (a.h, b.h) {
        (Some(ha), Some(hb)) => {
            let mut delta = hb - ha;
            if delta > 180. {
                delta -= 360.;
            } else if delta < -180. {
                delta += 360.;
            }
            Some((ha + delta * wb).rem_euclid(360.))
        }
        (Some(h), None) | (None, Some(h)) => Some(h),
        (None, None) => None,
    };
    Oklch {
        l: premul(al, bl),
        c: premul(ac, bc),
        h: hue,
        alpha: alpha * alpha_scale,
    }
}

/// Shorthand for `color-mix(in oklch, a p%, b)`.
pub(crate) fn mix_pct(a: Oklch, pa: f32, b: Oklch) -> Oklch {
    mix(a, Some(pa), b, None)
}

/// Linear interpolation used by animated transitions (CSS interpolates
/// colors in premultiplied oklab; oklch with shorter hue is visually close).
pub(crate) fn lerp(from: Oklch, to: Oklch, t: f32) -> Oklch {
    if t <= 0. {
        return from;
    }
    if t >= 1. {
        return to;
    }
    mix(from, Some((1. - t) * 100.), to, Some(t * 100.))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn round_trips_srgb() {
        let c = Oklch::from_srgb8(255, 128, 0).to_rgba();
        assert!(close(c.r, 1.) && close(c.g, 128. / 255.) && close(c.b, 0.));
    }

    #[test]
    fn mixing_with_transparent_scales_alpha() {
        let c = mix(oklch(50., 0.1, 30.), Some(20.), TRANSPARENT, None);
        assert!(close(c.alpha, 0.2) && close(c.l, 0.5) && close(c.c, 0.1));
    }

    #[test]
    fn mixing_under_100_percent_reduces_alpha() {
        let c = mix(TRANSPARENT, Some(100.), oklch(50., 0.1, 30.), Some(1.));
        assert!(close(c.alpha, 0.0099));
    }
}
