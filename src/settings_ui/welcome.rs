//! Welcome flow from `gui/welcome.rs`, welcome.css and welcome-stars.js.
//! The first step greets and picks Microphone and/or Volume; later steps follow that choice.
mod flow;
use super::{
    SettingsApp,
    anim::{self, Easing, Transition},
    assets::icon,
    color::{TRANSPARENT, mix_pct},
    controls::{self, ButtonKind},
    live, style, theme,
};
use gpui::{
    Bounds, BoxShadow, Context, Div, FontWeight, IntoElement, ParentElement, PathBuilder,
    SharedString, Styled, Task, TextRun, Window, canvas, div, fill, linear_color_stop,
    linear_gradient, point, prelude::*, px, relative, rgb, size, svg,
};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

pub(crate) struct State {
    step: usize,
    captured: Option<crate::Shortcut>,
    modifier_hold: Option<(crate::Shortcut, Instant)>,
    held_keys: HashSet<u32>,
    progress: f32,
    completed: bool,
    closing: Option<Instant>,
    entered: Instant,
    poll: Option<Task<()>>,
    stars: StarField,
    keycaps: HashMap<String, Keycap>,
    button_focus: HashMap<&'static str, gpui::FocusHandle>,
    /// Seeded from `config.modules`. A valid choice is written back to it.
    pick_microphone: bool,
    pick_volume: bool,
    volume2_detection: Option<crate::volume2_import::Volume2Detection>,
    volume2_probe_started: bool,
    /// Latched when leaving the module step so later indexes stay put.
    volume2_included: bool,
    volume2_summary: Vec<String>,
    /// Result text under the import, autostart and close rows, in that order.
    volume2_notes: [String; 3],
    volume2_note_errors: [bool; 3],
}

impl Default for State {
    fn default() -> Self {
        Self {
            step: 0,
            captured: None,
            modifier_hold: None,
            held_keys: HashSet::new(),
            progress: 0.,
            completed: false,
            closing: None,
            entered: Instant::now(),
            poll: None,
            stars: StarField::new(),
            keycaps: HashMap::new(),
            button_focus: HashMap::new(),
            pick_microphone: false,
            pick_volume: false,
            volume2_detection: None,
            volume2_probe_started: false,
            volume2_included: false,
            volume2_summary: Vec::new(),
            volume2_notes: Default::default(),
            volume2_note_errors: [false; 3],
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        if self.poll.is_some() {
            crate::set_settings_hotkey_recording(false);
        }
    }
}

fn set_step(
    app: &mut SettingsApp,
    step: usize,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) {
    app.welcome.poll = None;
    crate::set_settings_hotkey_recording(false);
    // Do not leave queued presses for the main screen's hotkey editor.
    for _ in live::keys().try_iter() {}
    app.welcome.step = step;
    app.welcome.entered = Instant::now();
    app.welcome.modifier_hold = None;
    app.welcome.held_keys.clear();
    app.welcome.keycaps.clear();
    app.focus_handle.focus(window, cx);
    if flow::is_hotkey_step(app) {
        for vk in [
            0xA0,
            0xA1,
            0xA2,
            0xA3,
            0xA4,
            0xA5,
            crate::VK_LWIN,
            crate::VK_RWIN,
        ] {
            if crate::key_down(vk) {
                app.welcome.held_keys.insert(vk);
            }
        }
        crate::set_settings_hotkey_recording(true);
        app.focus_handle.focus(window, cx);
        let keys = live::keys();
        app.welcome.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let alive = this.update(cx, |app, cx| {
                    if !flow::is_hotkey_step(app) || app.snapshot.config.welcome_completed {
                        crate::set_settings_hotkey_recording(false);
                        return false;
                    }
                    poll_recording(app, &keys, cx);
                    true
                });
                if !matches!(alive, Ok(true)) {
                    break;
                }
            }
        }));
    }
    sync_hotkey_trial(app, cx);
    cx.notify();
}

