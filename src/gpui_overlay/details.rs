//! Small, state-aware material details painted behind the readable content.
use super::theme::{Detail, Look, color_alpha};
use gpui::{Div, div, prelude::*, px};

pub(super) fn render(
    look: &Look,
    width: f32,
    height: f32,
    scale: f32,
    opacity: f32,
    phase: f32,
) -> Div {
    let w = width / scale;
    let h = height / scale;
    let ink = look.icon;
    let mut layer = div()
        .absolute()
        .left_0()
        .top_0()
        .size_full()
        .opacity(opacity);
    let line = |x: f32, y: f32, width: f32, height: f32, color: u32, alpha: f32| {
        div()
            .absolute()
            .left(px(x * scale))
            .top(px(y * scale))
            .w(px(width.max(0.) * scale))
            .h(px(height.max(0.) * scale))
            .bg(color_alpha(color, alpha))
    };
    let ring = |x: f32, y: f32, diameter: f32, color: u32, alpha: f32| {
        div()
            .absolute()
            .left(px(x * scale))
            .top(px(y * scale))
            .size(px(diameter * scale))
            .rounded(px(diameter * scale / 2.))
            .border_1()
            .border_color(color_alpha(color, alpha))
    };
    match look.detail {
        Detail::None => {}
        Detail::Terminal => {
            // Move one scan-line spacing every 800 ms. Extra rows beyond both
            // edges keep the clipped pattern identical when the offset wraps.
            let spacing = 4.;
            let offset = (phase * 3.).rem_euclid(1.) * spacing;
            for index in -1..(h / spacing).ceil() as i32 + 2 {
                let y = 3. + index as f32 * spacing - offset;
                layer = layer.child(line(0., y, w, 1., ink, 0.045));
            }
        }
        Detail::Blueprint => {
            for x in (0..w as usize).step_by(12) {
                layer = layer.child(line(x as f32, 0., 1., h, 0xa7d6ff, 0.10));
            }
            for y in (0..h as usize).step_by(12) {
                layer = layer.child(line(0., y as f32, w, 1., 0xa7d6ff, 0.10));
            }
            for x in [5., w - 10.] {
                layer = layer.child(line(x, 5., 5., 1., 0xebf5ff, 0.75)).child(line(
                    x,
                    h - 6.,
                    5.,
                    1.,
                    0xebf5ff,
                    0.75,
                ));
            }
        }
        Detail::Cassette => {
            for x in [5., w - 9.] {
                for y in [5., h - 9.] {
                    layer = layer.child(ring(x, y, 4., 0x75664e, 0.65));
                }
            }
            layer = layer
                .child(line(14., 8., w - 28., 3., ink, 0.7))
                .child(line(14., h - 10., w - 28., 1., 0x75664e, 0.4));
        }
        Detail::Arcade => {
            // Alternating square pixels form the cabinet's lower trim.
            for x in (6..w.max(6.) as usize - 4).step_by(8) {
                layer = layer.child(line(x as f32, h - 5., 4., 3., ink, 0.65));
            }
            layer = layer.child(line(5., 4., w - 10., 2., 0xffefab, 0.45));
        }
        Detail::Paper => {
            for y in [h / 2. - 12., h / 2. + 12.] {
                layer = layer.child(line(0., y, w, 1., 0x6c91bc, 0.17));
            }
            layer = layer.child(line(10., 0., 1., h, 0xc75b68, 0.45));
        }
        Detail::Frosted => {
            layer = layer
                .child(line(18., 3., w - 36., 1., 0xffffff, 0.9))
                .child(line(22., h - 4., w - 44., 1., 0x7299b8, 0.22));
        }
        Detail::Porcelain => {
            layer = layer.child(
                div()
                    .absolute()
                    .left(px(4. * scale))
                    .top(px(4. * scale))
                    .w(px((width - 8. * scale).max(0.)))
                    .h(px((height - 8. * scale).max(0.)))
                    .rounded(px(26. * scale))
                    .border_1()
                    .border_color(color_alpha(0x8ca1c9, 0.3)),
            );
        }
        Detail::Radar => {
            let center_x = if look.has_icon && look.has_text {
                let holder_size = look.icon_box.as_ref().map_or(look.icon_size, |b| b.size);
                look.pad_icon + holder_size / 2.
            } else {
                w / 2.
            };
            // Three staggered waves fade in at the mic and dissolve at the edge.
            // The zero opacity at both ends makes the wrapping cycle seamless.
            for index in 0..3 {
                let progress = (phase + index as f32 / 3.).rem_euclid(1.);
                let diameter = 22. + progress * 64.;
                let alpha = (std::f32::consts::PI * progress).sin() * 0.22;
                layer = layer.child(ring(
                    center_x - diameter / 2.,
                    (h - diameter) / 2.,
                    diameter,
                    ink,
                    alpha,
                ));
            }
            if look.has_text {
                for y in [8., h - 9.] {
                    layer = layer.child(line(w - 16., y, 7., 1., ink, 0.55));
                }
            }
        }
    }
    layer
}
