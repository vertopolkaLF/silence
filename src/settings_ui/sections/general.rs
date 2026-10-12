//! General tab (`src/gui/sections/general.rs`, `assets/styles/general.css`).
use crate::settings_ui::{
    SettingsApp,
    anim::{self, Easing, Transition},
    assets::icon,
    color::{gray, lerp},
    controls::{self, ButtonKind},
    style::{self, palette},
};
use gpui::{
    AnyElement, BoxShadow, Context, Div, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, Styled, Window, anchored, deferred, div, point, prelude::*, px,
    relative, svg,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfirmAction {
    ImportV1,
    Reset,
}

#[derive(Default)]
pub(crate) struct State {
    confirm: Option<ConfirmAction>,
    /// Set while the confirm dialog plays its exit animation.
    confirm_closing: Option<Instant>,
}

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let advanced = app.snapshot.config.advanced.clone();
    let launch_on_startup = app.snapshot.config.startup.launch_on_startup;

    let mut status = div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(header("General", None, cx))
        .child(toggle_card(
            "general-startup",
            "Launch with Windows",
            None,
            launch_on_startup,
            window,
            cx,
            |app, checked, _, cx| {
                app.update_config(cx, |config| config.startup.launch_on_startup = checked)
            },
        ));
    if crate::settings_mica_available() {
        status = status.child(toggle_card(
            "general-mica",
            "Enable Mica background",
            None,
            advanced.enable_mica,
            window,
            cx,
            |app, checked, _, cx| app.update_config(cx, |config| config.advanced.enable_mica = checked),
        ));
    }
    status = status
        .child(toggle_card(
            "general-double-click",
            "Double-click tray icon to open Settings",
            Some("Disable to remove delay from muting mic with single-click"),
            !advanced.disable_tray_double_click_settings,
            window,
            cx,
            |app, checked, _, cx| {
                app.update_config(cx, |config| {
                    config.advanced.disable_tray_double_click_settings = !checked
                })
            },
        ))
        .child(toggle_card(
            "general-volume-zero",
            "Set mic volume to 0 in addition to mute",
            Some("Enable this if your mute isn't working"),
            advanced.set_mic_volume_to_zero_on_mute,
            window,
            cx,
            |app, checked, _, cx| {
                app.update_config(cx, |config| {
                    config.advanced.set_mic_volume_to_zero_on_mute = checked
                })
            },
        ))
        .child(toggle_card(
            "general-auto-updates",
            "Disable auto-updates",
            Some("Update checks still run, but Windows update notifications stay quiet"),
            advanced.disable_auto_updates,
            window,
            cx,
            |app, checked, _, cx| {
                app.update_config(cx, |config| config.advanced.disable_auto_updates = checked)
            },
        ));

    // `.general-import-export-panel`: grid gap 16px, padding-top 4px, and the
    // parent `.general-panel` gap of 16px above it.
    let import_export = div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .pt(px(4. + 16.))
        .child(header("Import/Export", Some("Backup your settings"), cx))
        .child(
            div()
                .flex()
                .gap(px(12.))
                .max_w(px(720.))
                .child(import_export_button("general-export", "Export", "export-linear", window, cx, |app, _, _, cx| {
                    if crate::export_settings().is_ok() {
                        app.reload_config(cx);
                    }
                }))
                .child(import_export_button("general-import", "Import", "import-linear", window, cx, |app, _, _, cx| {
                    if crate::import_settings().is_ok() {
                        app.reload_config(cx);
                        app.refresh_devices(cx);
                    }
                })),
        )
        .child(action_card(
            "general-import-v1",
            "Import from silence! v.1",
            "Import",
            ConfirmAction::ImportV1,
            false,
            window,
            cx,
        ))
        .child(action_card(
            "general-reset",
            "Reset settings",
            "Reset settings",
            ConfirmAction::Reset,
            true,
            window,
            cx,
        ));

    let import_export = import_export.children(confirm_dialog(app, window, cx));
    vec![status.into_any_element(), import_export.into_any_element()]
}

/// `.auto-mute-header`: title plus optional muted subtitle, gap 8px, 4px below.
pub(crate) fn header(title: &'static str, subtitle: Option<&'static str>, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .mb(px(4.))
        .child(style::h1(title))
        .children(subtitle.map(|text| style::p(text, cx)))
}

use gpui::App;

