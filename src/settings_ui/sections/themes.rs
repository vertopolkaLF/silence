//! Theme gallery for both overlays. Every card is a still frame of the real
//! overlay in that theme; one click applies it.
use crate::settings_ui::{
    SettingsApp, anim,
    assets::icon,
    color::{TRANSPARENT, lerp},
    controls::{self, SegmentedToggleOption, segmented_toggle},
    shell::nav::SettingsTab,
    style::{self, BLOCK_GAP},
};
use super::form;
use gpui::{prelude::*, *};

/// Which overlay the gallery dresses.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Target {
    #[default]
    Microphone,
    Volume,
}

#[derive(Default)]
pub(crate) struct State {
    pub target: Target,
}

const COLUMNS: usize = 2;
const TILE_GAP: f32 = 12.;
const STAGE_HEIGHT: f32 = 104.;
const STAGE_INSET: f32 = 6.;
/// Previews are drawn a little smaller than on screen so long labels fit.
const PREVIEW_SCALE: f32 = 0.82;
/// Volume level the OSD previews show.
const PREVIEW_PERCENT: u8 = 64;

/// Sidebar icon and one-line description of a theme.
pub(crate) fn theme_meta(id: &str) -> (&'static str, &'static str) {
    match id {
        "Windows" => ("microsoft-windows", "Native acrylic flyout"),
        "MaterialYou" => ("material-design", "Tonal colors from your accent"),
        "Cute" => ("heart-linear", "Pastel sticker"),
        "CuteSticker" => ("heart-linear", "Die-cut sticker, stick & peel motion"),
        "Neon" => ("bolt-linear", "Glowing tube sign"),
        "Brutalism" => ("box-linear", "Flat slab, hard shadow"),
        "Terminal" => ("monitor-linear", "Upward scan lines, blinking underscore"),
        "Blueprint" => ("tuning-2-linear", "Drafting grid, precise square frame"),
        "Cassette" => ("box-linear", "Tape label, bare mic, vintage cream"),
        "Arcade" => ("bolt-linear", "Pixel edge, violet cabinet, hard shadow"),
        "Paper" => ("box-linear", "Ruled note, red margin, ink lettering"),
        "Frosted" => ("sun-2-linear", "Icy translucent surface, glass highlights"),
        "Porcelain" => ("pallete-2-linear", "Blue ceramic medallion, double rim"),
        "Radar" => ("monitor-linear", "Instrument panel, pulsing signal rings"),
        _ => ("tuning-2-linear", "Your own style, icons and labels"),
    }
}

fn theme_label(id: &str) -> &'static str {
    crate::gpui_overlay::theme::THEMES
        .iter()
        .find(|(theme, _)| *theme == id)
        .map_or("Custom", |(_, label)| label)
}

