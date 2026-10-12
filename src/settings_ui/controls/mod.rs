//! Shared widgets ported from `assets/styles/controls.css`, `global.css` and
//! `sounds.css`. Every widget takes a stable `id` (unique within the window)
//! that keys its hover/transition state.
//!
//! Callbacks receive the app, so sections can call `app.update_config(...)`:
//! `toggle(id, checked, window, cx, |app, checked, _, cx| app.update_config(cx, |c| ...))`.
mod range;
mod segmented_toggle;
mod select;
mod text_input;

pub(crate) use range::{RangeOptions, range};
pub(crate) use segmented_toggle::{SegmentedToggleOption, segmented_toggle};
pub(crate) use select::{SelectOption, SelectOptions, select};
pub(crate) use text_input::{NumericInput, TextInput, TextInputEvent, TextInputOptions, text_input, text_input_container};

use super::{
    SettingsApp,
    anim::{self, UI},
    assets::icon,
    color::{TRANSPARENT, lerp},
    style::palette,
};
use gpui::{
    BoxShadow, Context, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, Stateful, Styled, Transformation, Window, div, point, prelude::*, px, size, svg,
    Div,
};

/// CSS `outline: 2px solid var(--focus); outline-offset: -1px` without
/// changing layout or hover borders. One pixel paints on either side.
pub(crate) fn focus_outline(style: gpui::StyleRefinement, color: gpui::Hsla) -> gpui::StyleRefinement {
    style.shadow(vec![
        BoxShadow { color, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(1.), inset: true },
        BoxShadow { color, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(1.), inset: false },
    ])
}

/// Hover state plus a 0..1 hover tween for an element keyed by `id`.
pub(crate) fn hover_tween(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> (gpui::Entity<anim::Pointer>, f32, f32) {
    let id: ElementId = id.into();
    let pointer = anim::pointer(window, cx, (id.clone(), "pointer"));
    let state = *pointer.read(cx);
    let hover = anim::tween(window, cx, (id.clone(), "hover"), state.hovered as u8 as f32, UI);
    let press = anim::tween(window, cx, (id, "press"), state.pressed as u8 as f32, UI);
    (pointer, hover, press)
}

/// Wires hover/press tracking onto an interactive element.
pub(crate) fn track_pointer(element: Stateful<Div>, pointer: gpui::Entity<anim::Pointer>) -> Stateful<Div> {
    let down = pointer.clone();
    let up = pointer.clone();
    element
        .on_hover(move |hovered, _, cx| anim::set_hovered(&pointer, *hovered, cx))
        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| anim::set_pressed(&down, true, cx))
        .on_mouse_up(gpui::MouseButton::Left, move |_, _, cx| anim::set_pressed(&up, false, cx))
}