/// Hotkeys stay off during the flow, except on the last screen, where the
/// user tries the one they just picked.
fn sync_hotkey_trial(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    let wanted = flow::current(app) == flow::StepKind::Done && app.welcome.pick_microphone;
    if app.snapshot.config.welcome_hotkey_trial != wanted {
        app.update_config(cx, |config| config.welcome_hotkey_trial = wanted);
    }
}

fn capture(app: &mut SettingsApp, shortcut: crate::Shortcut, cx: &mut Context<SettingsApp>) {
    app.welcome.captured = Some(shortcut.clone());
    let _ = crate::set_welcome_toggle_shortcut(shortcut);
    app.reload_config(cx);
    app.welcome.modifier_hold = None;
    app.welcome.progress = 1.;
    app.welcome.completed = true;
    cx.notify();
}

fn poll_recording(
    app: &mut SettingsApp,
    keys: &flume::Receiver<live::RawKey>,
    cx: &mut Context<SettingsApp>,
) {
    for key in keys.try_iter() {
        // Ctrl/Alt use generic VKs in WM_KEY*, with the side in `extended`.
        let held_vk = match key.vk {
            crate::VK_CONTROL => {
                if key.extended {
                    0xA3
                } else {
                    0xA2
                }
            }
            crate::VK_MENU => {
                if key.extended {
                    0xA5
                } else {
                    0xA4
                }
            }
            other => other,
        };
        if key.down {
            app.welcome.held_keys.insert(held_vk);
        } else {
            app.welcome.held_keys.remove(&held_vk);
        }
        if !key.down {
            continue;
        }
        let held = &app.welcome.held_keys;
        let modifiers = crate::ModifierState {
            ctrl: held.contains(&0xA2) || held.contains(&0xA3),
            alt: held.contains(&0xA4) || held.contains(&0xA5),
            shift: held.contains(&crate::VK_SHIFT) || held.contains(&0xA0) || held.contains(&0xA1),
            win: held.contains(&crate::VK_LWIN) || held.contains(&crate::VK_RWIN),
        };
        if let Some(shortcut) = crate::shortcut_from_raw_vk(key.vk, key.extended, &modifiers) {
            if shortcut.vk != 0 {
                capture(app, shortcut, cx);
            } else if app.welcome.modifier_hold.as_ref().map(|(s, _)| s) != Some(&shortcut) {
                app.welcome.modifier_hold = Some((shortcut, Instant::now()));
                app.welcome.progress = 0.;
                app.welcome.completed = false;
                cx.notify();
            }
        }
    }
    if let Some(shortcut) = crate::take_settings_mouse_pressed_shortcut() {
        capture(app, shortcut, cx);
    } else if let Some((shortcut, started)) = app.welcome.modifier_hold.clone() {
        let modifiers = crate::current_modifier_state();
        let current =
            (modifiers.ctrl || modifiers.alt || modifiers.shift || modifiers.win).then(|| {
                crate::Shortcut {
                    ctrl: modifiers.ctrl,
                    alt: modifiers.alt,
                    shift: modifiers.shift,
                    win: modifiers.win,
                    vk: 0,
                    mouse_buttons: Vec::new(),
                }
            });
        if current.as_ref() != Some(&shortcut) {
            app.welcome.modifier_hold = current.map(|s| (s, Instant::now()));
            app.welcome.progress = 0.;
            app.welcome.completed = false;
            cx.notify();
        } else {
            app.welcome.progress = started.elapsed().as_secs_f32().min(1.);
            app.welcome.completed = false;
            if app.welcome.progress >= 1. {
                capture(app, shortcut, cx);
            }
            cx.notify();
        }
    }
}

fn finish(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    if app.welcome.closing.is_some() {
        return;
    }
    app.welcome.closing = Some(Instant::now());
    cx.spawn(async move |this, cx| {
        cx.background_executor()
            .timer(Duration::from_millis(540))
            .await;
        let _ = this.update(cx, |app, cx| {
            if crate::complete_welcome().is_ok() {
                crate::set_settings_hotkey_recording(false);
                app.welcome.poll = None;
                app.main_intro = Some(Instant::now());
                app.reload_config(cx);
            }
            cx.notify();
        });
    })
    .detach();
    cx.notify();
}