/// `.sound-card` with an `h2` title, optional description and a trailing
/// control: gap 16px, 18px bottom padding and a bottom rule.
pub(crate) fn card_row(
    title: impl Into<SharedString>,
    description: Option<&'static str>,
    trailing: impl IntoElement,
    last: bool,
    cx: &App,
) -> Div {
    let palette = palette(cx);
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(6.))
        .max_w(px(720.))
        .when(!last, |this| {
            this.pb(px(18.))
                .border_b_1()
                .border_color(palette.border.hsla())
        })
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .min_w_0()
                .child(style::h2(title))
                .children(description.map(|text| style::p(text, cx))),
        )
        .child(trailing)
}

fn toggle_card(
    id: &'static str,
    title: &'static str,
    description: Option<&'static str>,
    checked: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_change: impl Fn(&mut SettingsApp, bool, &mut Window, &mut Context<SettingsApp>) + 'static,
) -> Div {
    let toggle = controls::toggle(id, checked, window, cx, on_change);
    card_row(title, description, toggle, false, cx)
}

/// `.general-reset-card`: no bottom rule; its button turns danger-red on hover.
fn action_card(
    id: &'static str,
    title: &'static str,
    label: &'static str,
    action: ConfirmAction,
    last: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let palette = palette(cx);
    let (pointer, hover, _) = controls::hover_tween(id, window, cx);
    let button = controls::track_pointer(
        div()
            .id(id)
            .h(px(38.))
            .px(px(16.))
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .rounded(px(8.))
            .bg(lerp(palette.surface_2, palette.surface_danger_hover, hover).hsla())
            .text_color(lerp(palette.text, palette.danger_hover, hover).hsla())
            .text_size(px(14.))
            .font_weight(FontWeight(500.))
            .shadow(vec![BoxShadow {
                color: palette.shadow_soft.hsla(),
                offset: point(px(0.), px(1.)),
                blur_radius: px(1.),
                spread_radius: px(0.),
                inset: false,
            }])
            .cursor_pointer()
            .child(label),
        pointer,
    )
    .on_click(cx.listener(move |app, _, _, cx| {
        app.sections.general.confirm = Some(action);
        app.sections.general.confirm_closing = None;
        cx.notify();
    }));
    let _ = last;
    card_row(title, None, button, true, cx)
}

