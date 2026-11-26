//! Volume → Wheel & clicks.
use super::super::form::{self, SECTION_GAP};
use super::column;
use crate::settings_ui::{
    SettingsApp,
    controls::{self, SelectOption, SelectOptions},
};
use gpui::{prelude::*, *};

const ZONES: &[(crate::WheelZone, &str, &str, &str)] = &[
    (
        crate::WheelZone::Off,
        "Off",
        "Off",
        "forbidden-circle-linear",
    ),
    (
        crate::WheelZone::TrayIcon,
        "TrayIcon",
        "Tray icon",
        "volume-loud-linear",
    ),
    (
        crate::WheelZone::NotificationArea,
        "NotificationArea",
        "Notification area",
        "bell-linear",
    ),
    (
        crate::WheelZone::Taskbar,
        "Taskbar",
        "Whole taskbar",
        "window-frame-linear",
    ),
];

/// (key, label, detail, group, icon). Options are listed group by group.
const CLICKS: &[(&str, &str, &str, Option<&str>, &str)] = &[
    ("None", "None", "Do nothing", None, "close-circle-linear"),
    (
        "Popup",
        "Volume slider popup",
        "Flyout with a volume slider",
        Some("Volume"),
        "slider-minimalistic-horizontal-linear",
    ),
    (
        "ToggleMute",
        "Mute",
        "Toggle mute on the output device",
        Some("Volume"),
        "volume-cross-linear",
    ),
    (
        "NextOutputDevice",
        "Next output device",
        "Switch to the next speakers or headphones",
        Some("Volume"),
        "round-transfer-horizontal-linear",
    ),
    (
        "OpenMixer",
        "Windows volume mixer",
        "Per-app volume levels",
        Some("Windows"),
        "tuning-2-linear",
    ),
    (
        "OpenSoundSettings",
        "Windows sound settings",
        "Sound page in Windows Settings",
        Some("Windows"),
        "settings-linear",
    ),
    (
        "OpenSettings",
        "Open silence! settings",
        "This window",
        Some("silence!"),
        "settings-minimalistic-linear",
    ),
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
            .map(|(_, key, label, icon)| SelectOption::new(*key, *label).icon(*icon))
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
    let app_buttons = controls::toggle(
        "volume-wheel-app-buttons",
        wheel.app_buttons,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| config.volume.wheel.app_buttons = checked);
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
            form::block().child(
                form::card(cx)
                    .row(zone)
                    .row(step)
                    .row(form::setting_row(
                        "Per-app volume on the taskbar",
                        Some("Scroll over an app's taskbar button to change only that app's volume."),
                        app_buttons,
                        cx,
                    )),
            ),
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
    let mut options = SelectOptions::new(
        CLICKS
            .iter()
            .map(|(key, label, detail, group, icon)| {
                let option = SelectOption::new(*key, *label).detail(*detail).icon(*icon);
                match group {
                    Some(group) => option.group(*group),
                    None => option,
                }
            })
            .collect(),
    )
    .row(label);
    // The details explain options inside the menu; the row stays one line.
    options.show_current_detail = false;
    controls::select(
        id,
        click_key(action),
        options,
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