/// CSS closing delays are per block, independent of the screen entrance.
fn block(app: &SettingsApp, delay_ms: u64, child: impl IntoElement) -> Div {
    let closing = app
        .welcome
        .closing
        .map(|started| {
            started
                .elapsed()
                .saturating_sub(Duration::from_millis(delay_ms))
                .as_secs_f32()
                / 0.34
        })
        .unwrap_or(0.)
        .min(1.);
    div()
        .relative()
        .top(px(72. * Easing::Bezier(0.4, 0., 1., 1.).apply(closing)))
        .opacity(1. - Easing::Ease.apply(closing))
        .child(child)
}

fn action(
    id: &'static str,
    label: &'static str,
    primary: bool,
    import: bool,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_click: impl Fn(&mut SettingsApp, (), &mut Window, &mut Context<SettingsApp>) + 'static,
) -> gpui::Stateful<Div> {
    let focus = app
        .welcome
        .button_focus
        .entry(id)
        .or_insert_with(|| cx.focus_handle())
        .clone();
    let focus_color = app.palette.focus.hsla();
    let arrow_color = app.palette.on_accent.hsla();
    let mouse_focus = focus.clone();
    let on_click = std::rc::Rc::new(on_click);
    let keyboard_click = on_click.clone();
    let space_click = on_click.clone();
    controls::button(
        id,
        if primary {
            ButtonKind::Primary
        } else {
            ButtonKind::Secondary
        },
        import.then_some("import-linear"),
        Some(label.into()),
        false,
        window,
        cx,
        move |app, value, window, cx| {
            if app.welcome.closing.is_none() {
                on_click(app, value, window, cx);
            }
        },
    )
    .h(px(44.))
    .px(px(20.))
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
    .on_key_down(
        cx.listener(move |app, event: &gpui::KeyDownEvent, window, cx| {
            if event.keystroke.key == "enter" && app.welcome.closing.is_none() {
                keyboard_click(app, (), window, cx);
                cx.stop_propagation();
            }
        }),
    )
    .on_key_up(
        cx.listener(move |app, event: &gpui::KeyUpEvent, window, cx| {
            if event.keystroke.key == "space" && app.welcome.closing.is_none() {
                space_click(app, (), window, cx);
                cx.stop_propagation();
            }
        }),
    )
    .when(primary, |this| {
        this.child(
            svg()
                .path(icon("arrow-right-linear"))
                .size(px(17.))
                .text_color(arrow_color),
        )
    })
    .when(app.welcome.closing.is_some(), |this| this.cursor_default())
}

fn heading(
    title: &'static str,
    subtitle: Option<&'static str>,
    final_screen: bool,
    cx: &gpui::App,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .font_family("Bricolage Grotesque Welcome")
                .font_weight(FontWeight(650.))
                .text_size(px(56.))
                .line_height(relative(0.92))
                .child(title),
        )
        .children(subtitle.map(|text| {
            style::p(text, cx).when(final_screen, |this| {
                this.max_w(px(560.))
                    .text_size(px(15.))
                    .line_height(relative(1.55))
            })
        }))
}

struct Keycap {
    born: Instant,
    moved: Instant,
    from: (f32, f32),
    to: (f32, f32),
}

impl Keycap {
    fn position(&self) -> (f32, f32) {
        let t = Easing::Bezier(0.22, 1., 0.36, 1.).apply(self.moved.elapsed().as_secs_f32() / 0.22);
        (
            self.from.0 + (self.to.0 - self.from.0) * t,
            self.from.1 + (self.to.1 - self.from.1) * t,
        )
    }
}

