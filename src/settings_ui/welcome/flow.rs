//! Dynamic welcome steps: module choice, then microphone and/or volume.
//!
//! Volume² click indexes are still being checked, so import uses only the
//! high-level fields. `None` keeps the silence! default. Hotkeys are not imported.
use super::{SettingsApp, action, block, heading};
use crate::settings_ui::{
    anim::{self, Easing, Transition},
    assets::icon,
    color::{Oklch, lerp},
    controls,
};
use gpui::{
    AnyElement, BoxShadow, Context, Div, FontWeight, Stateful, Transformation, Window, div, point,
    prelude::*, px, relative, size, svg,
};
use std::rc::Rc;

const STEP_FADE: Transition = Transition::new(220, Easing::Bezier(0.22, 1., 0.36, 1.));
/// Modules + three mic screens + the Volume² screen + three volume screens.
const MAX_DOTS: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StepKind {
    Modules,
    MicIntro,
    MicHotkey,
    MicDone,
    Volume2,
    Wheel,
    Skin,
    MediaKeys,
}

pub(super) fn is_hotkey_step(app: &SettingsApp) -> bool {
    current(app) == StepKind::MicHotkey
}

pub(super) fn current(app: &SettingsApp) -> StepKind {
    plan(app)
        .get(app.welcome.step)
        .copied()
        .unwrap_or(StepKind::Modules)
}

pub(super) fn focus_ids(kind: StepKind) -> &'static [&'static str] {
    match kind {
        StepKind::Modules => &[
            "welcome-pick-microphone",
            "welcome-pick-volume",
            "welcome-modules-next",
        ],
        StepKind::MicIntro => &[
            "welcome-intro-back",
            "welcome-import",
            "welcome-set-hotkey",
        ],
        StepKind::MicHotkey => &[],
        StepKind::MicDone => &["welcome-done-back", "welcome-finish"],
        StepKind::Volume2 => &[
            "welcome-v2-import",
            "welcome-v2-autostart",
            "welcome-v2-close",
            "welcome-v2-back",
            "welcome-v2-next",
        ],
        StepKind::Wheel => &[
            "welcome-wheel-off",
            "welcome-wheel-icon",
            "welcome-wheel-area",
            "welcome-wheel-taskbar",
            "welcome-wheel-back",
            "welcome-wheel-next",
        ],
        StepKind::Skin => &[
            "welcome-skin-rings",
            "welcome-skin-digits",
            "welcome-skin-bar",
            "welcome-skin-speaker",
            "welcome-skin-back",
            "welcome-skin-next",
        ],
        StepKind::MediaKeys => &["welcome-media-back", "welcome-media-next"],
    }
}

fn plan(app: &SettingsApp) -> Vec<StepKind> {
    let mut steps = vec![StepKind::Modules];
    if app.welcome.pick_microphone {
        steps.extend([
            StepKind::MicIntro,
            StepKind::MicHotkey,
            StepKind::MicDone,
        ]);
    }
    if app.welcome.pick_volume {
        if volume2_in_flow(app) {
            steps.push(StepKind::Volume2);
        }
        steps.extend([StepKind::Wheel, StepKind::Skin, StepKind::MediaKeys]);
    }
    steps
}

fn volume2_detected(app: &SettingsApp) -> bool {
    app.welcome
        .volume2_detection
        .as_ref()
        .is_some_and(|detection| detection.installed_path.is_some() || detection.running)
}

/// On the module step the dot count follows a live probe. Afterwards it stays
/// latched so a late result cannot renumber the step the user is already on.
fn volume2_in_flow(app: &SettingsApp) -> bool {
    if !app.welcome.pick_volume {
        return false;
    }
    if app.welcome.step == 0 {
        volume2_detected(app)
    } else {
        app.welcome.volume2_included
    }
}

pub(super) fn is_last(app: &SettingsApp) -> bool {
    app.welcome.step + 1 >= plan(app).len()
}

pub(super) fn advance(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) {
    let next = app.welcome.step + 1;
    if next >= plan(app).len() {
        super::finish(app, cx);
    } else {
        super::set_step(app, next, window, cx);
    }
}

pub(super) fn retreat(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) {
    if app.welcome.step > 0 {
        super::set_step(app, app.welcome.step - 1, window, cx);
    }
}

pub(super) fn goto(
    app: &mut SettingsApp,
    kind: StepKind,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) {
    let Some(index) = plan(app).iter().position(|step| *step == kind) else {
        return;
    };
    super::set_step(app, index, window, cx);
}

pub(super) fn progress_label(app: &SettingsApp) -> String {
    let count = plan(app).len().max(1);
    let number = app.welcome.step.min(count - 1) + 1;
    format!("Step {number} of {count}")
}

