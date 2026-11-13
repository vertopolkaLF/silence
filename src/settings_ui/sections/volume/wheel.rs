//! Volume → Wheel & clicks.
use super::super::form::{self, SECTION_GAP};
use super::column;
use crate::settings_ui::{
    SettingsApp,
    controls::{self, SelectOption, SelectOptions},
};
use gpui::{prelude::*, *};

const ZONES: &[(crate::WheelZone, &str, &str, &str, &str)] = &[
    (
        crate::WheelZone::Off,
        "Off",
        "Off",
        "The wheel never changes the volume.",
        "forbidden-circle-linear",
    ),
    (
        crate::WheelZone::TrayIcon,
        "TrayIcon",
        "Tray icon",
        "Over the volume icon. Keep it pinned on the taskbar, not behind ^.",
        "volume-loud-linear",
    ),
    (
        crate::WheelZone::NotificationArea,
        "NotificationArea",
        "Notification area",
        "Anywhere over the icons next to the clock.",
        "bell-linear",
    ),
    (
        crate::WheelZone::Taskbar,
        "Taskbar",
        "Whole taskbar",
        "Anywhere on the taskbar, on every monitor.",
        "window-frame-linear",
    ),
];

const CLICKS: &[(&str, &str, &str)] = &[
    ("None", "None", "close-circle-linear"),
    ("Popup", "Volume slider popup", "slider-minimalistic-horizontal-linear"),
    ("ToggleMute", "Mute", "volume-cross-linear"),
    ("OpenMixer", "Windows volume mixer", "tuning-2-linear"),
    ("OpenSoundSettings", "Windows sound settings", "settings-linear"),
    (
        "NextOutputDevice",
        "Next output device",
        "round-transfer-horizontal-linear",
    ),
    ("OpenSettings", "Open silence! settings", "settings-minimalistic-linear"),
];

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let wheel = app.snapshot.config.volume.wheel.clone();
    let clicks = app.snapshot.config.volume.clicks.clone();
    let zone_id = zone_key(wheel.zone);
    let zone_options = SelectOptions::new(
        ZONES
            .iter()
            .map(|(_, key, label, detail, icon)| {
                SelectOption::new(*key, *label).detail(*detail).icon(*icon)
            })
            .collect(),
    );
    let zone_options = zone_options.row("Scroll zone");
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
    let step = form::int_slider(
        "volume-wheel-step",
        "Step per notch",
        f64::from(wheel.step_percent.clamp(1, 10)),
        1.,
        10.,
        1.,
        "%",
        window,
        cx,
        |app, value, _, cx| {
            let value = value.round().clamp(1., 10.) as u8;
            app.update_config(cx, |config| config.volume.wheel.step_percent = value);
        },
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
    vec![column()
        .child(form::page_title("Wheel & clicks", None))
        .child(
            form::block()
                .child(form::card(cx).row(zone).row(step)),
        )
        .child(
            form::block()
                .mt(px(SECTION_GAP))
                .child(form::section_heading("Clicks", cx))
                .child(form::card(cx).row(left).row(double).row(middle)),
        )
        .into_any_element()]
}

fn click_select(
    id: &'static str,
    label: &'static str,
    action: crate::VolumeClickAction,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    write: impl Fn(&mut crate::VolumeClicksConfig, crate::VolumeClickAction) + 'static,
) -> Stateful<Div> {
    controls::select(
        id,
        click_key(action),
        SelectOptions::new(
            CLICKS
                .iter()
                .map(|(key, label, icon)| SelectOption::new(*key, *label).icon(*icon))
                .collect(),
        )
        .row(label),
        app,
        window,
        cx,
        move |app, value, _, cx| {
            let action = click_from_key(value.as_ref());
            app.update_config(cx, |config| write(&mut config.volume.clicks, action));
        },
    )
}

fn zone_key(zone: crate::WheelZone) -> &'static str {
    ZONES
        .iter()
        .find(|(item, ..)| *item == zone)
        .map(|(_, key, ..)| *key)
        .unwrap_or("TrayIcon")
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
