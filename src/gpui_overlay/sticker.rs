//! Two monochrome SVG masks make a real die-cut outline, not a rounded card.
//! Outline text once with the bundled font so GPUI never depends on SVG fonts.
use super::theme::Look;
use gpui::{Transformation, div, point, prelude::*, px, radians, rgb, size, svg};
use resvg::usvg;
use std::{cell::RefCell, collections::VecDeque, sync::OnceLock};

const MIC_CENTER_X: f32 = 58.;
const CAPTION_OFFSET_X: f32 = 18.;
const CAPTION_BASELINE: f32 = 94.;

#[derive(Clone, PartialEq)]
struct Key {
    label: String,
    muted: bool,
    icon: bool,
    text: bool,
    width: u32,
}

#[derive(Clone)]
struct Artwork {
    paper: Vec<u8>,
    ink: Vec<u8>,
    fold: Vec<u8>,
}

thread_local! {
    // Slider changes and frame ticks reuse paths; bound memory for edited labels.
    static ART: RefCell<VecDeque<(Key, Artwork)>> = const { RefCell::new(VecDeque::new()) };
}

fn options() -> &'static usvg::Options<'static> {
    static OPTIONS: OnceLock<usvg::Options<'static>> = OnceLock::new();
    OPTIONS.get_or_init(|| {
        let mut options = usvg::Options::default();
        options
            .fontdb_mut()
            .load_font_data(include_bytes!("../../assets/fonts/overlay/Nunito.ttf").to_vec());
        options
    })
}

fn artwork(look: &Look, logical_width: f32) -> Artwork {
    let key = Key {
        label: look.label.to_string(),
        muted: look.icon_path.ends_with("muted"),
        icon: look.has_icon,
        text: look.has_text,
        width: logical_width.ceil() as u32,
    };
    ART.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, art)) = cache.iter().find(|(cached, _)| *cached == key) {
            return art.clone();
        }
        let art = Artwork {
            paper: mask(&key, true),
            ink: mask(&key, false),
            fold: fold(&key),
        };
        if cache.len() == 16 {
            cache.pop_front();
        }
        cache.push_back((key, art.clone()));
        art
    })
}

fn fold(key: &Key) -> Vec<u8> {
    let width = key.width as f32;
    let height = 146. + (width - 132.).max(0.) * 0.18;
    let y = if key.text && key.icon {
        CAPTION_BASELINE - (width - 90.).max(0.) * 0.14
    } else if key.text {
        112.
    } else {
        84.
    };
    let x = if key.text && key.icon {
        width - 14.
    } else {
        width / 2.
            + if key.text {
                (width / 2. - 18.).min(52.)
            } else {
                29.
            }
    };
    format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {width} {height}'><g transform='translate({} {})'><path d='M2 29 Q9 6 29 2 Q30 18 2 29Z' fill='black'/></g></svg>", x - 20., y - 28.).into_bytes()
}

