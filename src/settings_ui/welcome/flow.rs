//! Dynamic welcome steps: module choice, then microphone and/or volume.
//!
//! Volume² click indexes are still being checked, so import uses only the
//! high-level fields. `None` keeps the silence! default. Hotkeys are not imported.
use super::{SettingsApp, action, block, heading};
use crate::settings_ui::{
    anim::{self, Easing, Transition},
    assets::icon,
    color::{TRANSPARENT, lerp},
    controls,
    sections::{form, volume::tray_icon::skin_preview},
    theme::Palette,
};
use gpui::{
    BoxShadow, Context, Div, FontWeight, Hsla, SharedString, Stateful, Transformation, Window,
    div, point, prelude::*, px, relative, size, svg,
};
use std::rc::Rc;

const STEP_FADE: Transition = Transition::new(220, Easing::Bezier(0.22, 1., 0.36, 1.));
/// Modules, hotkey, Volume², wheel, look, done.
const MAX_DOTS: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StepKind {
    Modules,
    MicHotkey,
    Volume2,
    Wheel,
    Look,
    Done,
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

/// Tab order of the current step. Volume² actions that are already done have
/// no button left to focus.
pub(super) fn focus_ids(app: &SettingsApp) -> Vec<&'static str> {
    match current(app) {
        StepKind::Modules => vec![
            "welcome-pick-microphone",
            "welcome-pick-volume",
            "welcome-modules-next",
        ],
        StepKind::MicHotkey => Vec::new(),
        StepKind::Volume2 => {
            let mut ids = Vec::new();
            if !volume2_imported(app) {
                ids.push("welcome-v2-import");
            }
            if volume2_autostarts(app) {
                ids.push("welcome-v2-autostart");
            }
            if volume2_running(app) {
                ids.push("welcome-v2-close");
            }
            ids.extend(["welcome-v2-back", "welcome-v2-next"]);
            ids
        }
        StepKind::Wheel => vec![
            "welcome-wheel-off",
            "welcome-wheel-icon",
            "welcome-wheel-area",
            "welcome-wheel-taskbar",
            "welcome-wheel-back",
            "welcome-wheel-next",
        ],
        StepKind::Look => vec![
            "welcome-skin-rings",
            "welcome-skin-digits",
            "welcome-skin-bar",
            "welcome-skin-speaker",
            "welcome-look-back",
            "welcome-look-next",
        ],
        StepKind::Done => vec!["welcome-done-back", "welcome-finish"],
    }
}

fn plan(app: &SettingsApp) -> Vec<StepKind> {
    let mut steps = vec![StepKind::Modules];
    if app.welcome.pick_microphone {
        steps.push(StepKind::MicHotkey);
    }
    if app.welcome.pick_volume {
        if volume2_in_flow(app) {
            steps.push(StepKind::Volume2);
        }
        steps.extend([StepKind::Wheel, StepKind::Look]);
    }
    steps.push(StepKind::Done);
    steps
}

/// The tiles start from the saved modules: a fresh install has the microphone on.
pub(super) fn init(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    let modules = app.snapshot.config.modules.clone();
    app.welcome.pick_microphone = modules.microphone;
    app.welcome.pick_volume = modules.volume;
    if modules.volume {
        probe_volume2(app, cx);
    }
}

/// Debug only (`--welcome-preview=<step>`): opens the flow at `step` without
/// walking there. `done-mic` and `done-volume` show the single-module endings.
pub(super) fn preview(app: &mut SettingsApp, step: &str) {
    let (kind, microphone, volume) = match step {
        "hotkey" => (StepKind::MicHotkey, true, true),
        "volume2" => (StepKind::Volume2, true, true),
        "wheel" => (StepKind::Wheel, true, true),
        "look" => (StepKind::Look, true, true),
        "done" => (StepKind::Done, true, true),
        "done-mic" => (StepKind::Done, true, false),
        "done-volume" => (StepKind::Done, false, true),
        _ => return,
    };
    app.welcome.pick_microphone = microphone;
    app.welcome.pick_volume = volume;
    app.welcome.volume2_included = kind == StepKind::Volume2;
    // Past the module step the plan no longer follows the live Volume² probe.
    app.welcome.step = 1;
    app.welcome.step = plan(app).iter().position(|step| *step == kind).unwrap_or(0);
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
        "Mute your mic from anywhere.",
        [
            ("keyboard-linear", "Global hotkey"),
            ("monitor-linear", "On-screen overlay"),
            ("bell-linear", "Sound feedback"),
        ],
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
        "Change volume without opening anything.",
        [
            ("mouse-linear", "Scroll wheel"),
            ("slider-minimalistic-horizontal-linear", "Tray icon with sliders"),
            ("keyboard-linear", "Volume keys"),
        ],
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
                "Welcome to silence!",
                Some("Pick what it should handle. You can change this later in Settings."),
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
            // A row, so the button hugs its label like Back/Next on later steps.
            block(app, 0, div().flex().child(next))
        })
}

