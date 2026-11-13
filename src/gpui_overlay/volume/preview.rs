//! A still frame of the volume OSD's level meter for the settings window's
//! theme gallery. Same chrome and row layout as the live flyout, frozen.
use super::{
    NUMBER_GAP, STICKER_RIM, VolumeChrome, border_width, chrome_for, content_glyph, detail_look,
    fluent_font, fluent_glyph, icon_svg, measure_text, paint_shadow,
};
use crate::{
    gpui_overlay::{
        details,
        preview::Preview,
        theme::{Detail, Shadow, color_alpha},
    },
    volume_osd::VolumeOsdContent,
};
use gpui::{prelude::*, *};

/// The OSD showing `percent` in `theme`, with the user's Custom knobs and
/// that theme's saved corner radius.
pub(crate) fn level(
    config: &crate::VolumeConfig,
    theme: &str,
    percent: u8,
    muted: bool,
    scale: f32,
    window: &mut Window,
) -> Preview {
    let mut style = crate::volume_osd_style(config);
    style.theme = theme.to_string();
    style.corner_radius = config
        .osd
        .corner_radii
        .get(theme)
        .map(|radius| f32::from(*radius));
    let chrome = chrome_for(theme, crate::gpui_overlay::preview::system(), muted, &style);
    let content = VolumeOsdContent::Level { percent, muted };
    let glyph = if chrome.fluent {
        fluent_glyph(&content)
    } else {
        content_glyph(&content)
    };
    meter(&chrome, glyph, percent, scale, window)
}