pub(super) fn progress_fade(
    app: &SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> f32 {
    let count = plan(app).len();
    anim::tween_from(
        window,
        cx,
        ("welcome-step-count", count),
        0.45,
        1.,
        Transition::new(200, Easing::Ease),
    )
}

pub(super) fn progress_dots(
    app: &SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let count = plan(app).len();
    let current = app.welcome.step;
    let palette = app.palette;
    let mut dots = Vec::new();
    for index in 0..MAX_DOTS {
        let present = anim::tween(
            window,
            cx,
            ("welcome-dot-on", index),
            (index < count) as u8 as f32,
            STEP_FADE,
        );
        let active = anim::tween(
            window,
            cx,
            ("welcome-dot-active", index),
            (index == current && index < count) as u8 as f32,
            STEP_FADE,
        );
        let width = (7. + 11. * active) * present;
        dots.push(
            div()
                .w(px(width))
                .h(px(7. * present))
                .mr(px(6. * present))
                .rounded_full()
                .bg(lerp(palette.surface_3, palette.accent, active).hsla())
                .opacity(present),
        );
    }
    div().flex().items_center().h(px(8.)).children(dots)
}

fn probe_volume2(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    if app.welcome.volume2_detection.is_some() || app.welcome.volume2_probe_started {
        return;
    }
    app.welcome.volume2_probe_started = true;
    cx.spawn(async move |this, cx| {
        let task = cx
            .background_executor()
            .spawn(async move { crate::volume2_import::detect_volume2() });
        let detection = task.await;
        let _ = this.update(cx, |app, cx| {
            if app.welcome.volume2_detection.is_none() {
                app.welcome.volume2_detection = Some(detection);
            }
            cx.notify();
        });
    })
    .detach();
}

fn write_modules(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    let microphone = app.welcome.pick_microphone;
    let volume = app.welcome.pick_volume;
    // Both off is not a legal config: load_config turns the microphone back on.
    if !microphone && !volume {
        return;
    }
    app.update_config(cx, |config| {
        config.modules.microphone = microphone;
        config.modules.volume = volume;
    });
}

fn toggle_pick(app: &mut SettingsApp, microphone: bool, cx: &mut Context<SettingsApp>) {
    if app.welcome.closing.is_some() {
        return;
    }
    if microphone {
        app.welcome.pick_microphone = !app.welcome.pick_microphone;
    } else {
        app.welcome.pick_volume = !app.welcome.pick_volume;
        if app.welcome.pick_volume {
            probe_volume2(app, cx);
        }
    }
    write_modules(app, cx);
    cx.notify();
}

fn confirm_modules(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) {
    if app.welcome.closing.is_some()
        || (!app.welcome.pick_microphone && !app.welcome.pick_volume)
    {
        return;
    }
    if app.welcome.pick_volume && app.welcome.volume2_detection.is_none() {
        app.welcome.volume2_detection = Some(crate::volume2_import::detect_volume2());
    }
    app.welcome.volume2_included = app.welcome.pick_volume && volume2_detected(app);
    write_modules(app, cx);
    super::set_step(app, 1, window, cx);
}

pub(super) fn modules_step(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let microphone = module_tile(
        "welcome-pick-microphone",
        "microphone-3-bold",
        "Microphone",
        "Mute your mic anywhere with a hotkey.",
        app.welcome.pick_microphone,
        true,
        app,
        window,
        cx,
    );
    let volume = module_tile(
        "welcome-pick-volume",
        "volume-loud-bold",
        "Volume",
        "Control output volume from the tray, wheel and keyboard.",
        app.welcome.pick_volume,
        false,
        app,
        window,
        cx,
    );
    div()
        .flex()
        .flex_col()
        .gap(px(22.))
        .child(block(
            app,
            80,
            heading(
                "What should silence! handle?",
                Some("Pick Microphone, Volume, or both."),
                false,
                cx,
            ),
        ))
        .child(block(
            app,
            40,
            div()
                .flex()
                .items_stretch()
                .gap(px(12.))
                .child(microphone)
                .child(volume),
        ))
        .child({
            let next = modules_next(app, window, cx);
            block(app, 0, next)
        })
}

fn module_tile(
    id: &'static str,
    icon_name: &'static str,
    title: &'static str,
    detail: &'static str,
    selected: bool,
    microphone: bool,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Stateful<Div> {
    let palette = app.palette;
    let (pointer, hover, press) = controls::hover_tween(id, window, cx);
    let selected_t = anim::tween(window, cx, picked_key(id), selected as u8 as f32, STEP_FADE);
    let bg = lerp(
        lerp(palette.surface_2, palette.surface_hover, hover),
        palette.accent_soft,
        selected_t,
    );
    let icon_scale = 1. + 0.06 * selected_t - 0.08 * press;
    let focus = focus_of(app, id, cx);
    let focus_color = palette.focus.hsla();
    let mouse_focus = focus.clone();
    let element = div()
        .id(id)
        .relative()
        .flex_1()
        .min_w_0()
        .h(px(168.))
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(px(8.))
        .p(px(16.))
        .rounded(px(12.))
        .bg(bg.hsla())
        .cursor_pointer()
        .shadow(vec![
            BoxShadow {
                color: palette.shadow_soft.hsla(),
                offset: point(px(0.), px(12.)),
                blur_radius: px(28.),
                spread_radius: px(0.),
                inset: false,
            },
            BoxShadow {
                color: palette.accent.opacity(selected_t).hsla(),
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(2.),
                inset: true,
            },
        ])
        .track_focus(&focus)
        .focus_visible(move |style| {
            style.shadow(vec![BoxShadow {
                color: focus_color,
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(2.),
                inset: true,
            }])
        })
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            mouse_focus.focus(window, cx)
        })
        .child(
            div().absolute().top(px(12.)).right(px(12.)).child(
                svg()
                    .path(icon("check-circle-bold"))
                    .size(px(22.))
                    .opacity(selected_t)
                    .text_color(palette.accent.hsla())
                    .with_transformation(Transformation::scale(size(
                        0.4 + 0.6 * selected_t,
                        0.4 + 0.6 * selected_t,
                    ))),
            ),
        )
        .child(
            div()
                .relative()
                .top(px(press))
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    svg()
                        .path(icon(icon_name))
                        .size(px(40.))
                        .text_color(lerp(palette.accent, palette.accent_strong, selected_t).hsla())
                        .with_transformation(Transformation::scale(size(icon_scale, icon_scale))),
                )
                .child(
                    div()
                        .font_family("Bricolage Grotesque Welcome Card")
                        .font_weight(FontWeight(600.))
                        .text_size(px(22.))
                        .line_height(relative(1.1))
                        .child(title),
                )
                .child(
                    div()
                        .text_color(palette.muted.hsla())
                        .text_size(px(13.))
                        .line_height(relative(1.4))
                        .child(detail),
                ),
        );
    let element = controls::track_pointer(element, pointer);
    if app.welcome.closing.is_some() {
        return element.cursor_default();
    }
    let keyboard = microphone;
    element
        .on_click(cx.listener(move |app, _, _, cx| toggle_pick(app, microphone, cx)))
        .on_key_down(cx.listener(move |app, event: &gpui::KeyDownEvent, _, cx| {
            if event.keystroke.key == "enter" {
                toggle_pick(app, keyboard, cx);
                cx.stop_propagation();
            }
        }))
        .on_key_up(cx.listener(move |app, event: &gpui::KeyUpEvent, _, cx| {
            if event.keystroke.key == "space" {
                toggle_pick(app, keyboard, cx);
                cx.stop_propagation();
            }
        }))
}

