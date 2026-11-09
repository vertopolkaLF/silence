//! Volume → OSD.
use super::super::form::{self, field, percent_slider};
use super::super::overlay;
use super::{column, heading};
use crate::settings_ui::{
    SettingsApp,
    controls::{self, RangeOptions, SelectOption, SelectOptions},
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

    let positioning = controls::toggle(
        "volume-osd-positioning",
        app.sections.volume.positioning,
        window,
        cx,
        |app, active, _, cx| {
            app.sections.volume.positioning = active;
            crate::request_volume_osd_positioning(active);
            cx.notify();
        },
    );
    let appearance = appearance(&osd, app, window, cx);

    vec![column()
        .gap(px(0.))
        .child(heading("OSD", "", Some(enabled), cx))
        .child(
            form::panel(18., false, cx)
                .child(field("Visibility", visibility))
                .child(duration)
                .child(super::super::general::card_row(
                    "Show when the default device changes",
                    Some("Speakers, headphones or microphone: shows the new device's name."),
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
                .child(div().h(px(8.)))
                .child(super::super::general::card_row(
                    "Move OSD",
                    Some("Shows the OSD so you can drag it into place, until this is turned off."),
                    positioning,
                    true,
                    cx,
                )),
        )
        .child(appearance)
        .into_any_element()]
}

/// Integer slider bound to an OSD field; flashes the OSD on change.
fn osd_slider(
    id: &'static str,
    label: &'static str,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    suffix: &str,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    write: impl Fn(&mut crate::VolumeOsdConfig, f64) + 'static,
) -> Stateful<Div> {
    let write = std::rc::Rc::new(write);
    form::int_slider(
        id,
        label,
        value,
        min,
        max,
        step,
        suffix,
        window,
        cx,
        move |app, value, _, cx| {
            let write = write.clone();
            update_and_preview(app, cx, move |osd| write(osd, value));
        },
    )
}

fn osd_select(
    id: &'static str,
    label: &'static str,
    value: String,
    options: SelectOptions,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    write: impl Fn(&mut crate::VolumeOsdConfig, String) + 'static,
) -> Div {
    let write = std::rc::Rc::new(write);
    field(
        label,
        controls::select(id, value, options, app, window, cx, move |app, value, _, cx| {
            let value = value.to_string();
            let write = write.clone();
            update_and_preview(app, cx, move |osd| write(osd, value));
        }),
    )
}

fn icon_options(values: &[(&str, &str, &str)]) -> SelectOptions {
    SelectOptions::new(
        values
            .iter()
            .map(|(value, label, icon)| SelectOption::new(*value, *label).icon(*icon))
            .collect(),
    )
}

/// Theme, the Custom theme's knobs, corner radius and size: the same set as
/// the microphone overlay's Appearance block.
fn appearance(
    osd: &crate::VolumeOsdConfig,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let theme = osd_select(
        "volume-osd-theme",
        "Theme",
        osd.theme.clone(),
        overlay::theme_options(),
        app,
        window,
        cx,
        |osd, value| osd.theme = value,
    );
    let custom = osd.theme == "Custom";
    let mut fonts = app
        .snapshot
        .system_fonts
        .iter()
        .map(|f| SelectOption::new(f.family.clone(), f.family.clone()).font_family(f.family.clone()))
        .collect::<Vec<_>>();
    if !osd.text_font.is_empty()
        && !fonts.iter().any(|f| f.value.eq_ignore_ascii_case(&osd.text_font))
    {
        fonts.insert(
            0,
            SelectOption::new(osd.text_font.clone(), osd.text_font.clone())
                .font_family(osd.text_font.clone()),
        );
    }
    let mut font_options = SelectOptions::new(fonts);
    font_options.searchable = true;
    let font = osd_select(
        "volume-osd-font",
        "Font",
        osd.text_font.clone(),
        font_options,
        app,
        window,
        cx,
        |osd, value| osd.text_font = value,
    );
    let weight = osd_slider(
        "volume-osd-weight",
        "Font weight",
        f64::from(osd.text_font_weight),
        100.,
        900.,
        100.,
        "",
        window,
        cx,
        |osd, value| osd.text_font_weight = value as u16,
    );
    let icon_style = osd_select(
        "volume-osd-icon-style",
        "Icon style",
        osd.icon_style.clone(),
        icon_options(&[
            ("Colored", "Colored", "pallete-2-linear"),
            ("Monochrome", "Monochrome", "ic-baseline-contrast"),
            ("SystemColor", "System color", "widget-linear"),
        ]),
        app,
        window,
        cx,
        |osd, value| osd.icon_style = value,
    );
    let background = osd_select(
        "volume-osd-background",
        "Background",
        osd.background_style.clone(),
        icon_options(&[("Dark", "Dark", "moon-linear"), ("Light", "Light", "sun-2-linear")]),
        app,
        window,
        cx,
        |osd, value| osd.background_style = value,
    );
    let background_opacity = osd_slider(
        "volume-osd-bg-opacity",
        "Background opacity",
        f64::from(osd.background_opacity),
        0.,
        100.,
        5.,
        "%",
        window,
        cx,
        |osd, value| osd.background_opacity = value as u8,
    );
    let border = controls::checkbox(
        "volume-osd-border",
        osd.show_border,
        false,
        "Show border",
        window,
        cx,
        |app, value, _, cx| update_and_preview(app, cx, |osd| osd.show_border = value),
    );
    let opacity = osd_slider(
        "volume-osd-opacity",
        "Opacity",
        f64::from(osd.content_opacity),
        20.,
        100.,
        5.,
        "%",
        window,
        cx,
        |osd, value| osd.content_opacity = value as u8,
    );
    let custom_content = div()
        .flex()
        .flex_col()
        .gap(px(18.))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_end()
                .gap(px(14.))
                .child(font.flex_basis(px(280.)).flex_grow(1.))
                .child(div().flex_basis(px(220.)).flex_grow(1.).min_w_0().child(weight)),
        )
        .child(
            div()
                .flex()
                .gap(px(14.))
                .child(icon_style.flex_1())
                .child(background.flex_1()),
        )
        .child(background_opacity)
        .child(border)
        .child(opacity);
    let supports_radius = crate::gpui_overlay::theme::supports_corner_radius(
        crate::gpui_overlay::volume::normalize_theme(&osd.theme),
    );
    let radius_value = osd
        .corner_radii
        .get(&osd.theme)
        .map(|radius| f64::from(*radius))
        .unwrap_or_else(|| f64::from(crate::gpui_overlay::volume::preset_radius(&osd.theme)));
    let radius = osd_slider(
        "volume-osd-radius",
        "Border radius",
        radius_value.min(32.),
        0.,
        32.,
        1.,
        "px",
        window,
        cx,
        |osd, value| {
            osd.corner_radii.insert(osd.theme.clone(), value as u8);
        },
    );
    let scale = osd_slider(
        "volume-osd-scale",
        "Size scale",
        f64::from(osd.scale),
        10.,
        400.,
        5.,
        "%",
        window,
        cx,
        |osd, value| osd.scale = value as u32,
    );
    form::panel(18., true, cx)
        .mt(px(18.))
        .child(form::section_heading("Appearance"))
        .child(theme)
        .child(form::collapse("volume-osd-custom", custom, custom_content, window, cx))
        .child(form::collapse(
            "volume-osd-radius-collapse",
            supports_radius,
            div().child(radius),
            window,
            cx,
        ))
        .child(scale)
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