fn module_tile(
    id: &'static str,
    icon_name: &'static str,
    title: &'static str,
    detail: &'static str,
    features: [(&'static str, &'static str); 3],
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
    let feature_ink = lerp(palette.muted, palette.accent, selected_t).hsla();
    let feature_text = lerp(palette.muted, palette.text, selected_t).hsla();
    let element = div()
        .id(id)
        .relative()
        .flex_1()
        .min_w_0()
        .h(px(222.))
        .flex_shrink_0()
        .p(px(16.))
        .rounded(px(12.))
        .bg(bg.hsla())
        .shadow(vec![
            card_shadow(&palette),
            ring(palette.accent.opacity(selected_t).hsla()),
        ])
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
                .child(
                    svg()
                        .path(icon(icon_name))
                        .size(px(40.))
                        .text_color(lerp(palette.accent, palette.accent_strong, selected_t).hsla())
                        .with_transformation(Transformation::scale(size(icon_scale, icon_scale))),
                )
                .child(
                    div()
                        .mt(px(8.))
                        .font_family("Bricolage Grotesque Welcome Card")
                        .font_weight(FontWeight(600.))
                        .text_size(px(22.))
                        .line_height(relative(1.1))
                        .child(title),
                )
                .child(
                    div()
                        .mt(px(6.))
                        .text_color(palette.muted.hsla())
                        .text_size(px(13.))
                        .line_height(relative(1.4))
                        .child(detail),
                )
                .child(
                    div()
                        .mt(px(14.))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .children(features.into_iter().map(|(name, label)| {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .h(px(18.))
                                .child(
                                    svg()
                                        .path(icon(name))
                                        .size(px(16.))
                                        .flex_shrink_0()
                                        .text_color(feature_ink),
                                )
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight(560.))
                                        .text_color(feature_text)
                                        .child(label),
                                )
                        })),
                ),
        );
    pickable(
        controls::track_pointer(element, pointer),
        id,
        app,
        cx,
        move |app, cx| toggle_pick(app, microphone, cx),
    )
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

fn volume2_imported(app: &SettingsApp) -> bool {
    !app.welcome.volume2_summary.is_empty()
}

fn volume2_autostarts(app: &SettingsApp) -> bool {
    app.welcome
        .volume2_detection
        .as_ref()
        .is_some_and(|detection| detection.autostart.is_some())
}

fn volume2_running(app: &SettingsApp) -> bool {
    app.welcome
        .volume2_detection
        .as_ref()
        .is_some_and(|detection| detection.running)
}

pub(super) fn volume2_step(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let palette = app.palette;
    let running = volume2_running(app);
    let import = volume2_row(
        Volume2Row {
            index: 0,
            id: "welcome-v2-import",
            note_id: "welcome-v2-import-note",
            icon: "import-linear",
            title: "Bring your settings over",
            button: "Import",
            done_label: "Imported",
            done: volume2_imported(app),
        },
        app,
        window,
        cx,
        import_volume2,
    );
    let autostart = volume2_row(
        Volume2Row {
            index: 1,
            id: "welcome-v2-autostart",
            note_id: "welcome-v2-autostart-note",
            icon: "bolt-linear",
            title: "Stop it starting with Windows",
            button: "Turn off",
            done_label: "Off",
            done: !volume2_autostarts(app),
        },
        app,
        window,
        cx,
        disable_autostart,
    );
    let close = volume2_row(
        Volume2Row {
            index: 2,
            id: "welcome-v2-close",
            note_id: "welcome-v2-close-note",
            icon: "close-circle-linear",
            title: "Close it now",
            button: "Close",
            done_label: "Not running",
            done: !running,
        },
        app,
        window,
        cx,
        close_volume2,
    );
    let rule = || div().w_full().h(px(1.)).bg(palette.border.hsla());
    let nav = nav_row("welcome-v2-back", "welcome-v2-next", app, window, cx);
    div()
        .flex()
        .flex_col()
        .gap(px(22.))
        .child(block(
            app,
            80,
            heading(
                if running {
                    "Volume² is also running"
                } else {
                    "Volume² is still installed"
                },
                Some(
                    "Two volume apps fight over the wheel and the volume keys. Sort it out here, or skip.",
                ),
                true,
                cx,
            ),
        ))
        .child(block(
            app,
            40,
            div()
                .flex()
                .flex_col()
                .rounded(px(12.))
                .bg(palette.surface_2.hsla())
                .shadow(vec![card_shadow(&palette)])
                .child(import)
                .child(rule())
                .child(autostart)
                .child(rule())
                .child(close),
        ))
        .child(block(app, 0, nav))
}