fn modules_next(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Stateful<Div> {
    let id = "welcome-modules-next";
    let ready = app.welcome.pick_microphone || app.welcome.pick_volume;
    let on = anim::tween(window, cx, "welcome-modules-next-on", ready as u8 as f32, STEP_FADE);
    let (pointer, hover, press) = controls::hover_tween(id, window, cx);
    let hover = if ready { hover } else { 0. };
    let press = if ready { press } else { 0. };
    let palette = app.palette;
    let bg = lerp(
        lerp(palette.surface_3, palette.accent, on),
        palette.accent_strong,
        hover * on,
    );
    let fg = lerp(palette.faint, palette.on_accent, on);
    let focus = focus_of(app, id, cx);
    let focus_color = palette.focus.hsla();
    let mouse_focus = focus.clone();
    let closing = app.welcome.closing.is_some();
    let mut button = div()
        .id(id)
        .h(px(44.))
        .min_w(px(104.))
        .px(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .rounded(px(8.))
        .bg(bg.hsla())
        .text_color(fg.hsla())
        .text_size(px(14.))
        .font_weight(FontWeight(600.))
        .opacity(0.45 + 0.55 * on)
        .shadow(vec![BoxShadow {
            color: palette.shadow_soft.opacity(on).hsla(),
            offset: point(px(0.), px(2.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
            inset: false,
        }])
        .track_focus(&focus)
        .focus_visible(move |style| {
            style.shadow(vec![BoxShadow {
                color: focus_color,
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(2.),
                inset: true,
            }])
        })
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            mouse_focus.focus(window, cx)
        })
        .when(closing, |this| this.cursor_default())
        .when(!closing && ready, |this| this.cursor_pointer())
        .when(!closing && !ready, |this| this.cursor_not_allowed())
        .child(
            div()
                .relative()
                .top(px(press))
                .flex()
                .items_center()
                .gap(px(8.))
                .child("Next")
                .child(
                    svg()
                        .path(icon("arrow-right-linear"))
                        .size(px(17.))
                        .opacity(on)
                        .text_color(fg.hsla()),
                ),
        );
    button = controls::track_pointer(button, pointer);
    if closing || !ready {
        return button;
    }
    button
        .on_click(cx.listener(|app, _, window, cx| confirm_modules(app, window, cx)))
        .on_key_down(cx.listener(|app, event: &gpui::KeyDownEvent, window, cx| {
            if event.keystroke.key == "enter" {
                confirm_modules(app, window, cx);
                cx.stop_propagation();
            }
        }))
        .on_key_up(cx.listener(|app, event: &gpui::KeyUpEvent, window, cx| {
            if event.keystroke.key == "space" {
                confirm_modules(app, window, cx);
                cx.stop_propagation();
            }
        }))
}

fn picked_key(id: &'static str) -> gpui::ElementId {
    (gpui::ElementId::from(id), "picked").into()
}

fn focus_of(
    app: &mut SettingsApp,
    id: &'static str,
    cx: &mut Context<SettingsApp>,
) -> gpui::FocusHandle {
    app.welcome
        .button_focus
        .entry(id)
        .or_insert_with(|| cx.focus_handle())
        .clone()
}

fn nav_row(
    back_id: &'static str,
    next_id: &'static str,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let back = action(
        back_id,
        "Back",
        false,
        false,
        app,
        window,
        cx,
        |app, _, window, cx| retreat(app, window, cx),
    );
    let next = action(
        next_id,
        "Next",
        true,
        false,
        app,
        window,
        cx,
        |app, _, window, cx| advance(app, window, cx),
    );
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(back)
        .child(next)
}

pub(super) fn volume2_step(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let import = action(
        "welcome-v2-import",
        "Import its settings",
        false,
        true,
        app,
        window,
        cx,
        |app, _, _, cx| import_volume2(app, cx),
    );
    let autostart = action(
        "welcome-v2-autostart",
        "Turn off its autostart",
        false,
        false,
        app,
        window,
        cx,
        |app, _, _, cx| disable_autostart(app, cx),
    );
    let close = action(
        "welcome-v2-close",
        "Close Volume²",
        false,
        false,
        app,
        window,
        cx,
        |app, _, _, cx| close_volume2(app, cx),
    );
    let nav = nav_row("welcome-v2-back", "welcome-v2-next", app, window, cx);
    let feedback = volume2_feedback(app, window, cx);
    div()
        .flex()
        .flex_col()
        .gap(px(22.))
        .child(block(
            app,
            80,
            heading(
                "Volume² is in the way",
                Some(
                    "It conflicts with silence!: two volume popups, and both apps react to the wheel and the volume keys. Import what you want, turn off its autostart, or skip this.",
                ),
                true,
                cx,
            ),
        ))
        .children(feedback.map(|feedback| block(app, 40, feedback)))
        .child(block(
            app,
            20,
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(10.))
                .child(import)
                .child(autostart)
                .child(close),
        ))
        .child(block(app, 0, nav))
}

