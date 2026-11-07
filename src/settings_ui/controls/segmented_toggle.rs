//! `.hotkey-source-toggle` / `.source-option` from `hotkeys.css`.
use super::{SettingsApp, anim, hover_tween, track_pointer};
use crate::settings_ui::{
    assets::icon,
    color::{TRANSPARENT, lerp},
    style::palette,
};
use gpui::{prelude::*, *};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SegmentedToggleOption {
    pub value: SharedString,
    pub label: SharedString,
    /// Asset name without `icons/` or `.svg`, e.g. `keyboard-linear`.
    pub icon: Option<SharedString>,
}

impl SegmentedToggleOption {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            icon: None,
        }
    }
    pub fn icon(mut self, icon: impl Into<SharedString>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

/// Equal-width segments; returns a caller-stylable shell.
/// ```ignore
/// segmented_toggle("source", source, vec![
///     SegmentedToggleOption::new("keyboard", "Keyboard").icon("keyboard-linear"),
///     SegmentedToggleOption::new("gamepad", "Gamepad").icon("gamepad-bold"),
/// ], window, cx, |app, value, _, cx| { /* apply immediately */ })
/// ```
pub(crate) fn segmented_toggle(
    id: impl Into<ElementId>,
    value: impl Into<SharedString>,
    options: Vec<SegmentedToggleOption>,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_change: impl Fn(&mut SettingsApp, SharedString, &mut Window, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    let id = id.into();
    let value = value.into();
    let p = palette(cx);
    let callback = Rc::new(on_change);
    let children = options
        .into_iter()
        .map(|option| {
            let key = ElementId::from((id.clone(), option.value.clone()));
            let active = anim::tween(
                window,
                cx,
                (key.clone(), "active"),
                (value == option.value) as u8 as f32,
                anim::UI,
            );
            let (pointer, _, _) = hover_tween(key.clone(), window, cx);
            let callback = callback.clone();
            let focus = window
                .use_keyed_state((key.clone(), "focus"), cx, |_, cx| cx.focus_handle())
                .read(cx)
                .clone();
            let key_callback = callback.clone();
            let key_value = option.value.clone();
            track_pointer(
                div()
                    .id(key)
                    .track_focus(&focus)
                    .focus_visible(move |style| super::focus_outline(style, p.focus.hsla()))
                    .flex_1()
                    .min_w_0()
                    .h(px(34.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .rounded(px(12.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .font_weight(FontWeight(650.))
                    .bg(lerp(TRANSPARENT, p.surface_color[3], active).hsla())
                    .text_color(lerp(p.muted, p.text, active).hsla())
                    .shadow(vec![BoxShadow {
                        color: p.shadow_soft.opacity(active).hsla(),
                        offset: point(px(0.), px(1.)),
                        blur_radius: px(2.),
                        spread_radius: px(0.),
                        inset: false,
                    }])
                    .when_some(option.icon, |this, name| {
                        this.child(
                            svg()
                                .path(icon(&name))
                                .size(px(16.))
                                .flex_shrink_0()
                                .text_color(lerp(p.muted, p.text, active).hsla()),
                        )
                    })
                    .child(option.label),
                pointer,
            )
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    key_callback(app, key_value.clone(), window, cx);
                    cx.stop_propagation();
                }
            }))
            .on_click(cx.listener(move |app, _, window, cx| {
                callback(app, option.value.clone(), window, cx)
            }))
        })
        .collect::<Vec<_>>();
    div()
        .id(id)
        .w_full()
        .flex()
        .gap(px(4.))
        .p(px(4.))
        .border_1()
        .border_color(TRANSPARENT.hsla())
        .rounded(px(16.))
        .bg(p.surface_2.hsla())
        .children(children)
}