/// Left-aligned keycaps: actual font measurement, a 220ms FLIP and keycap-rise.
fn keycaps(app: &mut SettingsApp, window: &mut Window, cx: &mut Context<SettingsApp>) -> Div {
    let shortcut = app
        .welcome
        .modifier_hold
        .as_ref()
        .map(|(s, _)| s)
        .or(app.welcome.captured.as_ref())
        .cloned()
        .unwrap_or_default();
    let parts = shortcut.parts();
    let mut font = gpui::font(theme::FONT_TEXT);
    font.weight = FontWeight(800.);
    let run = TextRun {
        len: 0,
        font,
        color: app.palette.text.hsla(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let widths: Vec<f32> = parts
        .iter()
        .map(|part| {
            let mut run = run.clone();
            run.len = part.len();
            let line = window.text_system().shape_line(
                SharedString::from(part.clone()),
                px(22.),
                &[run],
                None,
            );
            (f32::from(line.width) + 36.).max(70.)
        })
        .collect();
    let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
    let mut row_width = 0.;
    for (i, width) in widths.iter().enumerate() {
        let gap = if rows.last().unwrap().is_empty() {
            0.
        } else {
            10.
        };
        if row_width + gap + width > 680. && gap > 0. {
            rows.push(Vec::new());
            row_width = 0.;
        }
        if !rows.last().unwrap().is_empty() {
            row_width += 10.;
        }
        rows.last_mut().unwrap().push(i);
        row_width += width;
    }
    let mut elements = Vec::new();
    let mut present = HashSet::new();
    for (row, indices) in rows.iter().enumerate() {
        let mut x = 0.;
        for i in indices {
            let key = format!("{i}-{}", parts[*i]);
            present.insert(key.clone());
            let target = (x, row as f32 * 74.);
            let cap = app
                .welcome
                .keycaps
                .entry(key.clone())
                .or_insert_with(|| Keycap {
                    born: Instant::now(),
                    moved: Instant::now(),
                    from: target,
                    to: target,
                });
            if cap.to != target {
                cap.from = cap.position();
                cap.to = target;
                cap.moved = Instant::now();
            }
            let (left, top) = cap.position();
            let rise = Easing::Ease.apply(cap.born.elapsed().as_secs_f32() / 0.2);
            elements.push(
                div()
                    .id(SharedString::from(format!("welcome-cap-{key}")))
                    .absolute()
                    .left(px(left))
                    .top(px(top + 8. * (1. - rise)))
                    .w(px(widths[*i]))
                    .h(px(64.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .bg(app.palette.surface_2.hsla())
                    .opacity(rise)
                    .text_size(px(22.))
                    .font_weight(FontWeight(800.))
                    .shadow(vec![BoxShadow {
                        color: app.palette.shadow_soft.hsla(),
                        offset: point(px(0.), px(12.)),
                        blur_radius: px(24.),
                        spread_radius: px(0.),
                        inset: false,
                    }])
                    .child(parts[*i].clone()),
            );
            x += widths[*i] + 10.;
        }
    }
    app.welcome.keycaps.retain(|key, _| present.contains(key));
    let height = anim::tween(
        window,
        cx,
        "welcome-keycaps-height",
        rows.len() as f32 * 74. - 10.,
        Transition::new(220, Easing::Bezier(0.22, 1., 0.36, 1.)),
    );
    div()
        .relative()
        .w_full()
        .h(px(height.max(64.)))
        .children(elements)
}

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> impl IntoElement {
    if app.welcome.captured.is_none() {
        app.welcome.entered = Instant::now();
        app.welcome.captured = Some(
            app.snapshot
                .config
                .hotkeys
                .iter()
                .find(|b| b.action == crate::HotkeyAction::ToggleMute && b.gamepad.is_none())
                .map(|b| b.shortcut.clone())
                .unwrap_or_default(),
        );
        flow::init(app, cx);
        if let Some(step) = super::debug_arg("--welcome-preview=") {
            flow::preview(app, &step);
        }
        // A trial left on by a crashed settings window must not outlive it.
        sync_hotkey_trial(app, cx);
    }
    // Only the visible welcome requests star frames. Completion unmounts it.
    if window.is_visible() {
        window.request_animation_frame();
    }
    let palette = app.palette;
    let closing_bg = app
        .welcome
        .closing
        .map(|s| 1. - Easing::Ease.apply(s.elapsed().as_secs_f32() / 0.38))
        .unwrap_or(1.);
    let entrance = Easing::Bezier(0.22, 1., 0.36, 1.)
        .apply(app.welcome.entered.elapsed().as_secs_f32() / 0.26);
    let progress = anim::tween(
        window,
        cx,
        "welcome-progress",
        app.welcome.progress,
        Transition::new(120, Easing::Linear),
    );
    let tint = anim::tween(
        window,
        cx,
        "welcome-progress-tint",
        if app.welcome.completed { 0.01 } else { 0.16 },
        anim::UI,
    );
    let fill_tint = anim::tween(
        window,
        cx,
        "welcome-fill-tint",
        if app.welcome.completed { 0.02 } else { 0.16 },
        anim::UI,
    );
    let kind = flow::current(app);
    let viewport = window.viewport_size();
    let stars = app.welcome.stars.frame(
        f32::from(viewport.width),
        f32::from(viewport.height),
        kind == flow::StepKind::Done,
        window.is_visible(),
    );
    let star_canvas = canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            for star in stars {
                star.paint(bounds, window);
            }
        },
    )
    .absolute()
    .left_0()
    .top_0()
    .size_full();
    let dots = flow::progress_dots(app, window, cx);
    let mut screen = div()
        .flex()
        .flex_col()
        .gap(px(22.))
        .child(block(app, 80, dots));
    match kind {
        flow::StepKind::Modules => {
            screen = screen.child(flow::modules_step(app, window, cx));
        }
        flow::StepKind::Volume2 => {
            screen = screen.child(flow::volume2_step(app, window, cx));
        }
        flow::StepKind::Wheel => {
            screen = screen.child(flow::wheel_step(app, window, cx));
        }
        flow::StepKind::Look => {
            screen = screen.child(flow::look_step(app, window, cx));
        }
        flow::StepKind::Done => {
            screen = screen.child(flow::done_step(app, window, cx));
        }
        flow::StepKind::MicHotkey => {
            screen = screen.child(block(
                app,
                80,
                heading(
                    "Pick your mute hotkey",
                    Some(
                        "Press a key combo or a mouse button. Hold a modifier on its own for a second to use just that.",
                    ),
                    true,
                    cx,
                ),
            ));
            let caps = keycaps(app, window, cx);
            screen = screen.child(block(app, 40, caps));
            screen = screen.child(block(
                app,
                20,
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(13.))
                    .text_color(palette.muted.hsla())
                    .child(
                        svg()
                            .path(icon("monitor-linear"))
                            .size(px(16.))
                            .flex_shrink_0()
                            .text_color(palette.muted.hsla()),
                    )
                    .child("Works in games and fullscreen apps. You can add more hotkeys later."),
            ));
            let back = action(
                "welcome-back",
                "Back",
                false,
                false,
                app,
                window,
                cx,
                |app, _, window, cx| {
                    app.welcome.completed = false;
                    app.welcome.progress = 0.;
                    flow::retreat(app, window, cx);
                },
            );
            let next = action(
                "welcome-looks-good",
                "Use this hotkey",
                true,
                false,
                app,
                window,
                cx,
                |app, _, window, cx| {
                    capture(app, app.welcome.captured.clone().unwrap_or_default(), cx);
                    flow::advance(app, window, cx);
                },
            );
            screen = screen.child(block(
                app,
                0,
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(back)
                    .child(next),
            ));
        }
    }
    let background = div()
        .absolute()
        .left_0()
        .top_0()
        .size_full()
        .opacity(closing_bg)
        .bg(linear_gradient(
            135.,
            linear_color_stop(palette.content_bg.hsla(), 0.),
            linear_color_stop(TRANSPARENT.hsla(), 1.),
        ))
        .child(star_canvas)
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .h_full()
                .w(px(f32::from(viewport.width) * progress))
                .bg(palette.accent.opacity(fill_tint).hsla()),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .h_full()
                .w(px(f32::from(viewport.width) * progress))
                .bg(palette.accent.opacity(tint).hsla()),
        )
        // Accepted radial-gradient approximation: one broad blurred shadow.
        .child(
            div()
                .absolute()
                .left(px(-160.))
                .top(px(-160.))
                .size(px(320.))
                .rounded_full()
                .shadow(vec![BoxShadow {
                    color: palette.accent.opacity(0.13).hsla(),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(400.),
                    spread_radius: px(100.),
                    inset: false,
                }]),
        )
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .size_full()
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(mix_pct(palette.content_bg, 22., TRANSPARENT).hsla(), 0.),
                    linear_color_stop(TRANSPARENT.hsla(), 1.),
                )),
        );
    div()
        .id("welcome-shell")
        .line_height(relative(1.2))
        .relative()
        .size_full()
        .overflow_hidden()
        .p(px(42.))
        .flex()
        .items_center()
        .justify_center()
        .on_key_down(cx.listener(|app, event: &gpui::KeyDownEvent, window, cx| {
            if event.keystroke.key != "tab"
                || flow::is_hotkey_step(app)
                || app.welcome.closing.is_some()
            {
                return;
            }
            let ids = flow::focus_ids(app);
            if ids.is_empty() {
                return;
            }
            let current = ids.iter().position(|id| {
                app.welcome
                    .button_focus
                    .get(id)
                    .is_some_and(|focus| focus.is_focused(window))
            });
            let next = if event.keystroke.modifiers.shift {
                current
                    .map(|i| (i + ids.len() - 1) % ids.len())
                    .unwrap_or(ids.len() - 1)
            } else {
                current.map(|i| (i + 1) % ids.len()).unwrap_or(0)
            };
            if let Some(focus) = app.welcome.button_focus.get(ids[next]) {
                focus.focus(window, cx);
            }
            cx.stop_propagation();
        }))
        .child(background)
        .child(
            div().absolute().top(px(36.)).left(px(40.)).child(block(
                app,
                120,
                div()
                    .flex()
                    .items_baseline()
                    .font_family("Bricolage Grotesque Welcome Logo")
                    .text_size(px(38.))
                    .font_weight(FontWeight(800.))
                    .line_height(relative(0.9))
                    .text_color(rgb(0xffffff))
                    .child("silence")
                    .child(div().text_color(rgb(0xff922d)).child("!")),
            )),
        )
        .child(
            div()
                .relative()
                .w_full()
                .max_w(px(680.))
                .min_h(px(430.))
                .flex()
                .flex_col()
                .justify_center()
                .child(
                    screen
                        .relative()
                        .top(px(14. * (1. - entrance)))
                        .opacity(entrance),
                ),
        )
}