struct Volume2Row {
    /// Slot in `volume2_notes`.
    index: usize,
    id: &'static str,
    note_id: &'static str,
    icon: &'static str,
    title: &'static str,
    button: &'static str,
    done_label: &'static str,
    done: bool,
}

/// One Volume² action. Its result stays in the row: the button gives way to a
/// check mark, and a note slides open under the title.
fn volume2_row(
    row: Volume2Row,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_click: fn(&mut SettingsApp, &mut Context<SettingsApp>),
) -> Div {
    let palette = app.palette;
    let note = app.welcome.volume2_notes[row.index].clone();
    let error = app.welcome.volume2_note_errors[row.index];
    let done = anim::tween(
        window,
        cx,
        (gpui::ElementId::from(row.id), "done"),
        row.done as u8 as f32,
        STEP_FADE,
    );
    let trailing = if row.done {
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .flex_shrink_0()
            .opacity(done)
            .child(
                svg()
                    .path(icon("check-circle-bold"))
                    .size(px(18.))
                    .text_color(palette.success.hsla()),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight(600.))
                    .text_color(palette.success.hsla())
                    .child(row.done_label),
            )
            .into_any_element()
    } else {
        action(
            row.id,
            row.button,
            false,
            false,
            app,
            window,
            cx,
            move |app, _, _, cx| on_click(app, cx),
        )
        .h(px(34.))
        .px(px(14.))
        .into_any_element()
    };
    let note_line = form::collapse(
        row.note_id,
        !note.is_empty(),
        div()
            .pt(px(3.))
            .text_size(px(13.))
            .line_height(relative(1.4))
            .text_color(if error { palette.danger } else { palette.muted }.hsla())
            .child(note),
        window,
        cx,
    );
    div()
        .flex()
        .items_center()
        .gap(px(14.))
        .min_h(px(58.))
        .px(px(16.))
        .py(px(10.))
        .child(
            svg()
                .path(icon(row.icon))
                .size(px(20.))
                .flex_shrink_0()
                .text_color(lerp(palette.muted, palette.success, done).hsla()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight(600.))
                        .child(row.title),
                )
                .child(note_line),
        )
        .child(trailing)
}

fn show_notice(app: &mut SettingsApp, index: usize, error: bool, text: impl Into<String>) {
    app.welcome.volume2_note_errors[index] = error;
    app.welcome.volume2_notes[index] = text.into();
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
            0,
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
                0,
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
    show_notice(app, 0, false, summary.join(" · "));
    app.welcome.volume2_summary = summary;
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
        return;
    };
    use crate::volume2_import::Volume2ImportError;
    let mut forget = false;
    match crate::volume2_import::disable_volume2_autostart(&autostart) {
        Ok(_) => {
            show_notice(app, 1, false, "Volume² will no longer start with Windows.");
            forget = true;
        }
        Err(Volume2ImportError::AdminRequired { .. }) => {
            show_notice(
                app,
                1,
                true,
                "silence! won't ask for administrator rights. Turn Volume² off in Task Manager → Startup apps, or in Volume²'s settings.",
            );
        }
        Err(Volume2ImportError::NotFound { .. }) => {
            show_notice(app, 1, false, "That autostart entry is already gone.");
            forget = true;
        }
        Err(Volume2ImportError::Failed { .. }) => {
            show_notice(
                app,
                1,
                true,
                "Couldn't turn off Volume² autostart. Disable it in Task Manager → Startup apps, or in Volume²'s settings.",
            );
        }
    }
    if forget && let Some(detection) = app.welcome.volume2_detection.as_mut() {
        detection.autostart_locations.retain(|item| item != &autostart);
        detection.autostart = detection.autostart_locations.first().cloned();
    }
    cx.notify();
}