fn volume2_feedback(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Option<Div> {
    let summary = summary_sentence(&app.welcome.volume2_summary);
    let notice = app.welcome.volume2_notice.clone();
    let notice_error = app.welcome.volume2_notice_error;
    let shown = !summary.is_empty() || !notice.is_empty();
    let reveal = anim::tween(
        window,
        cx,
        "welcome-v2-feedback",
        shown as u8 as f32,
        Transition::new(220, Easing::EaseInOut),
    );
    if reveal < 0.02 {
        return None;
    }
    let palette = app.palette;
    Some(
        div()
            .opacity(reveal)
            .child(
                div()
                    .p(px(12.))
                    .rounded(px(10.))
                    .bg(palette.surface_2.hsla())
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .text_size(px(13.))
                    .line_height(relative(1.4))
                    .when(!summary.is_empty(), |this| {
                        this.child(div().text_color(palette.text.hsla()).child(summary))
                    })
                    .when(!notice.is_empty(), |this| {
                        let color = if notice_error {
                            palette.danger
                        } else {
                            palette.text
                        };
                        this.child(div().text_color(color.hsla()).child(notice))
                    }),
            ),
    )
}

fn summary_sentence(lines: &[String]) -> String {
    lines.join(" · ")
}

fn show_notice(app: &mut SettingsApp, error: bool, text: impl Into<String>) {
    app.welcome.volume2_notice_error = error;
    app.welcome.volume2_notice = text.into();
}

fn import_volume2(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    if app.welcome.closing.is_some() {
        return;
    }
    let Some(path) = app
        .welcome
        .volume2_detection
        .as_ref()
        .and_then(|detection| detection.config_path.clone())
    else {
        show_notice(
            app,
            true,
            "Volume²'s settings file was not found. Nothing was changed.",
        );
        cx.notify();
        return;
    };
    let imported = match crate::volume2_import::read_volume2_config(&path) {
        Ok(settings) => settings,
        Err(_) => {
            show_notice(
                app,
                true,
                "Couldn't read Volume² settings. Nothing was changed.",
            );
            cx.notify();
            return;
        }
    };
    let mut summary = Vec::new();
    app.update_config(cx, |config| {
        summary = apply_volume2_settings(&mut config.volume, &imported);
    });
    app.welcome.volume2_summary = summary;
    show_notice(app, false, "");
    cx.notify();
}

fn disable_autostart(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    if app.welcome.closing.is_some() {
        return;
    }
    let Some(autostart) = app
        .welcome
        .volume2_detection
        .as_ref()
        .and_then(|detection| detection.autostart.clone())
    else {
        show_notice(app, false, "Volume² autostart was not found.");
        cx.notify();
        return;
    };
    use crate::volume2_import::Volume2ImportError;
    match crate::volume2_import::disable_volume2_autostart(&autostart) {
        Ok(_) => {
            show_notice(app, false, "Volume² will no longer start with Windows.");
            if let Some(detection) = app.welcome.volume2_detection.as_mut() {
                detection.autostart_locations.retain(|item| item != &autostart);
                detection.autostart = detection.autostart_locations.first().cloned();
            }
        }
        Err(Volume2ImportError::AdminRequired { .. }) => {
            show_notice(
                app,
                true,
                "silence! won't ask for administrator rights. Turn Volume² off in Task Manager → Startup apps, or in Volume²'s settings.",
            );
        }
        Err(Volume2ImportError::NotFound { .. }) => {
            show_notice(app, false, "That autostart entry is already gone.");
        }
        Err(Volume2ImportError::Failed { .. }) => {
            show_notice(
                app,
                true,
                "Couldn't turn off Volume² autostart. Disable it in Task Manager → Startup apps, or in Volume²'s settings.",
            );
        }
    }
    cx.notify();
}

fn close_volume2(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    if app.welcome.closing.is_some() {
        return;
    }
    let result = crate::volume2_import::stop_volume2();
    if result.not_running {
        show_notice(app, false, "Volume² is not running.");
    } else if result.failed_pids.is_empty() {
        show_notice(app, false, "Volume² was closed.");
        if let Some(detection) = app.welcome.volume2_detection.as_mut() {
            detection.running = false;
            detection.running_pids.clear();
            detection.running_elevated_or_unknown = false;
        }
    } else {
        show_notice(
            app,
            true,
            "silence! couldn't close Volume². It is probably running as administrator. Quit it from its tray icon, or end Volume².exe in Task Manager. silence! will not ask for administrator rights.",
        );
    }
    cx.notify();
}

/// Maps Volume² fields onto `config.volume`. Missing (`None`) fields are left alone.
fn apply_volume2_settings(
    volume: &mut crate::VolumeConfig,
    imported: &crate::volume2_import::Volume2ImportedSettings,
) -> Vec<String> {
    let mut lines = Vec::new();
    if !imported.wheel_enabled {
        // A saved scroll area plus scrolling disabled means the wheel was turned off.
        // A missing UseMouseScroll key also parses as false, so require a zone before
        // overwriting our default.
        if imported.wheel_zone.is_some() {
            volume.wheel.zone = crate::WheelZone::Off;
            lines.push("Wheel: off".into());
        }
    } else if let Some(zone) = imported.wheel_zone {
        match map_wheel_zone(zone) {
            Some(mapped) => {
                volume.wheel.zone = mapped;
                lines.push(format!("Wheel: {}", wheel_label(mapped)));
            }
            None => lines.push(
                "Wheel: kept the silence! default (Volume²'s area has no match)".into(),
            ),
        }
    }
    if let Some(step) = imported.step_percent {
        let step = step.clamp(1, 100);
        volume.wheel.step_percent = step;
        lines.push(format!("Wheel step: {step}%"));
    }
    let mut unmapped = false;
    unmapped |= apply_click(
        &mut volume.clicks.left,
        imported.left_click,
        "Left click",
        &mut lines,
    );
    unmapped |= apply_click(
        &mut volume.clicks.double,
        imported.double_click,
        "Double click",
        &mut lines,
    );
    unmapped |= apply_click(
        &mut volume.clicks.middle,
        imported.middle_click,
        "Middle click",
        &mut lines,
    );
    if imported.middle_click.is_none() && imported.middle_button_mutes {
        volume.clicks.middle = crate::VolumeClickAction::ToggleMute;
        lines.push("Middle click: mute".into());
    }
    if unmapped {
        lines.push(
            "Some click actions have no match in silence! yet, so those stay at the default."
                .into(),
        );
    }
    let osd_details = imported.osd_duration_ms.is_some() || imported.osd_position.is_some();
    if osd_details {
        volume.osd.enabled = imported.osd_enabled;
        if !imported.osd_enabled {
            lines.push("OSD: off".into());
        }
    } else if imported.osd_enabled {
        volume.osd.enabled = true;
        lines.push("OSD: on".into());
    }
    if let Some(ms) = imported.osd_duration_ms {
        let ms = ms.min(60_000);
        volume.osd.duration_ms = ms;
        lines.push(format!("OSD duration: {ms} ms"));
    }
    if let Some(position) = imported.osd_position {
        let x = (position.x_percent as f32).clamp(0., 100.);
        let y = (position.y_percent as f32).clamp(0., 100.);
        volume.osd.position_x = x;
        volume.osd.position_y = y;
        lines.push(format!("OSD position: {x:.0}%, {y:.0}%"));
    }
    if let Some(skin) = imported.tray_skin {
        let mapped = map_skin(skin);
        volume.tray_icon.skin = mapped;
        lines.push(format!("Tray icon: {}", skin_label(mapped)));
    }
    if lines.is_empty() {
        lines.push("Nothing mappable was found. silence! defaults were kept.".into());
    }
    lines
}

/// Volume² scroll areas we can represent. 0 is the notification icon, 2 is the
/// tray, 1 is the taskbar. Desktop, caption, full screen and the OSD window are not.
fn map_wheel_zone(zone: crate::volume2_import::ImportedWheelZone) -> Option<crate::WheelZone> {
    use crate::volume2_import::ImportedWheelZone;
    match zone {
        ImportedWheelZone::NotificationAreaIcon => Some(crate::WheelZone::TrayIcon),
        ImportedWheelZone::Tray => Some(crate::WheelZone::NotificationArea),
        ImportedWheelZone::Taskbar => Some(crate::WheelZone::Taskbar),
        ImportedWheelZone::Desktop
        | ImportedWheelZone::WindowCaption
        | ImportedWheelZone::AnywhereOnScreen
        | ImportedWheelZone::OsdWindow => None,
    }
}

fn map_click(action: crate::volume2_import::ImportedAction) -> Option<crate::VolumeClickAction> {
    use crate::volume2_import::ImportedAction;
    match action {
        ImportedAction::None => Some(crate::VolumeClickAction::None),
        ImportedAction::VolumePopup | ImportedAction::SystemVolumePopup => {
            Some(crate::VolumeClickAction::Popup)
        }
        ImportedAction::MuteToggle => Some(crate::VolumeClickAction::ToggleMute),
        ImportedAction::OpenMixer => Some(crate::VolumeClickAction::OpenMixer),
        ImportedAction::OpenAudioProperties => Some(crate::VolumeClickAction::OpenSoundSettings),
        ImportedAction::OpenSettings => Some(crate::VolumeClickAction::OpenSettings),
        ImportedAction::NextDefaultDevice => Some(crate::VolumeClickAction::NextOutputDevice),
        _ => None,
    }
}

fn map_skin(skin: crate::volume2_import::ImportedSkin) -> crate::VolumeTraySkin {
    match skin {
        crate::volume2_import::ImportedSkin::Rings => crate::VolumeTraySkin::Rings,
        crate::volume2_import::ImportedSkin::Digits => crate::VolumeTraySkin::Digits,
        crate::volume2_import::ImportedSkin::Speaker => crate::VolumeTraySkin::Speaker,
    }
}

fn apply_click(
    slot: &mut crate::VolumeClickAction,
    imported: Option<crate::volume2_import::ImportedAction>,
    label: &str,
    lines: &mut Vec<String>,
) -> bool {
    let Some(action) = imported else {
        return false;
    };
    match map_click(action) {
        Some(mapped) => {
            *slot = mapped;
            lines.push(format!("{label}: {}", click_label(mapped)));
            false
        }
        None => true,
    }
}

fn wheel_label(zone: crate::WheelZone) -> &'static str {
    match zone {
        crate::WheelZone::Off => "off",
        crate::WheelZone::TrayIcon => "over the volume icon",
        crate::WheelZone::NotificationArea => "over the notification area",
        crate::WheelZone::Taskbar => "anywhere on the taskbar",
    }
}

