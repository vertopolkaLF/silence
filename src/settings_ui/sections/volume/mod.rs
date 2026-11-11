//! Volume pages. Each one is its own file so the controls can land separately.
pub(crate) mod media_keys;
pub(crate) mod osd;
pub(crate) mod tray_icon;
pub(crate) mod wheel;

use super::sounds::PreviewState;
use crate::settings_ui::{
    SettingsApp,
    anim::UI,
    color::{TRANSPARENT, lerp},
    controls::{self, NumericInput, TextInput, TextInputEvent, TextInputOptions},
    style,
};
use gpui::{prelude::*, *};
use std::{collections::HashMap, rc::Rc};

#[derive(Default)]
struct NumberDraft {
    text: String,
    backing: Option<u64>,
    focused: bool,
    input: Option<Entity<TextInput>>,
}

#[derive(Default)]
pub(crate) struct State {
    /// Preview playback for the volume tick. The sink is the shared one.
    pub(crate) preview: PreviewState,
    numbers: HashMap<String, NumberDraft>,
    /// "Move OSD" toggle on the OSD page.
    pub(crate) positioning: bool,
}

pub(crate) fn column() -> Div {
    div()
        .w_full()
        .max_w(px(720.))
        .flex()
        .flex_col()
        .gap(px(18.))
}

/// Page title and optional enable switch. Like the microphone pages, no intro
/// line: the controls speak for themselves.
pub(crate) fn heading<T: IntoElement>(
    title: &'static str,
    _intro: &'static str,
    toggle: Option<T>,
    _cx: &App,
) -> Div {
    div().mb(px(4.)).child(
        style::page_header()
            .h(px(40.))
            .child(style::h1(title))
            .children(toggle),
    )
}

/// Compact integer field. In-range keystrokes apply immediately; blur clamps.
pub(crate) fn numeric_field(
    id: &'static str,
    value: u64,
    min: u64,
    max: u64,
    suffix: &'static str,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    write: impl Fn(&mut crate::Config, u64) + 'static,
) -> Div {
    let max = max.max(min);
    {
        let draft = app.sections.volume.numbers.entry(id.into()).or_default();
        if !draft.focused && draft.backing != Some(value) {
            draft.backing = Some(value);
            draft.text = value.to_string();
        }
    }
    let text = app
        .sections
        .volume
        .numbers
        .get(id)
        .filter(|draft| draft.focused)
        .map(|draft| draft.text.clone())
        .unwrap_or_else(|| value.to_string());
    let write: Rc<dyn Fn(&mut crate::Config, u64)> = Rc::new(write);
    let on_change = write.clone();
    let on_commit = write.clone();
    let on_step = write.clone();
    let input = controls::text_input(
        id,
        text,
        TextInputOptions {
            numeric: Some(NumericInput { min, max }),
            ..Default::default()
        },
        window,
        cx,
        move |app, event, window, cx| match event {
            TextInputEvent::Focus => {
                if let Some(draft) = app.sections.volume.numbers.get_mut(id) {
                    draft.focused = true;
                }
            }
            TextInputEvent::Change(raw) => {
                if let Some(draft) = app.sections.volume.numbers.get_mut(id) {
                    draft.text = raw.to_string();
                }
                if let Ok(parsed) = raw.as_ref().parse::<u64>()
                    && (min..=max).contains(&parsed)
                {
                    store_number(app, id, parsed, &on_change, cx);
                }
                cx.notify();
            }
            TextInputEvent::Blur(raw) => {
                commit_number(app, id, &raw, false, min, max, &on_commit, window, cx);
            }
            TextInputEvent::Enter(raw) => {
                commit_number(app, id, &raw, true, min, max, &on_commit, window, cx);
            }
            TextInputEvent::Up => step_number(app, id, true, min, max, &on_step, window, cx),
            TextInputEvent::Down => step_number(app, id, false, min, max, &on_step, window, cx),
            TextInputEvent::Escape => {
                let restore = app
                    .sections
                    .volume
                    .numbers
                    .get(id)
                    .and_then(|draft| draft.backing)
                    .unwrap_or(value);
                if let Some(draft) = app.sections.volume.numbers.get_mut(id) {
                    draft.focused = false;
                    draft.text = restore.to_string();
                }
                let shown = restore.to_string();
                cx.defer_in(window, move |app, window, cx| {
                    if let Some(input) = app
                        .sections
                        .volume
                        .numbers
                        .get(id)
                        .and_then(|draft| draft.input.clone())
                    {
                        input.update(cx, |input, cx| input.set_value(shown, cx));
                    }
                    app.focus_handle.focus(window, cx);
                });
                cx.notify();
            }
        },
    );
    if let Some(draft) = app.sections.volume.numbers.get_mut(id) {
        draft.input = Some(input.clone());
    }
    let p = app.palette;
    let focused = input.read(cx).focus_handle(cx).is_focused(window);
    let focus = crate::settings_ui::anim::tween(
        window,
        cx,
        (ElementId::from(id), "input-focus"),
        focused as u8 as f32,
        UI,
    );
    let (pointer, hover, _) = controls::hover_tween((ElementId::from(id), "shell"), window, cx);
    let focus_handle = input.read(cx).focus_handle(cx);
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .flex_shrink_0()
        .child(
            controls::track_pointer(
                controls::text_input_container((ElementId::from(id), "shell"), &input)
                    .w(px(84.))
                    .h(px(42.))
                    .px(px(12.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(lerp(TRANSPARENT, p.focus, focus).hsla())
                    .bg(lerp(p.surface_2, p.surface_3, hover).hsla())
                    .text_size(px(14.))
                    .font_weight(FontWeight(600.))
                    .text_color(p.text.hsla())
                    .cursor(CursorStyle::IBeam)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        focus_handle.focus(window, cx);
                    }),
                pointer,
            ),
        )
        .child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight(600.))
                .text_color(p.muted.hsla())
                .child(suffix),
        )
}