fn close_volume2(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    if app.welcome.closing.is_some() {
        return;
    }
    let result = crate::volume2_import::stop_volume2();
    if result.not_running || result.failed_pids.is_empty() {
        show_notice(
            app,
            2,
            false,
            if result.not_running {
                "Volume² had already quit."
            } else {
                "Volume² was closed."
            },
        );
        if let Some(detection) = app.welcome.volume2_detection.as_mut() {
            detection.running = false;
            detection.running_pids.clear();
            detection.running_elevated_or_unknown = false;
        }
    } else {
        show_notice(
            app,
            2,
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

fn card_shadow(palette: &Palette) -> BoxShadow {
    BoxShadow {
        color: palette.shadow_soft.hsla(),
        offset: point(px(0.), px(12.)),
        blur_radius: px(28.),
        spread_radius: px(0.),
        inset: false,
    }
}

/// A 2px line just inside the element's edge: selection and keyboard focus.
fn ring(color: Hsla) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(2.),
        inset: true,
    }
}

/// Keyboard focus, click, Enter and Space for a tile, segment or swatch.
fn pickable(
    element: Stateful<Div>,
    id: &'static str,
    app: &mut SettingsApp,
    cx: &mut Context<SettingsApp>,
    on_pick: impl Fn(&mut SettingsApp, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    let focus = focus_of(app, id, cx);
    let focus_color = app.palette.focus.hsla();
    let mouse_focus = focus.clone();
    let element = element
        .track_focus(&focus)
        .focus_visible(move |style| style.shadow(vec![ring(focus_color)]))
        .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
            mouse_focus.focus(window, cx)
        });
    if app.welcome.closing.is_some() {
        return element.cursor_default();
    }
    let on_pick = Rc::new(on_pick);
    let enter = on_pick.clone();
    let space = on_pick.clone();
    element
        .cursor_pointer()
        .on_click(cx.listener(move |app, _, _, cx| on_pick(app, cx)))
        .on_key_down(cx.listener(move |app, event: &gpui::KeyDownEvent, _, cx| {
            if event.keystroke.key == "enter" && app.welcome.closing.is_none() {
                enter(app, cx);
                cx.stop_propagation();
            }
        }))
        .on_key_up(cx.listener(move |app, event: &gpui::KeyUpEvent, _, cx| {
            if event.keystroke.key == "space" && app.welcome.closing.is_none() {
                space(app, cx);
                cx.stop_propagation();
            }
        }))
}

pub(super) fn wheel_step(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let palette = app.palette;
    let selected = app.snapshot.config.volume.wheel.zone;
    let options = [
        (
            "welcome-wheel-off",
            crate::WheelZone::Off,
            "forbidden-circle-linear",
            "Off",
        ),
        (
            "welcome-wheel-icon",
            crate::WheelZone::TrayIcon,
            "volume-loud-linear",
            "Volume icon",
        ),
        (
            "welcome-wheel-area",
            crate::WheelZone::NotificationArea,
            "widget-linear",
            "Whole tray",
        ),
        (
            "welcome-wheel-taskbar",
            crate::WheelZone::Taskbar,
            "window-frame-linear",
            "Whole taskbar",
        ),
    ];
    let mut segments = div()
        .flex()
        .gap(px(4.))
        .p(px(4.))
        .rounded(px(14.))
        .bg(palette.surface_2.hsla());
    for (id, zone, icon_name, label) in options {
        segments = segments.child(segment(
            id,
            icon_name,
            label,
            selected == zone,
            app,
            window,
            cx,
            move |app, cx| {
                app.update_config(cx, |config| config.volume.wheel.zone = zone);
            },
        ));
    }
    let detail = match selected {
        crate::WheelZone::Off => "Scrolling leaves the volume alone.",
        crate::WheelZone::TrayIcon => "Only over the silence! volume icon.",
        crate::WheelZone::NotificationArea => "Anywhere over the tray icons and the clock.",
        crate::WheelZone::Taskbar => "The whole bar, including the clock.",
    };
    let art = taskbar_art(selected, &palette, window, cx);
    div()
        .flex()
        .flex_col()
        .gap(px(22.))
        .child(block(
            app,
            80,
            heading(
                "Scroll to change volume",
                Some("Choose where the wheel works."),
                false,
                cx,
            ),
        ))
        .child(block(
            app,
            40,
            div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(art)
                .child(segments)
                .child(
                    div()
                        .px(px(4.))
                        .text_size(px(13.))
                        .text_color(palette.muted.hsla())
                        .child(detail),
                ),
        ))
        .child({
            let row = nav_row("welcome-wheel-back", "welcome-wheel-next", app, window, cx);
            block(app, 0, row)
        })
}

