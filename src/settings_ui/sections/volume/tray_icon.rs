//! Volume → Tray icon.
use super::super::form::{self, field};
use super::{column, heading};
use crate::settings_ui::{
    SettingsApp,
    anim::UI,
    assets::icon,
    color::lerp,
    controls::{self, SelectOption, SelectOptions},
    style,
    theme::FONT_DISPLAY,
};
use gpui::{prelude::*, *};

const SKINS: &[(crate::VolumeTraySkin, &str)] = &[
    (crate::VolumeTraySkin::Rings, "Rings"),
    (crate::VolumeTraySkin::Digits, "Digits"),
    (crate::VolumeTraySkin::Bar, "Bar"),
    (crate::VolumeTraySkin::Speaker, "Speaker"),
];

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let tray = app.snapshot.config.volume.tray_icon.clone();
    let show = controls::toggle(
        "volume-tray-enabled",
        tray.enabled,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| config.volume.tray_icon.enabled = checked);
        },
    );
    let mut skins = div().w_full().flex().gap(px(10.));
    for (skin, label) in SKINS {
        skins = skins.child(skin_card(*skin, label, tray.skin == *skin, window, cx));
    }
    let mode = match tray.color {
        crate::TrayColorMode::Colored => "Colored",
        crate::TrayColorMode::Monochrome => "Monochrome",
        crate::TrayColorMode::SystemColor => "SystemColor",
    };
    // Same options and icons as the microphone icon color.
    let color_mode = controls::select(
        "volume-tray-color",
        mode,
        SelectOptions::new(vec![
            SelectOption::new("Colored", "Colored").icon("pallete-2-linear"),
            SelectOption::new("Monochrome", "Monochrome").icon("ic-baseline-contrast"),
            SelectOption::new("SystemColor", "System color").icon("widget-linear"),
        ]),
        app,
        window,
        cx,
        |app, value, _, cx| {
            let color = match value.as_ref() {
                "Colored" => crate::TrayColorMode::Colored,
                "Monochrome" => crate::TrayColorMode::Monochrome,
                _ => crate::TrayColorMode::SystemColor,
            };
            app.update_config(cx, |config| config.volume.tray_icon.color = color);
        },
    );
    vec![column()
        .gap(px(0.))
        .child(heading("Tray icon", "", Some(show), cx))
        .child(
            form::panel(18., true, cx)
                .child(field("Style", skins))
                .child(field("Icon color", color_mode))
                .child(
                    style::p(
                        "Scroll over the icon to change the volume. Clicks are set on Wheel & clicks.",
                        cx,
                    )
                    .text_size(px(13.)),
                ),
        )
        .into_any_element()]
}

fn skin_card(
    skin: crate::VolumeTraySkin,
    label: &'static str,
    selected: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Stateful<Div> {
    let p = style::palette(cx);
    let id = SharedString::from(format!("volume-skin-{label}"));
    let active = crate::settings_ui::anim::tween(
        window,
        cx,
        (ElementId::from(id.clone()), "ink"),
        selected as u8 as f32,
        UI,
    );
    let ink = lerp(p.muted, p.accent, active).hsla();
    let preview = skin_preview(skin, 0.66, false, ink, p.danger.hsla());
    form::choice_card(id, label, selected, preview, window, cx, move |app, cx| {
        app.update_config(cx, |config| config.volume.tray_icon.skin = skin);
    })
}

/// Sample glyph for a skin card.
///
/// Drawn as vector shapes: the 16px tray rasterizer looks coarse at card size.
fn skin_preview(
    skin: crate::VolumeTraySkin,
    level: f32,
    muted: bool,
    ink: Hsla,
    danger: Hsla,
) -> AnyElement {
    let level = if muted { 0. } else { level.clamp(0., 1.) };
    let ink = if muted { danger } else { ink };
    match skin {
        crate::VolumeTraySkin::Rings => rings(ink, level),
        crate::VolumeTraySkin::Digits => digits(ink, level),
        crate::VolumeTraySkin::Bar => bars(ink, level),
        crate::VolumeTraySkin::Speaker => speaker(ink, muted),
    }
}

fn rings(ink: Hsla, level: f32) -> AnyElement {
    let inner = 8. + 12. * level;
    div()
        .size(px(34.))
        .rounded_full()
        .border_1()
        .border_color(ink)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .size(px(inner))
                .rounded_full()
                .border_1()
                .border_color(ink),
        )
        .into_any_element()
}

fn digits(ink: Hsla, level: f32) -> AnyElement {
    let percent = (level * 100.).round() as u8;
    div()
        .font_family(FONT_DISPLAY)
        .text_size(px(22.))
        .line_height(relative(1.))
        .font_weight(FontWeight(650.))
        .text_color(ink)
        .child(percent.to_string())
        .into_any_element()
}

fn bars(ink: Hsla, level: f32) -> AnyElement {
    let heights = [0.35, 0.55, 0.85, 0.62, 0.4];
    div()
        .h(px(28.))
        .flex()
        .items_end()
        .gap(px(3.))
        .children(heights.into_iter().map(|height| {
            let height = 6. + 22. * height * (0.35 + 0.65 * level);
            div()
                .w(px(4.))
                .h(px(height))
                .rounded(px(2.))
                .bg(ink)
        }))
        .into_any_element()
}

fn speaker(ink: Hsla, muted: bool) -> AnyElement {
    svg()
        .path(icon(if muted {
            "volume-cross-linear"
        } else {
            "volume-loud-linear"
        }))
        .size(px(30.))
        .text_color(ink)
        .into_any_element()
}