fn click_label(action: crate::VolumeClickAction) -> &'static str {
    match action {
        crate::VolumeClickAction::None => "nothing",
        crate::VolumeClickAction::Popup => "volume popup",
        crate::VolumeClickAction::ToggleMute => "mute",
        crate::VolumeClickAction::OpenMixer => "volume mixer",
        crate::VolumeClickAction::OpenSoundSettings => "sound settings",
        crate::VolumeClickAction::NextOutputDevice => "next output device",
        crate::VolumeClickAction::OpenSettings => "silence! settings",
    }
}

fn skin_label(skin: crate::VolumeTraySkin) -> &'static str {
    match skin {
        crate::VolumeTraySkin::Rings => "rings",
        crate::VolumeTraySkin::Digits => "digits",
        crate::VolumeTraySkin::Bar => "bar",
        crate::VolumeTraySkin::Speaker => "speaker",
    }
}

pub(super) fn wheel_step(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let selected = app.snapshot.config.volume.wheel.zone;
    let options = [
        (
            "welcome-wheel-off",
            crate::WheelZone::Off,
            "forbidden-circle-bold",
            "Off",
            "Scrolling leaves the volume alone.",
        ),
        (
            "welcome-wheel-icon",
            crate::WheelZone::TrayIcon,
            "volume-loud-bold",
            "Over the volume icon",
            "Only on the silence! volume icon.",
        ),
        (
            "welcome-wheel-area",
            crate::WheelZone::NotificationArea,
            "widget-bold",
            "Over the notification area",
            "Anywhere in the system tray.",
        ),
        (
            "welcome-wheel-taskbar",
            crate::WheelZone::Taskbar,
            "window-frame-bold",
            "Anywhere on the taskbar",
            "The whole bar, including the clock.",
        ),
    ];
    let mut cards = Vec::new();
    for (id, zone, icon_name, title, detail) in options {
        let picked = selected == zone;
        cards.push(choice_card(
            id,
            picked,
            title,
            detail,
            CardArt::Icon(icon_name),
            118.,
            app,
            window,
            cx,
            move |app, _, cx| {
                app.update_config(cx, |config| config.volume.wheel.zone = zone);
            },
        ));
    }
    div()
        .flex()
        .flex_col()
        .gap(px(22.))
        .child(block(
            app,
            80,
            heading(
                "Where the wheel changes volume",
                Some("The default is over the volume icon."),
                false,
                cx,
            ),
        ))
        .child(block(
            app,
            40,
            div().flex().flex_wrap().gap(px(12.)).children(cards),
        ))
        .child({
            let row = nav_row("welcome-wheel-back", "welcome-wheel-next", app, window, cx);
            block(app, 0, row)
        })
}