fn segment(
    id: &'static str,
    icon_name: &'static str,
    label: &'static str,
    selected: bool,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_pick: impl Fn(&mut SettingsApp, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    let palette = app.palette;
    let (pointer, hover, press) = controls::hover_tween(id, window, cx);
    let on = anim::tween(window, cx, picked_key(id), selected as u8 as f32, STEP_FADE);
    let bg = lerp(
        lerp(TRANSPARENT, palette.surface_hover, hover),
        palette.accent_soft,
        on,
    );
    let element = div()
        .id(id)
        .flex_1()
        .min_w_0()
        .h(px(40.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.))
        .bg(bg.hsla())
        .text_size(px(13.))
        .font_weight(FontWeight(650.))
        .text_color(lerp(palette.muted, palette.text, on).hsla())
        .child(
            div()
                .relative()
                .top(px(press))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    svg()
                        .path(icon(icon_name))
                        .size(px(16.))
                        .flex_shrink_0()
                        .text_color(lerp(palette.muted, palette.accent, on).hsla()),
                )
                .child(label),
        );
    pickable(controls::track_pointer(element, pointer), id, app, cx, on_pick)
}

/// A drawn taskbar. The highlight slides and resizes to the wheel zone.
fn taskbar_art(
    zone: crate::WheelZone,
    palette: &Palette,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    // The welcome column is 680px wide, so the bar is laid out in fixed pixels.
    let (x, y, w, h) = match zone {
        crate::WheelZone::Off | crate::WheelZone::TrayIcon => (566., 13., 30., 30.),
        // The tray icons and the clock.
        crate::WheelZone::NotificationArea => (478., 9., 193., 38.),
        crate::WheelZone::Taskbar => (3., 3., 674., 50.),
    };
    let on = (zone != crate::WheelZone::Off) as u8 as f32;
    let x = anim::tween(window, cx, "welcome-wheel-zone-x", x, STEP_FADE);
    let y = anim::tween(window, cx, "welcome-wheel-zone-y", y, STEP_FADE);
    let w = anim::tween(window, cx, "welcome-wheel-zone-w", w, STEP_FADE);
    let h = anim::tween(window, cx, "welcome-wheel-zone-h", h, STEP_FADE);
    let on = anim::tween(window, cx, "welcome-wheel-zone-on", on, STEP_FADE);
    let quiet = palette.muted.opacity(0.7).hsla();
    let glyph = |name: &'static str, left: f32, color: Hsla| {
        svg()
            .path(icon(name))
            .absolute()
            .left(px(left))
            .top(px(19.))
            .size(px(18.))
            .text_color(color)
    };
    div()
        .relative()
        .w_full()
        .h(px(56.))
        .rounded(px(12.))
        .bg(gpui::black().opacity(0.32))
        .child(
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w))
                .h(px(h))
                .rounded(px(9.))
                .opacity(on)
                .bg(palette.accent_soft.hsla())
                .shadow(vec![ring(palette.accent.hsla())]),
        )
        .child(
            svg()
                .path(icon("microsoft-windows"))
                .absolute()
                .left(px(18.))
                .top(px(18.))
                .size(px(20.))
                .text_color(quiet),
        )
        .children([54., 88., 122.].map(|left| {
            div()
                .absolute()
                .left(px(left))
                .top(px(17.))
                .size(px(22.))
                .rounded(px(6.))
                .bg(palette.surface_3.hsla())
        }))
        .child(glyph("shield-check-linear", 486., quiet))
        .child(glyph("headphones-round-sound-linear", 514., quiet))
        .child(glyph("mic", 542., quiet))
        .child(glyph("volume-loud-bold", 572., palette.accent.hsla()))
        .child(
            div()
                .absolute()
                .right(px(18.))
                .top_0()
                .h_full()
                .flex()
                .items_center()
                .text_size(px(12.))
                .font_weight(FontWeight(600.))
                .text_color(quiet)
                .child("12:40"),
        )
}