fn meter(chrome: &VolumeChrome, glyph: &str, percent: u8, scale: f32, window: &Window) -> Preview {
    let label = percent.to_string();
    let percent_slot = measure_text("100", chrome, scale, window).max(20.0 * scale);
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
    let (number_lead, number_trail) = if chrome.number_slot.is_some() {
        (0.0, 0.0)
    } else {
        (gap * NUMBER_GAP, (text_pad - 4.0 * scale).max(pad))
    };
    let number_w = chrome
        .number_slot
        .map(|slot| slot * scale)
        .unwrap_or(number_lead + percent_slot + number_trail);
    let width = (lead + icon_slot + gap + bar_w + number_w).max(icon_slot + lead * 2.0);
    let height = (chrome.height * scale).max(1.0);
    let radius = (chrome.radius * scale).min(height / 2.0).min(width / 2.0);
    let level = f32::from(percent.min(100)) / 100.0;

    let mut card = div()
        .relative()
        .w(px(width))
        .h(px(height))
        .rounded(px(radius))
        .opacity(chrome.opacity)
        .bg(chrome.surface);
    card = match chrome.shadow {
        Shadow::Soft => card.shadow_md(),
        Shadow::Drop(color, y, blur) => card.shadow(vec![paint_shadow((0.0, y), blur, color, scale)]),
        Shadow::Hard(color, x, y) => card.shadow(vec![paint_shadow((x, y), 0.0, color, scale)]),
        Shadow::Halo(color, blur) => card.shadow(vec![paint_shadow((0.0, 0.0), blur, color, scale)]),
    };
    let star = |id: &str, left: f32, top: f32, size: f32, tilt: f32| {
        svg()
            .data(icon_svg(id).unwrap_or_default().as_bytes())
            .absolute()
            .left(px(left))
            .top(px(top))
            .size(px(size))
            .with_transformation(Transformation::rotate(radians(tilt)))
    };
    let badge = icon_slot + STICKER_RIM * 2.0 * scale;
    let (badge_x, badge_y) = (lead + (icon_slot - badge) / 2.0, (height - badge) / 2.0);
    let stars = [
        (width - 34.0 * scale, -11.0 * scale, 22.0 * scale, 0.21),
        (
            badge_x + badge * 0.14 - 7.0 * scale,
            badge_y + badge * 0.86 - 7.0 * scale,
            14.0 * scale,
            -0.3,
        ),
    ];
    if chrome.sticker {
        if let Shadow::Drop(color, y, blur) = chrome.shadow {
            card = card.child(
                div()
                    .absolute()
                    .left(px(badge_x))
                    .top(px(badge_y))
                    .size(px(badge))
                    .rounded(px(badge / 2.0))
                    .shadow(vec![paint_shadow((0.0, y), blur, color, scale)]),
            );
            let mut soft = color;
            soft.a *= 0.6;
            for (left, top, size, tilt) in stars {
                card = card.child(
                    star("sticker-star-paper", left, top + y * scale, size, tilt).text_color(soft),
                );
            }
        }
        card = card
            .child(
                div()
                    .absolute()
                    .size_full()
                    .rounded(px(radius))
                    .bg(rgb(0xffffff)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(badge_x))
                    .top(px(badge_y))
                    .size(px(badge))
                    .rounded(px(badge / 2.0))
                    .bg(rgb(0xffffff)),
            );
        for (left, top, size, tilt) in stars {
            card = card.child(
                star("sticker-star-paper", left, top, size, tilt).text_color(rgb(0xffffff)),
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
                .size_full()
                .rounded(px(radius))
                .bg(linear_gradient(
                    180.0,
                    linear_color_stop(color_alpha(0xffffff, chrome.sheen * chrome.surface.a), 0.0),
                    linear_color_stop(color_alpha(0xffffff, 0.0), 0.55),
                )),
        );
    }
    if !matches!(chrome.detail, Detail::None) {
        contents = contents.child(details::render(
            &detail_look(chrome),
            width,
            height,
            scale,
            1.0,
            0.3,
            radius,
        ));
    }
    let mut icon = div()
        .relative()
        .flex_shrink_0()
        .w(px(icon_slot))
        .h(px(icon_slot))
        .flex()
        .items_center()
        .justify_center();
    if let Some(icon_box) = chrome.icon_box {
        let box_size = icon_box.size * scale;
        let corner = icon_box
            .radius
            .map(|corner| corner * scale)
            .unwrap_or_else(|| (radius * box_size / height).min(box_size / 2.0));
        let mut holder = div()
            .absolute()
            .left(px((icon_slot - box_size) / 2.0))
            .top(px((icon_slot - box_size) / 2.0))
            .w(px(box_size))
            .h(px(box_size))
            .rounded(px(corner.min(box_size / 2.0)))
            .bg(icon_box.fill);
        if let Some((color, line)) = icon_box.border {
            holder = border_width(holder, (line * scale).max(1.0)).border_color(color);
        }
        icon = icon.child(holder);
    }
    icon = icon.child(match glyph.strip_prefix("fluent:") {
        Some(code) => div()
            .font_family(fluent_font())
            .text_size(px(chrome.icon_size * scale))
            .line_height(px(chrome.icon_size * scale))
            .text_color(rgb(chrome.icon_color))
            .child(
                u32::from_str_radix(code, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .unwrap_or(' ')
                    .to_string(),
            )
            .into_any_element(),
        None => {
            let mut svg = svg()
                .data(icon_svg(glyph).unwrap_or_default().as_bytes())
                .size(px(chrome.icon_size * scale))
                .text_color(rgb(chrome.icon_color));
            if chrome.icon_tilt != 0.0 {
                svg = svg.with_transformation(Transformation::rotate(radians(chrome.icon_tilt)));
            }
            svg.into_any_element()
        }
    });
    let row = div()
        .absolute()
        .left_0()
        .top_0()
        .w(px(width))
        .h(px(height))
        .flex()
        .items_center()
        .pl(px(lead))
        .child(icon)
        .child(
            div().pl(px(gap)).flex_shrink_0().child(
                div()
                    .w(px(bar_w))
                    .h(px(bar_h.max(1.0)))
                    .rounded(px(bar_r))
                    .bg(chrome.track)
                    .when(level > 0.0, |rail| {
                        rail.child(
                            div()
                                .h(px(bar_h.max(1.0)))
                                .w(px((bar_w * level).max(bar_r * 2.0).max(1.0)))
                                .rounded(px(bar_r))
                                .bg(chrome.fill),
                        )
                    }),
            ),
        )
        .child(
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
                .text_color(rgb(chrome.foreground))
                .child(label),
        );
    card = card.child(contents.child(row));
    if chrome.sticker {
        for (left, top, size, tilt) in stars {
            card = card.child(
                star("sticker-star-ink", left, top, size, tilt).text_color(chrome.fill),
            );
        }
    }
    if let Some(color) = chrome.outline {
        let line = 1.0 / window.scale_factor().max(0.01);
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
    if let Some((color, line)) = chrome.border {
        card = card.child(
            border_width(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(width))
                    .h(px(height))
                    .rounded(px(radius)),
                (line * scale).max(1.0),
            )
            .border_color(color),
        );
    }
    Preview {
        element: card.into_any_element(),
        width,
        height,
    }
}