pub(super) fn skin_step(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let selected = app.snapshot.config.volume.tray_icon.skin;
    let options = [
        ("welcome-skin-rings", crate::VolumeTraySkin::Rings, "Rings"),
        ("welcome-skin-digits", crate::VolumeTraySkin::Digits, "Digits"),
        ("welcome-skin-bar", crate::VolumeTraySkin::Bar, "Bar"),
        (
            "welcome-skin-speaker",
            crate::VolumeTraySkin::Speaker,
            "Speaker",
        ),
    ];
    let mut cards = Vec::new();
    for (id, skin, title) in options {
        cards.push(choice_card(
            id,
            selected == skin,
            title,
            "",
            CardArt::Skin(skin),
            132.,
            app,
            window,
            cx,
            move |app, _, cx| {
                app.update_config(cx, |config| config.volume.tray_icon.skin = skin);
            },
        ));
    }
    div()
        .flex()
        .flex_col()
        .gap(px(22.))
        .child(block(
            app,
            80,
            heading(
                "Tray icon",
                Some("How volume shows up next to the clock."),
                false,
                cx,
            ),
        ))
        .child(block(
            app,
            40,
            div().flex().flex_wrap().gap(px(12.)).children(cards),
        ))
        .child({
            let row = nav_row("welcome-skin-back", "welcome-skin-next", app, window, cx);
            block(app, 0, row)
        })
}