fn store_number(
    app: &mut SettingsApp,
    id: &str,
    parsed: u64,
    write: &Rc<dyn Fn(&mut crate::Config, u64)>,
    cx: &mut Context<SettingsApp>,
) {
    let changed = app
        .sections
        .volume
        .numbers
        .get(id)
        .and_then(|draft| draft.backing)
        != Some(parsed);
    if let Some(draft) = app.sections.volume.numbers.get_mut(id) {
        draft.backing = Some(parsed);
    }
    if changed {
        app.update_config(cx, |config| write(config, parsed));
    }
}

fn commit_number(
    app: &mut SettingsApp,
    id: &'static str,
    raw: &str,
    keep_focus: bool,
    min: u64,
    max: u64,
    write: &Rc<dyn Fn(&mut crate::Config, u64)>,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) {
    let parsed = if raw.is_empty() {
        min
    } else {
        raw.parse::<u64>().unwrap_or(max).clamp(min, max)
    };
    if let Some(draft) = app.sections.volume.numbers.get_mut(id) {
        draft.focused = keep_focus;
        draft.text = parsed.to_string();
    }
    store_number(app, id, parsed, write, cx);
    if raw != parsed.to_string() {
        let shown = parsed.to_string();
        cx.defer_in(window, move |app, _, cx| {
            if let Some(input) = app
                .sections
                .volume
                .numbers
                .get(id)
                .and_then(|draft| draft.input.clone())
            {
                input.update(cx, |input, cx| input.set_value(shown, cx));
            }
        });
    }
    cx.notify();
}

fn step_number(
    app: &mut SettingsApp,
    id: &'static str,
    up: bool,
    min: u64,
    max: u64,
    write: &Rc<dyn Fn(&mut crate::Config, u64)>,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) {
    let current = app
        .sections
        .volume
        .numbers
        .get(id)
        .and_then(|draft| draft.backing)
        .unwrap_or(min);
    let parsed = if up {
        current.saturating_add(1)
    } else {
        current.saturating_sub(1)
    }
    .clamp(min, max);
    if let Some(draft) = app.sections.volume.numbers.get_mut(id) {
        draft.focused = true;
        draft.text = parsed.to_string();
    }
    store_number(app, id, parsed, write, cx);
    let shown = parsed.to_string();
    cx.defer_in(window, move |app, _, cx| {
        if let Some(input) = app
            .sections
            .volume
            .numbers
            .get(id)
            .and_then(|draft| draft.input.clone())
        {
            input.update(cx, |input, cx| input.set_value(shown, cx));
        }
    });
    cx.notify();
}
