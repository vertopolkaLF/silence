//! Volume → Wheel & clicks.
use super::super::form::{self, field};
use super::{column, heading, numeric_field};
use crate::settings_ui::{
    SettingsApp,
    controls::{self, SelectOption, SelectOptions},
    style,
};
use gpui::{prelude::*, *};

const ZONES: &[(crate::WheelZone, &str, &str, &str, &str)] = &[
    (
        crate::WheelZone::Off,
        "Off",
        "Off",
        "The wheel does not change the volume.",
        "forbidden-circle-linear",
    ),
    (
        crate::WheelZone::TrayIcon,
        "TrayIcon",
        "Tray icon",
        "Scroll over the volume icon in the tray.",
        "widget-linear",
    ),
    (
        crate::WheelZone::NotificationArea,
        "NotificationArea",
        "Notification area",
        "Scroll anywhere over the icons by the clock.",
        "bell-linear",
    ),
    (
        crate::WheelZone::Taskbar,
        "Taskbar",
        "Whole taskbar",
        "Scroll anywhere on the taskbar.",
        "slider-minimalistic-horizontal-linear",
    ),
];

const CLICKS: &[(&str, &str, &str)] = &[
    ("None", "None", "close-circle-linear"),
    ("Popup", "Volume slider popup", "widget-linear"),
    ("ToggleMute", "Mute", "volume-cross-linear"),
    ("OpenMixer", "Windows volume mixer", "tuning-2-linear"),
    ("OpenSoundSettings", "Windows sound settings", "settings-linear"),
    (
        "NextOutputDevice",
        "Next output device",
        "round-transfer-horizontal-linear",
    ),
    ("OpenSettings", "Open silence! settings", "settings-bold"),
];

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let wheel = app.snapshot.config.volume.wheel.clone();
    let clicks = app.snapshot.config.volume.clicks.clone();
    let zone_id = zone_key(wheel.zone);
    let mut zone_options = SelectOptions::new(
        ZONES
            .iter()
            .map(|(_, key, label, detail, icon)| {
                SelectOption::new(*key, *label).detail(*detail).icon(*icon)
            })
            .collect(),
    );
    zone_options.show_current_detail = false;
    let zone = controls::select(
        "volume-wheel-zone",
        zone_id,
        zone_options,
        app,
        window,
        cx,
        |app, value, _, cx| {
            let zone = zone_from_key(value.as_ref());
            app.update_config(cx, |config| config.volume.wheel.zone = zone);
        },
    );
    let step = numeric_field(
        "volume-wheel-step",
        u64::from(wheel.step_percent),
        1,
        10,
        "%",
        app,
        window,
        cx,
        |config, value| config.volume.wheel.step_percent = value as u8,
    );
    let left = click_select("volume-click-left", "Left click", clicks.left, app, window, cx, |clicks, action| {
        clicks.left = action;
    });
    let double = click_select(
        "volume-click-double",
        "Double click",
        clicks.double,
        app,
        window,
        cx,
        |clicks, action| clicks.double = action,
    );
    let middle = click_select(
        "volume-click-middle",
        "Middle click",
        clicks.middle,
        app,
        window,
        cx,
        |clicks, action| clicks.middle = action,
    );
    let explanation = zone_detail(wheel.zone);
    let tray_hint = wheel.zone == crate::WheelZone::TrayIcon;
    vec![column()
        .child(heading(
            "Wheel & clicks",
            "How the mouse wheel and buttons change the volume.",
            None::<Div>,
            cx,
        ))
        .child(field("Zone", zone))
        .child(
            div()
                .flex()
                .flex_col()
                .child(style::p(explanation, cx).text_size(px(13.)))
                .child(form::collapse(
                    "volume-wheel-tray-hint",
                    tray_hint,
                    div().pt(px(10.)).child(tray_hint_row(cx)),
                    window,
                    cx,
                )),
        )
        .child(
            field("Step", step)
                .child(style::p("Percent per notch, from 1 to 10.", cx).text_size(px(13.))),
        )
        .child(style::h2("Clicks").mt(px(4.)))
        .child(left)
        .child(double)
        .child(middle)
        .into_any_element()]
}

fn tray_hint_row(cx: &App) -> Div {
    let palette = style::palette(cx);
    div()
        .w_full()
        .px(px(14.))
        .py(px(10.))
        .rounded(px(10.))
        .bg(palette.accent_soft.hsla())
        .text_size(px(13.))
        .line_height(relative(1.4))
        .text_color(palette.text.hsla())
        .child("If the icon is hidden behind the ^ arrow, drag it onto the taskbar so the wheel can reach it.")
}

fn click_select(
    id: &'static str,
    label: &'static str,
    action: crate::VolumeClickAction,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    write: impl Fn(&mut crate::VolumeClicksConfig, crate::VolumeClickAction) + 'static,
) -> Div {
    field(
        label,
        controls::select(
            id,
            click_key(action),
            SelectOptions::new(
                CLICKS
                    .iter()
                    .map(|(key, label, icon)| SelectOption::new(*key, *label).icon(*icon))
                    .collect(),
            ),
            app,
            window,
            cx,
            move |app, value, _, cx| {
                let action = click_from_key(value.as_ref());
                app.update_config(cx, |config| write(&mut config.volume.clicks, action));
            },
        ),
    )
}

fn zone_key(zone: crate::WheelZone) -> &'static str {
    ZONES
        .iter()
        .find(|(item, ..)| *item == zone)
        .map(|(_, key, ..)| *key)
        .unwrap_or("TrayIcon")
}

fn zone_detail(zone: crate::WheelZone) -> &'static str {
    ZONES
        .iter()
        .find(|(item, ..)| *item == zone)
        .map(|(_, _, _, detail, _)| *detail)
        .unwrap_or("Scroll over the volume icon in the tray.")
}

fn zone_from_key(key: &str) -> crate::WheelZone {
    ZONES
        .iter()
        .find(|(_, item, ..)| *item == key)
        .map(|(zone, ..)| *zone)
        .unwrap_or(crate::WheelZone::TrayIcon)
}

fn click_key(action: crate::VolumeClickAction) -> &'static str {
    match action {
        crate::VolumeClickAction::None => "None",
        crate::VolumeClickAction::Popup => "Popup",
        crate::VolumeClickAction::ToggleMute => "ToggleMute",
        crate::VolumeClickAction::OpenMixer => "OpenMixer",
        crate::VolumeClickAction::OpenSoundSettings => "OpenSoundSettings",
        crate::VolumeClickAction::NextOutputDevice => "NextOutputDevice",
        crate::VolumeClickAction::OpenSettings => "OpenSettings",
    }
}

fn click_from_key(key: &str) -> crate::VolumeClickAction {
    match key {
        "Popup" => crate::VolumeClickAction::Popup,
        "ToggleMute" => crate::VolumeClickAction::ToggleMute,
        "OpenMixer" => crate::VolumeClickAction::OpenMixer,
        "OpenSoundSettings" => crate::VolumeClickAction::OpenSoundSettings,
        "NextOutputDevice" => crate::VolumeClickAction::NextOutputDevice,
        "OpenSettings" => crate::VolumeClickAction::OpenSettings,
        _ => crate::VolumeClickAction::None,
    }
}
