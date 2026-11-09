//! Volume → OSD.
use super::super::form::{self, field, percent_slider};
use super::super::overlay;
use super::{column, heading};
use crate::settings_ui::{
    SettingsApp,
    controls::{self, RangeOptions, SelectOption, SelectOptions},
    style,
};
use gpui::{prelude::*, *};

/// Saves an OSD setting and flashes the OSD so the change is visible at once.
fn update_and_preview(
    app: &mut SettingsApp,
    cx: &mut Context<SettingsApp>,
    update: impl FnOnce(&mut crate::VolumeOsdConfig),
) {
    app.update_config(cx, |config| update(&mut config.volume.osd));
    crate::request_volume_osd_preview();
}

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let osd = app.snapshot.config.volume.osd.clone();
    let enabled = controls::toggle(
        "volume-osd-enabled",
        osd.enabled,
        window,
        cx,
        |app, checked, _, cx| {
            update_and_preview(app, cx, |osd| osd.enabled = checked);
        },
    );
    let visibility = controls::select(
        "volume-osd-visibility",
        visibility_key(osd.visibility),
        SelectOptions::new(vec![
            SelectOption::new("OnChange", "When the volume changes")
                .detail("Hides after the duration below")
                .icon("volume-loud-linear"),
            SelectOption::new("OnChangeAndWhileMuted", "On change, and while muted")
                .detail("Stays on screen until you unmute")
                .icon("volume-cross-linear"),
        ]),
        app,
        window,
        cx,
        |app, value, _, cx| {
            let visibility = visibility_from_key(value.as_ref());
            update_and_preview(app, cx, |osd| osd.visibility = visibility);
        },
    );
    let duration_ms = osd.duration_ms.clamp(500, 5_000);
    let duration = controls::range(
        "volume-osd-duration",
        f64::from(duration_ms),
        RangeOptions {
            min: 500.,
            max: 5_000.,
            step: 100.,
            label: "Duration".into(),
            value_label: format!("{:.1} s", f64::from(duration_ms) / 1000.).into(),
            icon: None,
        },
        window,
        cx,
        |app, value, _, cx| {
            let ms = (value / 100.).round() as u32 * 100;
            update_and_preview(app, cx, |osd| osd.duration_ms = ms.clamp(500, 5_000));
        },
    );
    let on_device = controls::toggle(
        "volume-osd-device-change",
        osd.show_on_device_change,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| {
                config.volume.osd.show_on_device_change = checked;
            });
        },
    );

    let mut displays = overlay::display_options(&app.snapshot.overlay_displays);
    displays.insert(
        0,
        SelectOption::new(crate::VOLUME_OSD_DISPLAY_CURSOR, "Monitor under cursor")
            .detail("Follows the pointer")
            .icon("cursor-linear"),
    );
    if !displays.iter().any(|option| option.value.as_ref() == osd.display) {
        displays.push(
            SelectOption::new(osd.display.clone(), osd.display.clone()).icon("monitor-linear"),
        );
    }
    let display = controls::select(
        "volume-osd-display",
        osd.display.clone(),
        SelectOptions::new(displays),
        app,
        window,
        cx,
        |app, value, _, cx| {
            let display = value.to_string();
            update_and_preview(app, cx, |osd| osd.display = display);
        },
    );
    let horizontal = percent_slider(
        "volume-osd-x",
        "Horizontal position",
        f64::from(osd.position_x),
        "%",
        window,
        cx,
        |app, value, _, cx| {
            update_and_preview(app, cx, |osd| osd.position_x = value as f32);
        },
    );
    let vertical = percent_slider(
        "volume-osd-y",
        "Vertical position",
        f64::from(osd.position_y),
        "%",
        window,
        cx,
        |app, value, _, cx| {
            update_and_preview(app, cx, |osd| osd.position_y = value as f32);
        },
    );
    let theme = controls::select(
        "volume-osd-theme",
        osd.theme.clone(),
        overlay::theme_options(),
        app,
        window,
        cx,
        |app, value, _, cx| {
            let theme = value.to_string();
            update_and_preview(app, cx, |osd| osd.theme = theme);
        },
    );

    vec![column()
        .gap(px(0.))
        .child(heading("OSD", "", Some(enabled), cx))
        .child(
            form::panel(18., false, cx)
                .child(field("Visibility", visibility))
                .child(duration)
                .child(super::super::general::card_row(
                    "Show when the output device changes",
                    Some("Names the new device, then shows its volume."),
                    on_device,
                    true,
                    cx,
                )),
        )
        .child(
            form::panel(10., false, cx)
                .mt(px(18.))
                .child(field("Display", display))
                .child(div().h(px(4.)))
                .child(horizontal)
                .child(vertical)
                .child(
                    style::p("Changes here show the OSD so you can see where it lands.", cx)
                        .text_size(px(13.))
                        .mt(px(4.)),
                ),
        )
        .child(
            form::panel(18., true, cx)
                .mt(px(18.))
                .child(form::section_heading("Appearance"))
                .child(field("Theme", theme)),
        )
        .into_any_element()]
}

fn visibility_key(visibility: crate::VolumeOsdVisibility) -> &'static str {
    match visibility {
        crate::VolumeOsdVisibility::OnChange => "OnChange",
        crate::VolumeOsdVisibility::OnChangeAndWhileMuted => "OnChangeAndWhileMuted",
    }
}

fn visibility_from_key(key: &str) -> crate::VolumeOsdVisibility {
    match key {
        "OnChangeAndWhileMuted" => crate::VolumeOsdVisibility::OnChangeAndWhileMuted,
        _ => crate::VolumeOsdVisibility::OnChange,
    }
}