const STAR_COLORS: [u32; 7] = [
    0xc084fc, 0x4ade80, 0xfb923c, 0xf472b6, 0x60a5fa, 0xfde047, 0xffffff,
];
struct Star {
    x: f32,
    y: f32,
    base_size: f32,
    color: u32,
    speed: f32,
    progress: f32,
    progress_speed: f32,
}
struct StarField {
    stars: Vec<Star>,
    rng: u64,
    current_ftl: f32,
    warp: bool,
    last_frame: Instant,
    dimensions: (f32, f32),
}

impl StarField {
    fn new() -> Self {
        Self {
            stars: Vec::new(),
            rng: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64
                | 1,
            current_ftl: 1.,
            warp: false,
            last_frame: Instant::now(),
            dimensions: (0., 0.),
        }
    }
    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1_u32 << 24) as f32
    }
    fn star(&mut self, random_y: bool) -> Star {
        let x = self.random() * self.dimensions.0;
        let y = if random_y {
            self.random() * self.dimensions.1
        } else {
            self.dimensions.1 + 10. + self.random() * self.current_ftl * 5.
        };
        let base_size = self.random() * 3. + 1.5;
        let color = STAR_COLORS[(self.random() * 7.) as usize];
        let duration = self.random() * 15. + 5.;
        Star {
            x,
            y,
            base_size,
            color,
            speed: self.dimensions.1 * 1.2 / (duration * 60.),
            progress: if random_y { self.random() } else { 0. },
            progress_speed: 1. / (duration * 60.),
        }
    }
    fn frame(&mut self, width: f32, height: f32, warp: bool, visible: bool) -> Vec<StarPaint> {
        let dt = if visible {
            self.last_frame.elapsed().as_secs_f32().min(0.05) * 60.
        } else {
            0.
        };
        self.last_frame = Instant::now();
        self.dimensions = (width, height);
        if self.stars.is_empty() || (self.warp && !warp) {
            self.current_ftl = 1.;
            self.stars = (0..55).map(|_| self.star(true)).collect();
        }
        self.warp = warp;
        self.current_ftl +=
            (if warp { 10. } else { 1. } - self.current_ftl) * (1. - 0.95_f32.powf(dt));
        let ftl = self.current_ftl;
        for index in 0..self.stars.len() {
            let star = &mut self.stars[index];
            star.y -= star.speed * ftl * dt;
            star.progress += star.progress_speed * (1. + (ftl - 1.) * 0.1) * dt;
            if star.progress >= 1. || star.y < -200. {
                self.stars[index] = self.star(false);
            }
        }
        self.stars
            .iter()
            .map(|star| {
                let p = star.progress;
                let mut opacity = if p < 0.2 {
                    p / 0.2 * 0.8
                } else if p < 0.8 {
                    0.8
                } else {
                    (1. - p) / 0.2 * 0.8
                };
                if ftl > 2. {
                    opacity *= (1. - (ftl - 2.) / 48.).max(0.28);
                }
                let diameter = star.base_size * (0.5 + p) * (1. + ((ftl - 1.) / 15.).min(1.5));
                StarPaint {
                    x: star.x,
                    y: star.y,
                    diameter,
                    stretch: ((ftl - 1.) * star.speed * 2.).max(0.1),
                    color: star.color,
                    opacity,
                    glow: (diameter * 2. * (1. - (ftl - 1.) / 2.)).max(0.),
                }
            })
            .collect()
    }
}