pub(super) fn media_step(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let palette = app.palette;
    let checked = app.snapshot.config.volume.media_keys.intercept;
    let toggle = controls::toggle(
        "welcome-media-keys",
        checked,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| config.volume.media_keys.intercept = checked);
        },
    );
    div()
        .flex()
        .flex_col()
        .gap(px(22.))
        .child(block(
            app,
            80,
            heading(
                "Keyboard volume keys",
                Some("silence! can handle the volume keys so Windows does not pop its own OSD."),
                false,
                cx,
            ),
        ))
        .child(block(
            app,
            40,
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.))
                .p(px(16.))
                .rounded(px(12.))
                .bg(palette.surface_2.hsla())
                .shadow(vec![BoxShadow {
                    color: palette.shadow_soft.hsla(),
                    offset: point(px(0.), px(12.)),
                    blur_radius: px(28.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .min_w_0()
                        .child(
                            div()
                                .font_family("Bricolage Grotesque Welcome Card")
                                .font_weight(FontWeight(600.))
                                .text_size(px(18.))
                                .child("Use silence! for keyboard volume keys"),
                        )
                        .child(
                            div()
                                .text_color(palette.muted.hsla())
                                .text_size(px(13.))
                                .line_height(relative(1.4))
                                .child("On by default. Turn it off to leave the keys to Windows."),
                        ),
                )
                .child(toggle),
        ))
        .child({
            let row = nav_row("welcome-media-back", "welcome-media-next", app, window, cx);
            block(app, 0, row)
        })
}

