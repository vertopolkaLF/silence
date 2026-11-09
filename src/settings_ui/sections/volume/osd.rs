//! Volume → OSD.
use super::super::form::{field, percent_slider};
use super::super::overlay;
use super::{column, heading};
use crate::settings_ui::{
    SettingsApp,
    controls::{self, RangeOptions, SelectOption, SelectOptions},
    style,
};
use gpui::{prelude::*, *};

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
        |app, checked, _, cx| app.update_config(cx, |config| config.volume.osd.enabled = checked),
    );
    let theme = controls::select(
        "volume-osd-theme",
        osd.theme.clone(),
        overlay::theme_options(),
        app,
        window,
        cx,
        |app, value, _, cx| {
            app.update_config(cx, |config| config.volume.osd.theme = value.to_string());
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
            app.update_config(cx, |config| config.volume.osd.display = value.to_string());
        },
    );
    // TODO(lead): volume OSD drag positioning. The mic overlay drag mode uses
    // `set_overlay_positioning` / `WM_OVERLAY_POSITIONING` and writes
    // `config.overlay`. Do not reuse that IPC here.
    let horizontal = percent_slider(
        "volume-osd-x",
        "Horizontal position",
        f64::from(osd.position_x),
        "%",
        window,
        cx,
        |app, value, _, cx| {
            app.update_config(cx, |config| config.volume.osd.position_x = value as f32);
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
            app.update_config(cx, |config| config.volume.osd.position_y = value as f32);
        },
    );
    let duration_ms = osd.duration_ms;
    let duration = controls::range(
        "volume-osd-duration",
        f64::from(duration_ms),
        RangeOptions {
            min: 0.,
            max: 8_000.,
            step: 100.,
            label: "Duration".into(),
            value_label: format!("{duration_ms} ms").into(),
            icon: Some("clock-circle-linear".into()),
        },
        window,
        cx,
        |app, value, _, cx| {
            app.update_config(cx, |config| {
                config.volume.osd.duration_ms = value.round().clamp(0., 60_000.) as u32;
            });
        },
    );
    let visibility = controls::select(
        "volume-osd-visibility",
        visibility_key(osd.visibility),
        SelectOptions::new(vec![
            SelectOption::new("OnChange", "On change").icon("clock-circle-linear"),
            SelectOption::new("OnChangeAndWhileMuted", "On change and while muted")
                .icon("volume-cross-linear"),
        ]),
        app,
        window,
        cx,
        |app, value, _, cx| {
            let visibility = visibility_from_key(value.as_ref());
            app.update_config(cx, |config| config.volume.osd.visibility = visibility);
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
    vec![column()
        .child(heading(
            "OSD",
            "The on-screen display that appears when the volume changes.",
            Some(enabled),
            cx,
        ))
        .child(field("Theme", theme))
        .child(field("Display", display))
        .child(horizontal)
        .child(vertical)
        .child(duration)
        .child(field("Visibility", visibility))
        .child(super::super::general::card_row(
            "Show when the output device changes",
            None,
            on_device,
            true,
            cx,
        ))
        .child(live_preview_strip(cx))
        .into_any_element()]
}

fn live_preview_strip(cx: &App) -> Div {
    // TODO(lead): live preview strip. OSD rendering is being built by another agent.
    let palette = style::palette(cx);
    div()
        .min_h(px(72.))
        .flex()
        .items_center()
        .px(px(16.))
        .rounded(px(12.))
        .border_1()
        .border_dashed()
        .border_color(palette.border_strong.hsla())
        .bg(crate::settings_ui::color::mix_pct(palette.surface_2, 76., crate::settings_ui::color::TRANSPARENT).hsla())
        .text_size(px(14.))
        .text_color(palette.muted.hsla())
        .child("Live preview")
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