/// `.toggle`: 46x26 pill switch; the knob slides 20px.
pub(crate) fn toggle(
    id: impl Into<ElementId>,
    checked: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_change: impl Fn(&mut SettingsApp, bool, &mut Window, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    let id: ElementId = id.into();
    let palette = palette(cx);
    let on = anim::tween(window, cx, (id.clone(), "checked"), checked as u8 as f32, UI);
    div()
        .id(id)
        .relative()
        .w(px(46.))
        .h(px(26.))
        .flex_shrink_0()
        .rounded_full()
        .cursor_pointer()
        .bg(lerp(palette.surface_3, palette.accent, on).hsla())
        .shadow(vec![BoxShadow {
            color: palette.shadow_medium.hsla(),
            offset: point(px(0.), px(1.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
            inset: true,
        }])
        .on_click(cx.listener(move |app, _, window, cx| on_change(app, !checked, window, cx)))
        .child(
            div()
                .absolute()
                .top(px(5.))
                .left(px(5. + 20. * on))
                .size(px(16.))
                .rounded_full()
                .bg(palette.text.hsla())
                .shadow(vec![BoxShadow {
                    color: palette.shadow_medium.hsla(),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(2.),
                    spread_radius: px(0.),
                    inset: false,
                }]),
        )
}

/// `.ui-checkbox`: 20px rounded box with a scaling check mark and a label.
pub(crate) fn checkbox(
    id: impl Into<ElementId>,
    checked: bool,
    disabled: bool,
    label: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_change: impl Fn(&mut SettingsApp, bool, &mut Window, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    let id: ElementId = id.into();
    let palette = palette(cx);
    let (pointer, hover, press) = hover_tween(id.clone(), window, cx);
    let on = anim::tween(window, cx, (id.clone(), "checked"), checked as u8 as f32, UI);
    let fg = if disabled {
        palette.faint
    } else {
        lerp(palette.muted, palette.text, hover.max(press).max(on))
    };
    let box_bg = if disabled {
        crate::settings_ui::color::mix_pct(palette.surface_3, 88., TRANSPARENT)
    } else {
        lerp(palette.surface_3, palette.accent, on)
    };
    let mark_scale = 0.2 + 0.8 * on;
    let element = div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(12.))
        .min_w_0()
        .text_size(px(14.))
        .font_weight(FontWeight(500.))
        .text_color(fg.hsla())
        .when(!disabled, |this| this.cursor_pointer())
        .when(disabled, |this| this.cursor_not_allowed())
        .child(
            div()
                .size(px(20.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .bg(box_bg.hsla())
                .when(!disabled, |this| {
                    this.shadow(vec![BoxShadow {
                        color: palette.highlight_line.hsla(),
                        offset: point(px(0.), px(1.)),
                        blur_radius: px(0.),
                        spread_radius: px(0.),
                        inset: true,
                    }])
                })
                .child(
                    svg()
                        .path(icon("check-mark"))
                        .size(px(14.))
                        .with_transformation(Transformation::scale(size(mark_scale, mark_scale)))
                        .text_color(palette.on_accent.opacity(on).hsla()),
                ),
        )
        .child(div().min_w_0().line_height(gpui::relative(1.35)).child(label.into()));
    let element = track_pointer(element, pointer);
    if disabled {
        element
    } else {
        element.on_click(cx.listener(move |app, _, window, cx| on_change(app, !checked, window, cx)))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonKind {
    /// `.secondary`: surface-2 button that brightens on hover.
    Secondary,
    /// `.save`: accent-filled primary button.
    Primary,
    /// `.icon-button`: 38x38 square with a muted icon.
    Icon,
}

/// Buttons from `global.css` (`.secondary`, `.save`, `.icon-button`): 38px
/// tall, 8px radius. Returns the element so callers can adjust layout.
pub(crate) fn button(
    id: impl Into<ElementId>,
    kind: ButtonKind,
    icon_name: Option<&str>,
    label: Option<SharedString>,
    disabled: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_click: impl Fn(&mut SettingsApp, (), &mut Window, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    let id: ElementId = id.into();
    let palette = palette(cx);
    let (pointer, hover, _) = hover_tween(id.clone(), window, cx);
    let hover = if disabled { 0. } else { hover };
    let (bg, fg, weight, shadow) = match kind {
        ButtonKind::Primary if disabled => (palette.surface_3, palette.faint, 600., None),
        ButtonKind::Primary => (
            lerp(palette.accent, palette.accent_strong, hover),
            palette.on_accent,
            600.,
            Some(2.),
        ),
        ButtonKind::Secondary => (
            lerp(palette.surface_2, palette.surface_hover, hover),
            palette.text,
            500.,
            Some(1.),
        ),
        ButtonKind::Icon => (
            lerp(palette.surface_2, palette.surface_hover, hover),
            palette.muted,
            500.,
            Some(1.),
        ),
    };
    let icon_size = if kind == ButtonKind::Icon { 18. } else { 17. };
    let element = div()
        .id(id)
        .h(px(38.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .flex_shrink_0()
        .rounded(px(8.))
        .bg(bg.hsla())
        .text_color(fg.hsla())
        .text_size(px(14.))
        .font_weight(FontWeight(weight))
        .when(kind == ButtonKind::Icon, |this| this.w(px(38.)))
        .when(kind != ButtonKind::Icon, |this| this.px(px(16.)))
        .when(kind == ButtonKind::Primary, |this| this.min_w(px(104.)))
        .when_some(shadow, |this, y| {
            this.shadow(vec![BoxShadow {
                color: palette.shadow_soft.hsla(),
                offset: point(px(0.), px(y)),
                blur_radius: px(y),
                spread_radius: px(0.),
                inset: false,
            }])
        })
        .when(!disabled, |this| this.cursor_pointer())
        .when_some(icon_name, |this, name| {
            this.child(svg().path(icon(name)).size(px(icon_size)).text_color(fg.hsla()))
        })
        .when_some(label, |this, label| this.child(label));
    let element = track_pointer(element, pointer);
    if disabled {
        element
    } else {
        element.on_click(cx.listener(move |app, _, window, cx| on_click(app, (), window, cx)))
    }
}