enum CardArt {
    Icon(&'static str),
    Skin(crate::VolumeTraySkin),
}

fn choice_card(
    id: &'static str,
    selected: bool,
    title: &'static str,
    detail: &'static str,
    art: CardArt,
    min_h: f32,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_pick: impl Fn(&mut SettingsApp, &mut Window, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    let palette = app.palette;
    let (pointer, hover, press) = controls::hover_tween(id, window, cx);
    let selected_t = anim::tween(window, cx, picked_key(id), selected as u8 as f32, STEP_FADE);
    let bg = lerp(
        lerp(palette.surface_2, palette.surface_hover, hover * (1. - selected_t * 0.65)),
        palette.accent_soft,
        selected_t,
    );
    let scale = 1. + 0.04 * selected_t - 0.06 * press;
    let focus = focus_of(app, id, cx);
    let focus_color = palette.focus.hsla();
    let mouse_focus = focus.clone();
    let on_pick = Rc::new(on_pick);
    let click = on_pick.clone();
    let enter = on_pick.clone();
    let space = on_pick;
    let art: AnyElement = match art {
        CardArt::Icon(name) => svg()
            .path(icon(name))
            .size(px(28.))
            .text_color(lerp(palette.muted, palette.accent, selected_t).hsla())
            .with_transformation(Transformation::scale(size(scale, scale)))
            .into_any_element(),
        CardArt::Skin(skin) => skin_preview(skin, palette.accent).into_any_element(),
    };
    let element = div()
        .id(id)
        .relative()
        .w(px(324.))
        .min_h(px(min_h))
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(px(8.))
        .p(px(14.))
        .rounded(px(12.))
        .bg(bg.hsla())
        .cursor_pointer()
        .shadow(vec![
            BoxShadow {
                color: palette.shadow_soft.hsla(),
                offset: point(px(0.), px(8.)),
                blur_radius: px(18.),
                spread_radius: px(0.),
                inset: false,
            },
            BoxShadow {
                color: palette.accent.opacity(selected_t).hsla(),
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(2.),
                inset: true,
            },
        ])
        .track_focus(&focus)
        .focus_visible(move |style| {
            style.shadow(vec![BoxShadow {
                color: focus_color,
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(2.),
                inset: true,
            }])
        })
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            mouse_focus.focus(window, cx)
        })
        .child(
            div().absolute().top(px(10.)).right(px(10.)).child(
                svg()
                    .path(icon("check-circle-bold"))
                    .size(px(18.))
                    .opacity(selected_t)
                    .text_color(palette.accent.hsla())
                    .with_transformation(Transformation::scale(size(
                        0.4 + 0.6 * selected_t,
                        0.4 + 0.6 * selected_t,
                    ))),
            ),
        )
        .child(div().relative().top(px(press)).child(art))
        .child(
            div()
                .font_family("Bricolage Grotesque Welcome Card")
                .font_weight(FontWeight(600.))
                .text_size(px(16.))
                .line_height(relative(1.15))
                .child(title),
        )
        .when(!detail.is_empty(), |this| {
            this.child(
                div()
                    .text_color(palette.muted.hsla())
                    .text_size(px(13.))
                    .line_height(relative(1.35))
                    .child(detail),
            )
        });
    let element = controls::track_pointer(element, pointer);
    if app.welcome.closing.is_some() {
        return element.cursor_default();
    }
    element
        .on_click(cx.listener(move |app, _, window, cx| click(app, window, cx)))
        .on_key_down(
            cx.listener(move |app, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "enter" && app.welcome.closing.is_none() {
                    enter(app, window, cx);
                    cx.stop_propagation();
                }
            }),
        )
        .on_key_up(
            cx.listener(move |app, event: &gpui::KeyUpEvent, window, cx| {
                if event.keystroke.key == "space" && app.welcome.closing.is_none() {
                    space(app, window, cx);
                    cx.stop_propagation();
                }
            }),
        )
}

/// Placeholder art until the tray renderer can draw the real icon.
/// TODO(lead): render with render_volume_icon_rgba
fn skin_preview(skin: crate::VolumeTraySkin, accent: Oklch) -> Div {
    let color = accent.hsla();
    match skin {
        crate::VolumeTraySkin::Rings => div()
            .h(px(40.))
            .flex()
            .items_center()
            .child(
                div()
                    .size(px(36.))
                    .rounded_full()
                    .border_1()
                    .border_color(color)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .size(px(20.))
                            .rounded_full()
                            .border_1()
                            .border_color(color)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(div().size(px(6.)).rounded_full().bg(color)),
                    ),
            ),
        crate::VolumeTraySkin::Digits => div()
            .h(px(40.))
            .flex()
            .items_center()
            .text_size(px(28.))
            .font_weight(FontWeight(800.))
            .text_color(color)
            .child("42"),
        crate::VolumeTraySkin::Bar => div().h(px(40.)).flex().items_end().gap(px(4.)).children(
            [12_f32, 26., 18., 34.].into_iter().map(|height| {
                div()
                    .w(px(7.))
                    .h(px(height))
                    .rounded(px(2.))
                    .bg(color)
            }),
        ),
        crate::VolumeTraySkin::Speaker => div().h(px(40.)).flex().items_center().child(
            svg()
                .path(icon("volume-loud-bold"))
                .size(px(32.))
                .text_color(color),
        ),
    }
}