pub(super) fn look_step(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let palette = app.palette;
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
    let mut skins = div().flex().gap(px(10.));
    for (id, skin, label) in options {
        skins = skins.child(skin_swatch(id, skin, label, selected == skin, app, window, cx));
    }
    let toggle = controls::toggle(
        "welcome-media-keys",
        app.snapshot.config.volume.media_keys.intercept,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| config.volume.media_keys.intercept = checked);
        },
    );
    let keys = div()
        .flex()
        .items_center()
        .gap(px(14.))
        .p(px(16.))
        .rounded(px(12.))
        .bg(palette.surface_2.hsla())
        .shadow(vec![card_shadow(&palette)])
        .child(
            svg()
                .path(icon("keyboard-bold"))
                .size(px(24.))
                .flex_shrink_0()
                .text_color(palette.accent.hsla()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight(600.))
                        .child("Handle keyboard volume keys"),
                )
                .child(
                    div()
                        .text_color(palette.muted.hsla())
                        .text_size(px(13.))
                        .line_height(relative(1.4))
                        .child("Shows the silence! volume popup instead of the Windows one."),
                ),
        )
        .child(toggle);
    div()
        .flex()
        .flex_col()
        .gap(px(22.))
        .child(block(
            app,
            80,
            heading(
                "Tray icon and keys",
                Some("How volume looks next to the clock, and who handles the volume keys."),
                false,
                cx,
            ),
        ))
        .child(block(
            app,
            40,
            div().flex().flex_col().gap(px(12.)).child(skins).child(keys),
        ))
        .child({
            let row = nav_row("welcome-look-back", "welcome-look-next", app, window, cx);
            block(app, 0, row)
        })
}

fn skin_swatch(
    id: &'static str,
    skin: crate::VolumeTraySkin,
    label: &'static str,
    selected: bool,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Stateful<Div> {
    let palette = app.palette;
    let (pointer, hover, press) = controls::hover_tween(id, window, cx);
    let on = anim::tween(window, cx, picked_key(id), selected as u8 as f32, STEP_FADE);
    let bg = lerp(
        lerp(palette.surface_2, palette.surface_hover, hover * (1. - on * 0.65)),
        palette.accent_soft,
        on,
    );
    let ink = lerp(palette.muted, palette.accent, on).hsla();
    let element = div()
        .id(id)
        .flex_1()
        .min_w_0()
        .h(px(92.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(10.))
        .rounded(px(12.))
        .bg(bg.hsla())
        .shadow(vec![
            card_shadow(&palette),
            ring(palette.accent.opacity(on).hsla()),
        ])
        .child(
            div()
                .relative()
                .top(px(press))
                .h(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .child(skin_preview(skin, 0.66, false, ink, palette.danger.hsla())),
        )
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight(650.))
                .text_color(lerp(palette.muted, palette.text, on).hsla())
                .child(label),
        );
    pickable(
        controls::track_pointer(element, pointer),
        id,
        app,
        cx,
        move |app, cx| {
            app.update_config(cx, |config| config.volume.tray_icon.skin = skin);
        },
    )
}

pub(super) fn done_step(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let palette = app.palette;
    let microphone = app.welcome.pick_microphone;
    let volume = app.welcome.pick_volume;
    let (title, more, finish_label) = match (microphone, volume) {
        (true, true) => (
            "You're all set",
            "More in Settings: extra hotkeys, sounds, overlay looks, devices, gamepad.",
            "Let's go",
        ),
        (false, true) => (
            "Ready to scroll",
            "More in Settings: wheel step, click actions, slider popup, music controls.",
            "Start scrolling",
        ),
        _ => (
            "Ready to mute",
            "More in Settings: extra hotkeys, custom sounds, overlay looks, devices, gamepad.",
            "Start muting",
        ),
    };
    let mut recap = div()
        .flex()
        .flex_col()
        .rounded(px(12.))
        .bg(palette.surface_2.hsla())
        .shadow(vec![card_shadow(&palette)]);
    if microphone {
        recap = recap.child(mute_row(app, window, cx));
    }
    if microphone && volume {
        recap = recap.child(div().w_full().h(px(1.)).bg(palette.border.hsla()));
    }
    if volume {
        recap = recap.child(volume_row(app));
    }
    let back = action(
        "welcome-done-back",
        "Back",
        false,
        false,
        app,
        window,
        cx,
        |app, _, window, cx| {
            app.welcome.completed = false;
            app.welcome.progress = 0.;
            retreat(app, window, cx);
        },
    );
    let next = action(
        "welcome-finish",
        finish_label,
        true,
        false,
        app,
        window,
        cx,
        |app, _, window, cx| advance(app, window, cx),
    );
    div()
        .flex()
        .flex_col()
        .gap(px(22.))
        .child(block(
            app,
            80,
            heading(
                title,
                Some("silence! lives in the tray. Close this window and it keeps working."),
                true,
                cx,
            ),
        ))
        .child(block(app, 40, recap))
        .child(block(
            app,
            20,
            div()
                .text_size(px(13.))
                .text_color(palette.muted.hsla())
                .child(more),
        ))
        .child(block(
            app,
            0,
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(back)
                .child(next),
        ))
}

fn recap_row(
    icon_name: &'static str,
    title: &'static str,
    detail: SharedString,
    trailing: impl IntoElement,
    palette: &Palette,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(14.))
        .min_h(px(64.))
        .px(px(16.))
        .py(px(10.))
        .child(
            svg()
                .path(icon(icon_name))
                .size(px(24.))
                .flex_shrink_0()
                .text_color(palette.accent.hsla()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight(600.))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(palette.muted.hsla())
                        .child(detail),
                ),
        )
        .child(trailing)
}