/// "Theme" card row for the Overlay and OSD pages: the current theme and a way
/// to the gallery, already showing that overlay.
pub(crate) fn theme_link(
    id: &'static str,
    target: Target,
    current: &str,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let p = style::palette(cx);
    let (icon_name, detail) = theme_meta(current);
    let browse = controls::button(
        id,
        controls::ButtonKind::Tonal,
        Some("pallete-2-linear"),
        Some("Browse themes".into()),
        false,
        window,
        cx,
        move |app, _, _, cx| {
            app.sections.themes.target = target;
            app.open_select = None;
            app.nav.select(SettingsTab::Themes);
            cx.notify();
        },
    );
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .flex_shrink_0()
        .min_h(px(super::form::ROW_HEIGHT))
        .px(px(super::form::ROW_PAD))
        .py(px(10.))
        .child(
            div()
                .size(px(36.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(10.))
                .bg(p.surface_3.hsla())
                .child(
                    svg()
                        .path(icon(icon_name))
                        .size(px(18.))
                        .text_color(p.accent_text.hsla()),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .min_w_0()
                .flex_1()
                .child(style::label(format!("Theme: {}", theme_label(current))))
                .child(style::desc(detail, cx)),
        )
        .child(browse)
}

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let target = app.sections.themes.target;
    let switch = segmented_toggle(
        "themes-target",
        match target {
            Target::Microphone => "mic",
            Target::Volume => "volume",
        },
        vec![
            SegmentedToggleOption::new("mic", "Microphone overlay").icon("microphone-3-linear"),
            SegmentedToggleOption::new("volume", "Volume OSD")
                .icon("slider-minimalistic-horizontal-linear"),
        ],
        window,
        cx,
        |app, value, _, cx| {
            app.sections.themes.target = if value.as_ref() == "volume" {
                Target::Volume
            } else {
                Target::Microphone
            };
            cx.notify();
        },
    );
    // The previews swap at once and settle in, so the grid itself never moves.
    let swap = anim::tween(
        window,
        cx,
        (ElementId::from("themes-target"), "swap"),
        (target == Target::Volume) as u8 as f32,
        anim::UI,
    );
    let settle = 1. - (swap - (target == Target::Volume) as u8 as f32).abs();

    let current = match target {
        Target::Microphone => app.snapshot.config.overlay.theme.clone(),
        Target::Volume => app.snapshot.config.volume.osd.theme.clone(),
    };
    // Room for one preview: the content column split into the grid's columns.
    let viewport = window.viewport_size().width.as_f32();
    let column = ((viewport.min(240. + 76. + 10. + 720.) - 240. - 76. - 10.
        - TILE_GAP * (COLUMNS - 1) as f32)
        / COLUMNS as f32)
        .max(160.);
    let stage_width = column - STAGE_INSET * 2. - 2.;

    let mut grid = div().flex().flex_col().gap(px(TILE_GAP)).max_w(px(720.));
    for row in crate::gpui_overlay::theme::THEMES.chunks(COLUMNS) {
        let mut line = div().flex().gap(px(TILE_GAP));
        for (id, label) in row {
            let preview = preview(app, target, id, stage_width - 24., window);
            line = line.child(tile(
                id,
                label,
                *id == current,
                target,
                preview,
                settle,
                window,
                cx,
            ));
        }
        for _ in row.len()..COLUMNS {
            line = line.child(div().flex_1());
        }
        grid = grid.child(line);
    }

    vec![
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .max_w(px(720.))
            .child(form::page_title("Themes", None))
            .child(switch)
            .child(grid.mt(px(BLOCK_GAP)))
            .into_any_element(),
    ]
}

/// Still frame of `theme`, shrunk further when it would not fit the stage.
fn preview(
    app: &SettingsApp,
    target: Target,
    theme: &str,
    max_width: f32,
    window: &mut Window,
) -> crate::gpui_overlay::preview::Preview {
    let draw = |scale: f32, window: &mut Window| match target {
        Target::Microphone => crate::gpui_overlay::preview::mic(
            &app.snapshot.config.overlay,
            theme,
            app.snapshot.muted,
            scale,
            window,
        ),
        Target::Volume => crate::gpui_overlay::volume::preview::level(
            &app.snapshot.config.volume,
            theme,
            PREVIEW_PERCENT,
            false,
            scale,
            window,
        ),
    };
    let first = draw(PREVIEW_SCALE, window);
    let limit = (STAGE_HEIGHT - 2. * STAGE_INSET - 16.).max(1.);
    let fit = (max_width / first.width.max(1.)).min(limit / first.height.max(1.));
    if fit < 1. {
        draw(PREVIEW_SCALE * fit, window)
    } else {
        first
    }
}

#[allow(clippy::too_many_arguments)]
fn tile(
    id: &'static str,
    label: &'static str,
    selected: bool,
    target: Target,
    preview: crate::gpui_overlay::preview::Preview,
    settle: f32,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Stateful<Div> {
    let p = style::palette(cx);
    let key: SharedString = match target {
        Target::Microphone => format!("theme-tile-mic-{id}"),
        Target::Volume => format!("theme-tile-volume-{id}"),
    }
    .into();
    let (pointer, hover, _) = controls::hover_tween(key.clone(), window, cx);
    let active = anim::tween(
        window,
        cx,
        (ElementId::from(key.clone()), "selected"),
        selected as u8 as f32,
        anim::UI,
    );
    let focus = window
        .use_keyed_state((ElementId::from(key.clone()), "focus"), cx, |_, cx| {
            cx.focus_handle()
        })
        .read(cx)
        .clone();
    let (_, detail) = theme_meta(id);
    let apply = move |app: &mut SettingsApp, cx: &mut Context<SettingsApp>| match target {
        Target::Microphone => app.update_config(cx, |config| {
            if config.overlay.theme != id {
                config.overlay.icon_pair =
                    crate::gpui_overlay::theme::default_icon_pair(id).to_string();
                config.overlay.theme = id.to_string();
            }
        }),
        Target::Volume => super::volume::osd::update_and_preview(app, cx, |osd| {
            osd.theme = id.to_string();
        }),
    };
    let apply = std::rc::Rc::new(apply);
    let key_apply = apply.clone();
    let stage = div()
        .relative()
        .h(px(STAGE_HEIGHT))
        .m(px(STAGE_INSET))
        .mb_0()
        .rounded(px(10.))
        .overflow_hidden()
        // A flat, quiet stage for the overlay to sit on.
        .bg(p.surface.hsla())
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .relative()
                .top(px(6. * (1. - settle)))
                .opacity(settle)
                .w(px(preview.width))
                .h(px(preview.height))
                .child(preview.element),
        );
    let footer = div()
        .flex()
        .items_center()
        .gap(px(10.))
        .px(px(12.))
        .pt(px(10.))
        .pb(px(12.))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight(650.))
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_color(lerp(p.text, p.accent_text, active).hsla())
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .line_height(relative(1.3))
                        .text_color(p.muted.hsla())
                        .child(detail),
                ),
        )
        .child(
            div()
                .size(px(20.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(p.accent.opacity(active).hsla())
                .child(
                    svg()
                        .path(icon("check-mark"))
                        .size(px(12.))
                        .text_color(p.on_accent.opacity(active).hsla()),
                ),
        );
    controls::track_pointer(
        div()
            .id(key)
            .track_focus(&focus)
            .tab_index(0)
            .focus_visible(move |style| controls::focus_outline(style, p.focus.hsla()))
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .rounded(px(14.))
            .border_1()
            .border_color(lerp(TRANSPARENT, p.accent, active).hsla())
            .bg(lerp(
                lerp(p.surface_2, p.surface_hover, hover),
                p.accent_soft,
                active * 0.6,
            )
            .hsla())
            .cursor_pointer()
            .child(stage)
            .child(footer),
        pointer,
    )
    .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _, cx| {
        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
            key_apply(app, cx);
            cx.stop_propagation();
        }
    }))
    .on_click(cx.listener(move |app, _, _, cx| apply(app, cx)))
}
