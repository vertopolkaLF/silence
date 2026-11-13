//! Volume → Tray icon.
use super::super::form::{self, card_field};
use super::column;
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
    let mut skins = div().w_full().flex().gap(px(8.));
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
        ])
        .row("Icon color"),
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
    let ring_start = controls::select(
        "volume-tray-ring-start",
        match tray.ring_start {
            crate::RingStart::Top => "Top",
            crate::RingStart::Bottom => "Bottom",
        },
        SelectOptions::new(vec![
            SelectOption::new("Top", "Top"),
            SelectOption::new("Bottom", "Bottom"),
        ])
        .row("Ring starts at"),
        app,
        window,
        cx,
        |app, value, _, cx| {
            let start = if value.as_ref() == "Bottom" {
                crate::RingStart::Bottom
            } else {
                crate::RingStart::Top
            };
            app.update_config(cx, |config| config.volume.tray_icon.ring_start = start);
        },
    );
    let ring_direction = controls::select(
        "volume-tray-ring-direction",
        if tray.ring_clockwise {
            "Clockwise"
        } else {
            "Counterclockwise"
        },
        SelectOptions::new(vec![
            SelectOption::new("Clockwise", "Clockwise"),
            SelectOption::new("Counterclockwise", "Counterclockwise"),
        ])
        .row("Ring direction"),
        app,
        window,
        cx,
        |app, value, _, cx| {
            let clockwise = value.as_ref() == "Clockwise";
            app.update_config(cx, |config| config.volume.tray_icon.ring_clockwise = clockwise);
        },
    );
    let rings = tray.skin == crate::VolumeTraySkin::Rings;
    let popup = tray_popup_panel(app, window, cx);
    let dim = form::governed_opacity("volume-tray-governed", tray.enabled, window, cx);
    let master = form::master_switch(
        "volume-tray-master",
        "widget-bold",
        "Show the volume icon in the tray",
        tray.enabled,
        show,
        window,
        cx,
    );
    vec![column()
        .child(form::page_title("Tray icon", None))
        .child(master)
        .child(
            form::block()
                .mt(px(form::SECTION_GAP))
                .opacity(dim)
                .child(
                    form::card(cx)
                        .row(card_field("Style", skins))
                        .reveal("volume-tray-ring-start", rings, ring_start, window, cx)
                        .reveal(
                            "volume-tray-ring-direction",
                            rings,
                            ring_direction,
                            window,
                            cx,
                        )
                        .row(color_mode),
                )
                .child(form::footnote(
                    "Scroll over the icon to change the volume. Clicks are set on Wheel & clicks.",
                    cx,
                )),
        )
        .child(popup)
        .into_any_element()]
}

/// The slider popup a click on the icon opens.
fn tray_popup_panel(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let popup = app.snapshot.config.volume.popup.clone();
    let layout = controls::select(
        "volume-popup-layout",
        match popup.layout {
            crate::VolumePopupLayout::Compact => "Compact",
            crate::VolumePopupLayout::Labeled => "Labeled",
        },
        SelectOptions::new(vec![
            SelectOption::new("Compact", "Compact: names on hover"),
            SelectOption::new("Labeled", "App names above sliders"),
        ])
        .row("Layout"),
        app,
        window,
        cx,
        |app, value, _, cx| {
            let layout = if value.as_ref() == "Labeled" {
                crate::VolumePopupLayout::Labeled
            } else {
                crate::VolumePopupLayout::Compact
            };
            app.update_config(cx, |config| config.volume.popup.layout = layout);
        },
    );
    let header = controls::toggle(
        "volume-popup-header",
        popup.device_header,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| config.volume.popup.device_header = checked);
        },
    );
    let device_at_bottom = controls::toggle(
        "volume-popup-device-bottom",
        popup.device_at_bottom,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| config.volume.popup.device_at_bottom = checked);
        },
    );
    form::block()
        .mt(px(form::SECTION_GAP))
        .child(form::section_heading("Slider popup", cx))
        .child(
            form::card(cx)
                .row(layout)
                .row(form::setting_row(
                    "Device name header",
                    Some("Shows the output device's name above its slider."),
                    header,
                    cx,
                ))
                .row(form::setting_row(
                    "Device at the bottom",
                    Some("Puts the output slider under the app list."),
                    device_at_bottom,
                    cx,
                )),
        )
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
/// The welcome flow shows the same glyphs.
pub(crate) fn skin_preview(
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
    let inner = 6. + 10. * level;
    div()
        .size(px(28.))
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
        .text_size(px(19.))
        .line_height(relative(1.))
        .font_weight(FontWeight(650.))
        .text_color(ink)
        .child(percent.to_string())
        .into_any_element()
}

fn bars(ink: Hsla, level: f32) -> AnyElement {
    let heights = [0.35, 0.55, 0.85, 0.62, 0.4];
    div()
        .h(px(24.))
        .flex()
        .items_end()
        .gap(px(3.))
        .children(heights.into_iter().map(|height| {
            let height = 5. + 19. * height * (0.35 + 0.65 * level);
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
        .size(px(24.))
        .text_color(ink)
        .into_any_element()
}