struct StarPaint {
    x: f32,
    y: f32,
    diameter: f32,
    stretch: f32,
    color: u32,
    opacity: f32,
    glow: f32,
}

impl StarPaint {
    fn paint(self, bounds: Bounds<gpui::Pixels>, window: &mut Window) {
        let mut color: gpui::Hsla = rgb(self.color).into();
        color.a = self.opacity;
        let top = bounds.origin + point(px(self.x), px(self.y));
        let rect = Bounds::new(
            top - point(px(self.diameter / 2.), px(self.diameter / 2.)),
            size(px(self.diameter), px(self.diameter + self.stretch)),
        );
        if self.glow > 0. {
            window.paint_drop_shadows(
                rect,
                (self.diameter / 2.).into(),
                &[BoxShadow {
                    color,
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(self.glow),
                    spread_radius: px(0.),
                    inset: false,
                }],
            );
        }
        let mut streak = PathBuilder::stroke(px(self.diameter));
        streak.move_to(top);
        streak.line_to(top + point(px(0.), px(self.stretch)));
        if let Ok(path) = streak.build() {
            window.paint_path(path, color);
        }
        // Canvas lineCap="round".
        for y in [0., self.stretch] {
            let mut cap = fill(
                Bounds::new(
                    top + point(px(-self.diameter / 2.), px(y - self.diameter / 2.)),
                    size(px(self.diameter), px(self.diameter)),
                ),
                color,
            );
            cap.corner_radii = (self.diameter / 2.).into();
            window.paint_quad(cap);
        }
    }
}