/// `.general-import-export-button`: 64px tile that lifts 1px on hover.
fn import_export_button(
    id: &'static str,
    label: &'static str,
    icon_name: &'static str,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_click: impl Fn(&mut SettingsApp, (), &mut Window, &mut Context<SettingsApp>) + 'static,
) -> impl IntoElement {
    let palette = palette(cx);
    let (pointer, hover, press) = controls::hover_tween(id, window, cx);
    let lift = hover * (1. - press);
    let shadow = if lift > 0.01 {
        BoxShadow {
            color: palette.shadow_faint.hsla(),
            offset: point(px(0.), px(1. + 3. * lift)),
            blur_radius: px(2. + 8. * lift),
            spread_radius: px(0.),
            inset: false,
        }
    } else {
        BoxShadow {
            color: palette.shadow_soft.hsla(),
            offset: point(px(0.), px(1.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
            inset: false,
        }
    };
    controls::track_pointer(
        div()
            .id(id)
            .flex_1()
            .min_w_0()
            .min_h(px(64.))
            .relative()
            .top(px(-lift))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .px(px(16.))
            .rounded(px(12.))
            .bg(lerp(palette.surface_2, palette.surface_hover, hover).hsla())
            .text_color(palette.text.hsla())
            .text_size(px(15.))
            .font_weight(FontWeight(600.))
            .line_height(relative(1.2))
            .shadow(vec![shadow])
            .cursor_pointer()
            .child(
                svg()
                    .path(icon(icon_name))
                    .size(px(21.))
                    .text_color(palette.accent_text.hsla()),
            )
            .child(label),
        pointer,
    )
    .on_click(cx.listener(move |app, _, window, cx| on_click(app, (), window, cx)))
}

fn close_confirm(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    let state = &mut app.sections.general;
    if state.confirm.is_none() || state.confirm_closing.is_some() {
        return;
    }
    state.confirm_closing = Some(Instant::now());
    cx.spawn(async move |this, cx| {
        cx.background_executor().timer(Duration::from_millis(170)).await;
        let _ = this.update(cx, |app, cx| {
            app.sections.general.confirm = None;
            app.sections.general.confirm_closing = None;
            cx.notify();
        });
    })
    .detach();
    cx.notify();
}

/// `.general-confirm-backdrop` + `.general-confirm-modal`, drawn above the
/// whole window.
fn confirm_dialog(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Option<AnyElement> {
    let action = app.sections.general.confirm?;
    let palette = app.palette;
    let closing = app.sections.general.confirm_closing.is_some();
    // Backdrop: 160ms ease fade. Modal: in 180ms cubic-bezier(.22,1,.36,1)
    // from translateY(8px) scale(.98); out 160ms cubic-bezier(.4,0,1,1).
    let backdrop = anim::tween_from(
        window,
        cx,
        "general-confirm-backdrop",
        0.,
        if closing { 0. } else { 1. },
        Transition::new(160, Easing::Ease),
    );
    let modal_spec = if closing {
        Transition::new(160, Easing::Bezier(0.4, 0., 1., 1.))
    } else {
        Transition::new(180, Easing::Bezier(0.22, 1., 0.36, 1.))
    };
    let modal = anim::tween_from(
        window,
        cx,
        "general-confirm-modal",
        0.,
        if closing { 0. } else { 1. },
        modal_spec,
    );
    let shift = if closing { 6. } else { 8. } * (1. - modal);

    let (title, body, confirm_label) = match action {
        ConfirmAction::ImportV1 => (
            "Import from silence! v.1?",
            "Current settings will be replaced by converted v1 settings from the old app data folder.",
            "Import",
        ),
        ConfirmAction::Reset => (
            "Reset all settings?",
            "Current settings will be replaced with defaults immediately.",
            "Reset settings",
        ),
    };

    let cancel = controls::button(
        "general-confirm-cancel",
        ButtonKind::Secondary,
        None,
        Some("Cancel".into()),
        false,
        window,
        cx,
        |app, _, _, cx| close_confirm(app, cx),
    );
    let (pointer, hover, _) = controls::hover_tween("general-confirm-danger", window, cx);
    let danger = controls::track_pointer(
        div()
            .id("general-confirm-danger")
            .h(px(38.))
            .px(px(16.))
            .flex()
            .items_center()
            .rounded(px(8.))
            .bg(lerp(palette.surface_danger, palette.surface_danger_hover, hover).hsla())
            .text_color(lerp(palette.danger, palette.danger_hover, hover).hsla())
            .text_size(px(14.))
            .font_weight(FontWeight(500.))
            .cursor_pointer()
            .child(confirm_label),
        pointer,
    )
    .on_click(cx.listener(move |app, _, _, cx| {
        let result = match action {
            ConfirmAction::ImportV1 => {
                crate::import_v1_settings().and_then(|_| crate::complete_welcome())
            }
            ConfirmAction::Reset => crate::reset_settings(),
        };
        if result.is_ok() {
            app.reload_config(cx);
            app.refresh_devices(cx);
            close_confirm(app, cx);
        }
    }));

    let viewport = window.viewport_size();
    let dialog = div()
        .id("general-confirm-backdrop")
        .size_full()
        .w(viewport.width)
        .h(viewport.height)
        .flex()
        .items_center()
        .justify_center()
        .p(px(24.))
        .bg(gray(0., 58. * backdrop).hsla())
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|app, _, _, cx| close_confirm(app, cx)))
        .child(
            div()
                .id("general-confirm-modal")
                .w(px(420.))
                .max_w_full()
                .relative()
                .top(px(shift))
                .opacity(modal)
                .flex()
                .flex_col()
                .gap(px(22.))
                .p(px(22.))
                .border_1()
                .border_color(palette.border.hsla())
                .rounded(px(10.))
                .bg(palette.hotkey_panel_bg.hsla())
                .shadow(vec![BoxShadow {
                    color: palette.shadow_medium.hsla(),
                    offset: point(px(0.), px(18.)),
                    blur_radius: px(48.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(style::h2(title).text_size(px(20.)))
                        .child(style::p(body, cx)),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(10.))
                        .child(cancel)
                        .child(danger),
                ),
        );
    Some(
        deferred(anchored().position(point(px(0.), px(0.))).child(dialog))
            .with_priority(10)
            .into_any_element(),
    )
}
