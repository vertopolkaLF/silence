//! Auto-Mute tab, ported from `gui/sections/auto_mute.rs` and general.css.
use crate::settings_ui::{
    SettingsApp,
    anim::{self, UI},
    color::{TRANSPARENT, lerp},
    controls::{self, NumericInput, TextInput, TextInputEvent, TextInputOptions},
    style,
};
use gpui::{prelude::*, *};

#[derive(Default)]
pub(crate) struct State {
    minutes: String,
    backing_minutes: Option<u16>,
    input: Option<Entity<TextInput>>,
    focused: bool,
}

fn apply_minutes(app: &mut SettingsApp, value: &str, cx: &mut Context<SettingsApp>) {
    if !value.is_empty() {
        let value = value.parse::<u64>().unwrap_or(u64::MAX).clamp(1, 1440) as u16;
        app.sections.auto_mute.minutes = value.to_string();
        app.sections.auto_mute.backing_minutes = Some(value);
        if app.snapshot.config.auto_mute.after_inactivity_minutes != value {
            app.update_config(cx, |c| c.auto_mute.after_inactivity_minutes = value);
        }
    }
}

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let config = app.snapshot.config.auto_mute.clone();
    let enabled = config.after_inactivity_enabled;
    let minutes = config.after_inactivity_minutes.clamp(1, 1440);
    if app.sections.auto_mute.backing_minutes != Some(minutes) {
        app.sections.auto_mute.backing_minutes = Some(minutes);
        app.sections.auto_mute.minutes = minutes.to_string();
    }
    let draft = app.sections.auto_mute.minutes.clone();
    let input = controls::text_input(
        "auto-mute-minutes-input",
        draft,
        TextInputOptions {
            disabled: !enabled,
            numeric: Some(NumericInput { min: 1, max: 1440 }),
            ..Default::default()
        },
        window,
        cx,
        |app, event, window, cx| {
            if !app.snapshot.config.auto_mute.after_inactivity_enabled {
                if matches!(event, TextInputEvent::Blur(_)) {
                    app.sections.auto_mute.focused = false;
                    app.sections.auto_mute.minutes = app
                        .snapshot
                        .config
                        .auto_mute
                        .after_inactivity_minutes
                        .clamp(1, 1440)
                        .to_string();
                    cx.notify();
                }
                return;
            }
            match event {
                TextInputEvent::Change(value) => {
                    app.sections.auto_mute.minutes = value.to_string();
                    apply_minutes(app, &value, cx);
                    if app.sections.auto_mute.minutes != value.as_ref() {
                        let normalized = app.sections.auto_mute.minutes.clone();
                        // The input is borrowed while emitting Change. Apply the
                        // controlled/clamped value after this event has finished.
                        cx.defer_in(window, move |app, _, cx| {
                            if let Some(input) = &app.sections.auto_mute.input {
                                input.update(cx, |input, cx| input.set_value(normalized, cx));
                            }
                        });
                    }
                }
                TextInputEvent::Focus => app.sections.auto_mute.focused = true,
                TextInputEvent::Blur(value) => {
                    app.sections.auto_mute.focused = false;
                    app.sections.auto_mute.minutes = value.to_string();
                    if app.snapshot.config.auto_mute.after_inactivity_enabled {
                        apply_minutes(app, &value, cx);
                    }
                }
                TextInputEvent::Enter(value) => {
                    app.sections.auto_mute.minutes = value.to_string();
                    apply_minutes(app, &value, cx);
                }
                TextInputEvent::Up | TextInputEvent::Down => {
                    let old = app
                        .sections
                        .auto_mute
                        .minutes
                        .parse::<u64>()
                        .unwrap_or(1)
                        .clamp(1, 1440);
                    let value = if matches!(event, TextInputEvent::Up) {
                        (old + 1).min(1440)
                    } else {
                        old.saturating_sub(1).max(1)
                    };
                    let value = value.to_string();
                    app.sections.auto_mute.minutes = value.clone();
                    apply_minutes(app, &value, cx);
                    cx.defer_in(window, move |app, _, cx| {
                        if let Some(input) = &app.sections.auto_mute.input {
                            input.update(cx, |input, cx| input.set_value(value, cx));
                        }
                    });
                }
                _ => {}
            }
            cx.notify();
        },
    );
    app.sections.auto_mute.input = Some(input.clone());
    let p = app.palette;
    let disabled = anim::tween(
        window,
        cx,
        "auto-mute-subgroup-disabled",
        (!enabled) as u8 as f32,
        UI,
    );
    let focus = anim::tween(
        window,
        cx,
        "auto-mute-number-focus",
        (enabled && app.sections.auto_mute.focused) as u8 as f32,
        UI,
    );
    let (pointer, hover, _) = controls::hover_tween("auto-mute-number", window, cx);
    let number_focus = input.read(cx).focus_handle(cx);
    let number = controls::track_pointer(
        controls::text_input_container("auto-mute-number", &input)
            .w_full()
            .max_w(px(560.))
            .h(px(42.))
            .px(px(14.))
            .rounded(px(12.))
            .border_1()
            .border_color(lerp(TRANSPARENT, p.focus, focus).hsla())
            .bg(lerp(p.surface_2, p.surface_color[1], hover * (1. - disabled)).hsla())
            .text_color(p.text.hsla())
            // The padded shell is part of the input's click target too.
            .when(enabled, |this| {
                this.cursor(CursorStyle::IBeam)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        number_focus.focus(window, cx)
                    })
            })
            .when(!enabled, |this| this.cursor_not_allowed()),
        pointer,
    );
    let startup = controls::checkbox(
        "auto-mute-startup",
        config.mute_on_startup,
        false,
        "Mute microphone on app launch",
        window,
        cx,
        |app, value, _, cx| app.update_config(cx, |c| c.auto_mute.mute_on_startup = value),
    );
    let inactivity = controls::checkbox(
        "auto-mute-inactivity",
        enabled,
        false,
        "Mute microphone after inactivity",
        window,
        cx,
        |app, value, window, cx| {
            // A disabled number field must also release any existing keyboard focus.
            if !value && app.sections.auto_mute.focused {
                app.sections.auto_mute.focused = false;
                app.focus_handle.focus(window, cx);
            }
            app.update_config(cx, |c| c.auto_mute.after_inactivity_enabled = value);
        },
    );
    let unmute = controls::checkbox(
        "auto-mute-unmute",
        config.unmute_on_activity,
        !enabled,
        "Unmute on activity",
        window,
        cx,
        |app, value, _, cx| app.update_config(cx, |c| c.auto_mute.unmute_on_activity = value),
    );
    let sounds = controls::checkbox(
        "auto-mute-sounds",
        config.play_sounds,
        false,
        "Play sounds on auto-mute",
        window,
        cx,
        |app, value, _, cx| app.update_config(cx, |c| c.auto_mute.play_sounds = value),
    );
    let subgroup = div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .ml(px(32.))
        .pl(px(18.))
        .pt(px(4.))
        .pb(px(2.))
        .border_l_1()
        .border_color(p.border.hsla())
        .relative()
        .top(px(-2. * disabled))
        .opacity(1. - 0.54 * disabled)
        .child(
            style::label("Minutes without keyboard or mouse activity").text_color(p.muted.hsla()),
        )
        .child(number)
        .child(unmute);
    vec![
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .max_w(px(720.))
            .child(
                div().mb(px(4.)).child(
                    style::page_header()
                        .h(px(40.))
                        .child(style::h1("Auto-Mute")),
                ),
            )
            .child(
                div()
                    .pb(px(18.))
                    .border_b_1()
                    .border_color(p.border.hsla())
                    .child(startup),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .pb(px(18.))
                    .border_b_1()
                    .border_color(p.border.hsla())
                    .child(style::h2("Inactivity").mb(px(2.)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .child(inactivity)
                            .child(subgroup),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(style::h2("Options").mb(px(2.)))
                    .child(sounds),
            )
            .into_any_element(),
    ]
}