fn mask(key: &Key, paper: bool) -> Vec<u8> {
    let width = key.width as f32;
    // Caption origin is fixed relative to the mic, independent of label width.
    let center = if key.icon && key.text {
        MIC_CENTER_X
    } else {
        width / 2.
    };
    let stroke = if paper { 5.6 } else { 2.6 };
    let mut body = String::new();
    if key.icon {
        // Existing Iconify Lucide assets match the tall mic/slash in the layout reference.
        let icon = if key.muted {
            include_str!("../../assets/icons/overlay/lucide-mic-off.svg")
        } else {
            include_str!("../../assets/icons/overlay/lucide-mic.svg")
        };
        let inner = icon
            .split_once('>')
            .unwrap()
            .1
            .rsplit_once("</svg>")
            .unwrap()
            .0
            .replace("currentColor", "#000")
            .replace("stroke-width=\"2\"", &format!("stroke-width=\"{stroke}\""));
        let top = if key.text { 4. } else { 18. };
        body.push_str(&format!(
            "<g transform=\"translate({} {top}) scale(3.5)\">{inner}</g>",
            center - 42.
        ));
        // Little stars share the white cut edge, like the visual reference.
        for (x, y, s) in [(center + 48., 23., 5.), (center - 48., 69., 3.)] {
            let outline = if paper { 7. } else { 0. };
            body.push_str(&format!("<path d=\"M{x} {} Q{} {} {} {y} Q{} {} {x} {} Q{} {} {} {y} Q{} {} {x} {}Z\" fill=\"#000\" stroke=\"#000\" stroke-width=\"{outline}\" stroke-linejoin=\"round\"/>",
                y-s, x+s*0.25, y-s*0.25, x+s, x+s*0.25, y+s*0.25, y+s,
                x-s*0.25, y+s*0.25, x-s, x-s*0.25, y-s*0.25, y-s));
        }
    }
    if key.text {
        let label = key
            .label
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;");
        let center = center + if key.icon { CAPTION_OFFSET_X } else { 0. };
        let baseline = if key.icon { CAPTION_BASELINE } else { 78. };
        let anchor = if key.icon { "start" } else { "middle" };
        let outline = if paper { 10. } else { 0. };
        body.push_str(&format!("<text x=\"{center}\" y=\"{baseline}\" text-anchor=\"{anchor}\" font-family=\"Nunito\" font-weight=\"900\" font-size=\"26\" fill=\"#000\" stroke=\"#000\" stroke-width=\"{outline}\" stroke-linejoin=\"round\" paint-order=\"stroke fill\" transform=\"rotate(-8 {center} {baseline})\">{label}</text>"));
    }
    let height = 146. + (width - 132.).max(0.) * 0.18;
    let source = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">{body}</svg>"
    );
    // GPUI's SVG masks contain paths only. usvg shapes and converts our text first.
    match usvg::Tree::from_str(&source, options()) {
        Ok(tree) => tree.to_string(&usvg::WriteOptions::default()).into_bytes(),
        Err(error) => {
            eprintln!("Cute Sticker artwork: {error}");
            source.into_bytes()
        }
    }
}

pub(super) fn render(
    look: &Look,
    width: f32,
    height: f32,
    scale: f32,
    lift: f32,
    weight: f32,
) -> gpui::Div {
    let art = artwork(look, width / scale);
    // Stay opaque while the adhesive releases; fade only once the sticker is lifted.
    let opacity = (1. - ((lift - 0.72) / 0.28).clamp(0., 1.)) * weight;
    let transform = Transformation::rotate(radians(-0.26 * lift))
        .with_scaling(size(1. + 0.06 * lift, 1. - 0.30 * lift))
        .with_translation(point(px(9. * scale * lift), px(-22. * scale * lift)));
    let mut sticker = div()
        .absolute()
        .top_0()
        .left_0()
        .w(px(width))
        .h(px(height))
        .opacity(opacity);
    // The shadow separates and softens as the paper lifts off the surface.
    for spread in [0., 1.5, 3.] {
        sticker = sticker.child(
            svg()
                .data(&art.paper)
                .absolute()
                .size_full()
                .text_color(super::color_alpha(0x68475a, 0.10 - spread * 0.016))
                .with_transformation(transform.with_translation(point(
                    px((2. + 9. * lift + spread) * scale),
                    px((3. + 12. * lift + spread) * scale),
                ))),
        );
    }
    sticker = sticker
        .child(
            svg()
                .data(&art.paper)
                .absolute()
                .size_full()
                .text_color(rgb(0xffffff))
                .with_transformation(transform),
        )
        .child(
            svg()
                .data(&art.ink)
                .absolute()
                .size_full()
                .text_color(rgb(look.icon))
                .with_transformation(transform),
        );
    if lift > 0.001 {
        // A rolled-back paper corner follows the lower-right edge of the lettering.
        sticker = sticker.child(
            svg()
                .data(&art.fold)
                .absolute()
                .size_full()
                .opacity(lift)
                .text_color(rgb(0xfff0e1))
                .with_transformation(transform),
        );
    }
    sticker
}
