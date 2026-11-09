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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdminPrompt {
    /// Turn on "Run as administrator".
    Enable,
    /// The process is still elevated after the setting was turned off.
    RestartUnelevated,
    Notice(&'static str),
}

const ADMIN_CAPTION: &str = "Lets the wheel, media keys and hotkeys work while an administrator app is focused. Asks for permission once.";
const NOTICE_CANCELLED: &str =
    "Windows permission was cancelled, so the setting was not changed.";
const NOTICE_FAILED: &str =
    "silence! could not update the startup task, so the setting was not changed.";
const NOTICE_RESTART_CANCELLED: &str =
    "Windows permission was cancelled. silence! is still running without administrator rights.";
const NOTICE_RESTART_FAILED: &str =
    "silence! could not restart as administrator, so the setting was not changed.";
const NOTICE_UNELEVATED_FAILED: &str =
    "silence! could not restart on its own. Quit it from the tray and start it again from the Start menu. It keeps administrator rights until then.";

#[derive(Default)]
pub(crate) struct State {
    confirm: Option<ConfirmAction>,
    /// Set while the confirm dialog plays its exit animation.
    confirm_closing: Option<Instant>,
    admin_prompt: Option<AdminPrompt>,
    admin_busy: bool,
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
            |app, checked, _, cx| commit_launch(app, checked, cx),
        ))
        .child(admin_card(app, window, cx));
    if crate::settings_mica_available() {
        status = status.child(toggle_card(
            "general-mica",
            "Enable Mica background",
            None,
            advanced.enable_mica,
            window,
            cx,
            |app, checked, _, cx| {
                app.update_config(cx, |config| config.advanced.enable_mica = checked)
            },
        ));
    }
    status = status
        .child(toggle_card(
            "general-volume-zero",
            "Set microphone volume to 0 when muting",
            Some("Turn on if some apps still hear you while muted"),
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
                .child(import_export_button(
                    "general-export",
                    "Export",
                    "export-linear",
                    window,
                    cx,
                    |app, _, _, cx| {
                        if crate::export_settings().is_ok() {
                            app.reload_config(cx);
                        }
                    },
                ))
                .child(import_export_button(
                    "general-import",
                    "Import",
                    "import-linear",
                    window,
                    cx,
                    |app, _, _, cx| {
                        match crate::import_settings() {
                            Ok(()) => {
                                app.reload_config(cx);
                                app.refresh_devices(cx);
                            }
                            Err(err) if crate::startup_change_was_cancelled(&err) => {
                                app.sections.general.admin_prompt =
                                    Some(AdminPrompt::Notice(NOTICE_CANCELLED));
                                cx.notify();
                            }
                            Err(err) => eprintln!("failed to import settings: {err:?}"),
                        }
                    },
                )),
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
    let status = status.children(admin_dialog(app, window, cx));
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

fn admin_card(app: &mut SettingsApp, window: &mut Window, cx: &mut Context<SettingsApp>) -> Div {
    let launch = app.snapshot.config.startup.launch_on_startup;
    let admin = app.snapshot.config.startup.run_as_administrator;
    let elevated = crate::running_as_administrator();
    let busy = app.sections.general.admin_busy;
    let prompt_open = app.sections.general.admin_prompt.is_some();
    let toggle = controls::toggle(
        "general-admin",
        admin,
        window,
        cx,
        |app, checked, _, cx| commit_admin(app, checked, cx),
    );
    let drop_rights = !admin && elevated;
    let restart_link = if !busy && !prompt_open && admin != elevated {
        Some(admin_link(
            if drop_rights {
                "general-admin-drop"
            } else {
                "general-admin-restart"
            },
            if drop_rights {
                "Restart without administrator rights"
            } else {
                "Restart as administrator"
            },
            window,
            cx,
            move |app, _, _, cx| {
                if drop_rights {
                    restart_unelevated(app, cx);
                } else {
                    restart_elevated(app, cx);
                }
            },
        ))
    } else {
        None
    };
    let palette = palette(cx);
    let text = div()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(4.))
        .min_w_0()
        .child(style::h2("Run as administrator"))
        .child(style::p(ADMIN_CAPTION, cx))
        .when(elevated, |this| {
            this.child(
                div()
                    .mt(px(4.))
                    .px(px(8.))
                    .py(px(3.))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(palette.accent_soft.hsla())
                    .text_size(px(12.))
                    .font_weight(FontWeight(600.))
                    .text_color(palette.accent_text.hsla())
                    .child("Running as administrator"),
            )
        })
        .when(!launch, |this| {
            this.child(style::p(
                "Launch with Windows is off, so this only changes the current session until you turn launch on.",
                cx,
            ))
        })
        .when(busy, |this| {
            this.child(style::p("Waiting for Windows permission…", cx))
        })
        .when_some(restart_link, |this, link| this.child(link));
    div()
        .flex()
        .items_start()
        .justify_between()
        .gap(px(16.))
        .max_w(px(720.))
        .pb(px(18.))
        .border_b_1()
        .border_color(palette.border.hsla())
        .child(text)
        .child(toggle)
}

fn admin_link(
    id: &'static str,
    label: &'static str,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_click: impl Fn(&mut SettingsApp, (), &mut Window, &mut Context<SettingsApp>) + 'static,
) -> AnyElement {
    let palette = palette(cx);
    let (pointer, hover, _) = controls::hover_tween(id, window, cx);
    controls::track_pointer(
        div()
            .id(id)
            .cursor_pointer()
            .text_size(px(14.))
            .font_weight(FontWeight(600.))
            .text_color(lerp(palette.accent_text, palette.accent_strong, hover).hsla())
            .child(label),
        pointer,
    )
    .on_click(cx.listener(move |app, _, window, cx| on_click(app, (), window, cx)))
    .into_any_element()
}

fn commit_launch(app: &mut SettingsApp, launch: bool, cx: &mut Context<SettingsApp>) {
    if app.sections.general.admin_busy {
        return;
    }
    let admin = app.snapshot.config.startup.run_as_administrator;
    run_startup_sync(app, launch, admin, None, cx);
}

fn commit_admin(app: &mut SettingsApp, admin: bool, cx: &mut Context<SettingsApp>) {
    if app.sections.general.admin_busy {
        return;
    }
    let launch = app.snapshot.config.startup.launch_on_startup;
    if admin && !crate::running_as_administrator() {
        app.sections.general.admin_prompt = Some(AdminPrompt::Enable);
        cx.notify();
        return;
    }
    let afterward = (!admin && crate::running_as_administrator())
        .then_some(AdminPrompt::RestartUnelevated);
    run_startup_sync(app, launch, admin, afterward, cx);
}

fn run_startup_sync(
    app: &mut SettingsApp,
    launch: bool,
    admin: bool,
    afterward: Option<AdminPrompt>,
    cx: &mut Context<SettingsApp>,
) {
    app.sections.general.admin_prompt = None;
    if !crate::startup_sync_needs_consent(launch, admin) {
        finish_startup_sync(
            app,
            launch,
            admin,
            crate::sync_startup_registration(launch, admin),
            afterward,
            cx,
        );
        return;
    }
    app.sections.general.admin_busy = true;
    let task = cx
        .background_executor()
        .spawn(async move { crate::sync_startup_registration(launch, admin) });
    cx.spawn(async move |this, cx| {
        let result = task.await;
        let _ = this.update(cx, |app, cx| {
            app.sections.general.admin_busy = false;
            finish_startup_sync(app, launch, admin, result, afterward, cx);
        });
    })
    .detach();
    cx.notify();
}

fn finish_startup_sync(
    app: &mut SettingsApp,
    launch: bool,
    admin: bool,
    result: anyhow::Result<()>,
    afterward: Option<AdminPrompt>,
    cx: &mut Context<SettingsApp>,
) {
    match result {
        Ok(()) => {
            app.update_config(cx, |config| {
                config.startup.launch_on_startup = launch;
                config.startup.run_as_administrator = admin;
            });
            app.sections.general.admin_prompt = afterward;
        }
        Err(err) if crate::startup_change_was_cancelled(&err) => {
            app.sections.general.admin_prompt = Some(AdminPrompt::Notice(NOTICE_CANCELLED));
        }
        Err(err) => {
            eprintln!("startup registration failed: {err:?}");
            app.sections.general.admin_prompt = Some(AdminPrompt::Notice(NOTICE_FAILED));
        }
    }
    cx.notify();
}

fn restart_elevated(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    let was_admin = app.snapshot.config.startup.run_as_administrator;
    app.sections.general.admin_prompt = None;
    if !was_admin {
        let launch = app.snapshot.config.startup.launch_on_startup;
        app.update_config(cx, |config| {
            config.startup.launch_on_startup = launch;
            config.startup.run_as_administrator = true;
        });
    }
    match crate::relaunch_elevated() {
        Ok(()) => {}
        Err(err) if crate::startup_change_was_cancelled(&err) => {
            if !was_admin {
                app.update_config(cx, |config| config.startup.run_as_administrator = false);
            }
            app.sections.general.admin_prompt = Some(AdminPrompt::Notice(NOTICE_RESTART_CANCELLED));
        }
        Err(err) => {
            eprintln!("elevated relaunch failed: {err:?}");
            if !was_admin {
                app.update_config(cx, |config| config.startup.run_as_administrator = false);
            }
            app.sections.general.admin_prompt = Some(AdminPrompt::Notice(NOTICE_RESTART_FAILED));
        }
    }
    cx.notify();
}

fn restart_unelevated(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    app.sections.general.admin_prompt = None;
    if let Err(err) = crate::relaunch_unelevated() {
        eprintln!("unelevated relaunch failed: {err:?}");
        app.sections.general.admin_prompt = Some(AdminPrompt::Notice(NOTICE_UNELEVATED_FAILED));
    }
    cx.notify();
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
    // `.sound-card` keeps its bottom border (sounds.css loads after general.css);
    // only `.sound-card:last-child` drops it.
    card_row(title, None, button, last, cx)
}

/// `.general-import-export-button`: 64px tile that lifts 1px on hover.
fn import_export_button(
    id: &'static str,
    label: &'static str,
    icon_name: &'static str,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_click: impl Fn(&mut SettingsApp, (), &mut Window, &mut Context<SettingsApp>) + 'static,
) -> AnyElement {
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
    .into_any_element()
}

fn close_confirm(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    let state = &mut app.sections.general;
    if state.confirm.is_none() || state.confirm_closing.is_some() {
        return;
    }
    state.confirm_closing = Some(Instant::now());
    cx.spawn(async move |this, cx| {
        cx.background_executor()
            .timer(Duration::from_millis(170))
            .await;
        let _ = this.update(cx, |app, cx| {
            app.sections.general.confirm = None;
            app.sections.general.confirm_closing = None;
            cx.notify();
        });
    })
    .detach();
    cx.notify();
}

fn admin_dialog(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Option<AnyElement> {
    let prompt = app.sections.general.admin_prompt?;
    let launch = app.snapshot.config.startup.launch_on_startup;
    let palette = app.palette;
    let (title, body) = match prompt {
        AdminPrompt::Enable if launch => (
            "Run as administrator?",
            "Restart silence! so the wheel, media keys and hotkeys work while an administrator app is focused. Windows asks for permission once. Next sign-in waits until you log on again.",
        ),
        AdminPrompt::Enable => (
            "Run as administrator?",
            "Restart silence! so this session runs as administrator. Launch with Windows is off, so this does not come back after you sign in.",
        ),
        AdminPrompt::RestartUnelevated => (
            "Restart without administrator rights?",
            "silence! is still running as administrator. Restart it so the change applies now. If you skip this, it applies the next time you sign in.",
        ),
        AdminPrompt::Notice(text) => ("silence!", text),
    };
    let secondary_label = match prompt {
        AdminPrompt::Enable if launch => "Next sign-in",
        AdminPrompt::Enable => "Not now",
        AdminPrompt::RestartUnelevated => "Later",
        AdminPrompt::Notice(_) => "OK",
    };
    let primary = match prompt {
        AdminPrompt::Enable => Some("Restart now"),
        AdminPrompt::RestartUnelevated => Some("Restart"),
        AdminPrompt::Notice(_) => None,
    };

    let secondary = controls::button(
        "general-admin-secondary",
        if primary.is_some() {
            ButtonKind::Secondary
        } else {
            ButtonKind::Primary
        },
        None,
        Some(secondary_label.into()),
        false,
        window,
        cx,
        move |app, _, _, cx| match prompt {
            AdminPrompt::Enable => {
                let launch = app.snapshot.config.startup.launch_on_startup;
                run_startup_sync(app, launch, true, None, cx);
            }
            AdminPrompt::RestartUnelevated | AdminPrompt::Notice(_) => {
                app.sections.general.admin_prompt = None;
                cx.notify();
            }
        },
    );
    let primary = primary.map(|label| {
        controls::button(
            "general-admin-primary",
            ButtonKind::Primary,
            None,
            Some(label.into()),
            false,
            window,
            cx,
            move |app, _, _, cx| match prompt {
                AdminPrompt::Enable => restart_elevated(app, cx),
                AdminPrompt::RestartUnelevated => restart_unelevated(app, cx),
                AdminPrompt::Notice(_) => {
                    app.sections.general.admin_prompt = None;
                    cx.notify();
                }
            },
        )
    });

    let viewport = window.viewport_size();
    let dialog = div()
        .id("general-admin-backdrop")
        .occlude()
        .size_full()
        .w(viewport.width)
        .h(viewport.height)
        .flex()
        .items_center()
        .justify_center()
        .p(px(24.))
        .bg(gray(0., 58.).hsla())
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|app, _, _, cx| {
            app.sections.general.admin_prompt = None;
            cx.notify();
        }))
        .child(
            div()
                .id("general-admin-modal")
                .w(px(420.))
                .max_w_full()
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
                        .child(secondary)
                        .when_some(primary, |row, button| row.child(button)),
                ),
        );
    Some(
        deferred(anchored().position(point(px(0.), px(0.))).child(dialog))
            .with_priority(11)
            .into_any_element(),
    )
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
        match result {
            Ok(()) => {
                app.reload_config(cx);
                app.refresh_devices(cx);
                close_confirm(app, cx);
            }
            Err(err) if crate::startup_change_was_cancelled(&err) => {
                close_confirm(app, cx);
                app.sections.general.admin_prompt = Some(AdminPrompt::Notice(NOTICE_CANCELLED));
                cx.notify();
            }
            Err(err) => eprintln!("settings action failed: {err:?}"),
        }
    }));

    let viewport = window.viewport_size();
    let dialog = div()
        .id("general-confirm-backdrop")
        .occlude()
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