/// The chosen hotkey and the live mic state. Hotkeys are on while this screen
/// is open (`welcome_hotkey_trial`), so pressing it flips the chip.
fn mute_row(app: &mut SettingsApp, window: &mut Window, cx: &mut Context<SettingsApp>) -> Div {
    let palette = app.palette;
    let muted = app.snapshot.muted;
    let state = anim::tween(
        window,
        cx,
        "welcome-done-muted",
        muted as u8 as f32,
        STEP_FADE,
    );
    let ink = lerp(palette.success, palette.danger, state);
    let chip = div()
        .w(px(112.))
        .h(px(28.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .rounded_full()
        .bg(ink.opacity(0.16).hsla())
        .text_size(px(12.))
        .font_weight(FontWeight(700.))
        .text_color(ink.hsla())
        .child(
            svg()
                .path(icon(if muted { "mic-off" } else { "mic" }))
                .size(px(14.))
                .flex_shrink_0()
                .text_color(ink.hsla()),
        )
        .child(if muted { "Muted" } else { "Mic is live" });
    let parts = app.welcome.captured.clone().unwrap_or_default().parts();
    let keys = parts.into_iter().map(|part| {
        div()
            .h(px(28.))
            .px(px(9.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .bg(palette.surface_3.hsla())
            .text_size(px(12.))
            .font_weight(FontWeight(800.))
            .child(part)
    });
    recap_row(
        "microphone-3-bold",
        "Mute",
        "Press it now to try.".into(),
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .flex_shrink_0()
            .child(chip)
            .child(div().w(px(4.)))
            .children(keys),
        &palette,
    )
}

fn volume_row(app: &SettingsApp) -> Div {
    let palette = app.palette;
    let volume = &app.snapshot.config.volume;
    let detail: SharedString = match volume.wheel.zone {
        crate::WheelZone::Off => "The wheel is off. Use the tray icon or the volume keys.".into(),
        zone => format!("Scroll {}.", wheel_label(zone)).into(),
    };
    recap_row(
        "volume-loud-bold",
        "Volume",
        detail,
        div()
            .w(px(40.))
            .h(px(30.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .child(skin_preview(
                volume.tray_icon.skin,
                0.66,
                false,
                palette.accent.hsla(),
                palette.danger.hsla(),
            )),
        &palette,
    )
}
