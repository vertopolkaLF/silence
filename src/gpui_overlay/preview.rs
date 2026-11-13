//! Still frames of the overlays for the settings window's theme gallery.
//! They reuse the live renderer's looks and content row; only the animated
//! parts (crossfades, entrance, ambient details) are frozen.
use super::{
    Snapshot, content_row, details, glow, measure_width, sticker,
    theme::{self, Look, Shadow, System, color_alpha},
};
use crate::OverlayConfig;
use gpui::{prelude::*, *};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

/// A preview element and its size in logical pixels.
pub(crate) struct Preview {
    pub element: AnyElement,
    pub width: f32,
    pub height: f32,
}

/// Windows accent and light/dark mode, re-read at most every two seconds:
/// the gallery re-renders on every hover frame.
pub(super) fn system() -> System {
    static CACHE: Mutex<Option<(Instant, System)>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap();
    match *cache {
        Some((at, system)) if at.elapsed() < Duration::from_secs(2) => system,
        _ => {
            let system = System::load();
            *cache = Some((Instant::now(), system));
            system
        }
    }
}

/// The microphone overlay in `theme`, with the user's labels and content.
/// Picking a theme resets the icon pair to the theme's own, so other themes
/// show that one.
pub(crate) fn mic(
    base: &OverlayConfig,
    theme: &str,
    muted: bool,
    scale: f32,
    window: &mut Window,
) -> Preview {
    let mut settings = base.clone();
    if settings.theme != theme {
        settings.icon_pair = theme::default_icon_pair(theme).to_string();
        settings.theme = theme.to_string();
    }
    let pair = settings.icon_pair.clone();
    let state = Snapshot {
        muted,
        settings,
        visible: true,
        positioning: false,
    };
    let mut look = Look::resolve(&state, system());
    if !look.icon_path.starts_with("cute-sticker/") && look.icon_path != "warning" {
        // The settings window serves the same SVGs from its own asset tree.
        let pair = crate::overlay_icons::overlay_icon_pair(&pair);
        let asset = if muted {
            pair.muted_asset
        } else {
            pair.unmuted_asset
        };
        look.icon_path = asset.trim_start_matches("/assets/").to_string().into();
    }
    let width = measure_width(&look, scale, window);
    if look.icon_path.starts_with("cute-sticker/") {
        let height = look.height * scale + (width - 132. * scale).max(0.) * 0.18;
        let element = div()
            .relative()
            .w(px(width))
            .h(px(height))
            .child(sticker::render(&look, width, height, scale, 0., 1.))
            .into_any_element();
        return Preview {
            element,
            width,
            height,
        };
    }
    let height = look.height * scale;
    let radius = if look.acrylic {
        8. * scale
    } else {
        (look.radius * scale).min(height / 2.).min(width / 2.)
    };
    let mut card = div()
        .relative()
        .w(px(width))
        .h(px(height))
        .rounded(px(radius));
    if look.acrylic {
        // DWM acrylic: a frosted base under the theme's tint.
        let base = if system().light {
            color_alpha(0xeeeeee, 0.92)
        } else {
            color_alpha(0x2b2b2b, 0.92)
        };
        card = card
            .bg(base)
            .shadow(vec![shadow((0., 8.), 16., color_alpha(0x000000, 0.28), scale)]);
    } else if look.surface.a > 0. {
        card = card.bg(look.surface);
    }
    if !look.acrylic {
        card = match look.shadow {
            Shadow::Soft if look.surface.a > 0. => card.shadow_md(),
            Shadow::Soft => card,
            Shadow::Drop(color, y, blur) => card.shadow(vec![shadow((0., y), blur, color, scale)]),
            Shadow::Hard(color, x, y) => card.shadow(vec![shadow((x, y), 0., color, scale)]),
            Shadow::Halo(color, blur) => card.shadow(vec![shadow((0., 0.), blur, color, scale)]),
        };
    }
    let mut contents = div()
        .relative()
        .size_full()
        .overflow_hidden()
        .rounded(px(radius));
    if look.acrylic {
        contents = contents.child(
            div()
                .absolute()
                .size_full()
                .rounded(px(radius))
                .bg(look.surface),
        );
    }
    if look.sheen > 0. && look.surface.a > 0. {
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
    if !look.dot {
        if let Some(glow) = glow(&look, 1., radius) {
            contents = contents.child(glow);
        }
        contents = contents
            .child(details::render(&look, width, height, scale, 1., 0.3, radius))
            .child(content_row(&look, width, scale, height, radius, 1., true, window));
    }
    card = card.child(contents);
    if let Some((color, line)) = look.border {
        card = card.child(
            super::border_width(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(width))
                    .h(px(height))
                    .rounded(px(radius)),
                (line * scale).max(1.),
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

fn shadow(offset: (f32, f32), blur: f32, color: Rgba, scale: f32) -> BoxShadow {
    BoxShadow {
        color: color.into(),
        offset: point(px(offset.0 * scale), px(offset.1 * scale)),
        blur_radius: px(blur * scale),
        spread_radius: px(0.),
        inset: false,
    }
}
