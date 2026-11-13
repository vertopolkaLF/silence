//! Auto mute tab: the same toggle rows as every other page.
use super::form;
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
            .w(px(56.))
            .h(px(32.))
            .px(px(10.))
            .rounded(px(8.))
            .text_size(px(14.))
            .font_weight(FontWeight(600.))
            .border_1()
            .border_color(lerp(TRANSPARENT, p.focus, focus).hsla())
            .bg(lerp(p.surface_3, p.surface_hover, hover * (1. - disabled)).hsla())
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
    let startup = controls::toggle(
        "auto-mute-startup",
        config.mute_on_startup,
        window,
        cx,
        |app, value, _, cx| app.update_config(cx, |c| c.auto_mute.mute_on_startup = value),
    );
    let inactivity = controls::toggle(
        "auto-mute-inactivity",
        enabled,
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
    let unmute = controls::toggle(
        "auto-mute-unmute",
        config.unmute_on_activity,
        window,
        cx,
        |app, value, _, cx| app.update_config(cx, |c| c.auto_mute.unmute_on_activity = value),
    );
    let sounds = controls::toggle(
        "auto-mute-sounds",
        config.play_sounds,
        window,
        cx,
        |app, value, _, cx| app.update_config(cx, |c| c.auto_mute.play_sounds = value),
    );
    // The minutes sit in the row, next to the switch they belong to.
    let minutes_field = div()
        .flex()
        .items_center()
        .gap(px(7.))
        .flex_shrink_0()
        .opacity(1. - 0.6 * disabled)
        .child(style::desc("after", cx))
        .child(number)
        .child(style::desc("min", cx));
    let inactivity_row = form::setting_row(
        "Mute after inactivity",
        Some("No keyboard or mouse input."),
        div()
            .flex()
            .items_center()
            .gap(px(14.))
            .flex_shrink_0()
            .child(minutes_field)
            .child(inactivity),
        cx,
    );
    vec![
        div()
            .flex()
            .flex_col()
            .max_w(px(720.))
            .child(form::page_title("Auto mute", None))
            .child(
                form::card(cx)
                    .row(form::setting_row(
                        "Mute on app launch",
                        Some("silence! starts with the microphone muted."),
                        startup,
                        cx,
                    ))
                    .row(inactivity_row)
                    // Only meaningful while the inactivity mute is on, so it
                    // opens with it, indented under its parent.
                    .reveal(
                        "auto-mute-dependent",
                        enabled,
                        form::setting_row("Unmute on activity", None, unmute, cx).pl(px(40.)),
                        window,
                        cx,
                    )
                    .row(form::setting_row(
                        "Play sounds on auto-mute",
                        None,
                        sounds,
                        cx,
                    )),
            )
            .into_any_element(),
    ]
}
