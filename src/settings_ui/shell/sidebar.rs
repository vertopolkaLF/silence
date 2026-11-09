//! Sidebar: global pages, one open Microphone or Volume group, then General
//! and About pinned to the bottom.
//!
//! Height budget for the 590px window (60px under the brand, 12px bottom):
//! four global rows and two group headers at 42px, one open group of five
//! 32px rows, and the gaps below. Only one group is open, so both sets of
//! children never stack.
use super::nav::{self, SettingsTab};
use crate::settings_ui::{
    SettingsApp,
    anim::{self, Easing, Transition, UI},
    assets::icon,
    color::{Oklch, TRANSPARENT, lerp},
    controls, modules,
    modules::ModuleId,
};
use gpui::{
    Context, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled,
    Transformation, Window, div, prelude::*, px, radians, size, svg,
};

const PRIMARY_H: f32 = 42.;
const CHILD_H: f32 = 32.;
const CHILD_GAP: f32 = 2.;
const GROUP: Transition = Transition::new(260, Easing::EaseInOut);

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> impl IntoElement {
    div()
        .id("sidebar")
        .w(px(240.))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .pt(px(60.))
        .px(px(12.))
        .pb(px(12.))
        .overflow_hidden()
        .child(nav_item(app, SettingsTab::Hotkeys, false, 1., window, cx))
        .child(
            nav_item(app, SettingsTab::Devices, false, 1., window, cx).mt(px(4.)),
        )
        .child(module_group(app, ModuleId::Microphone, window, cx).mt(px(10.)))
        .child(module_group(app, ModuleId::Volume, window, cx).mt(px(6.)))
        .child(div().flex_1().min_h(px(8.)))
        .child(nav_item(app, SettingsTab::General, false, 1., window, cx))
        .child(nav_item(app, SettingsTab::About, false, 1., window, cx).mt(px(4.)))
}

fn module_group(
    app: &mut SettingsApp,
    module: ModuleId,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> gpui::Div {
    let open = app.nav.expanded_module == Some(module);
    let t = anim::tween(
        window,
        cx,
        ("nav-group-open", module.index()),
        open as u8 as f32,
        GROUP,
    );
    let enabled = modules::flags(app).get(module);
    let enabled_t = anim::tween(
        window,
        cx,
        ("nav-group-enabled", module.index()),
        enabled as u8 as f32,
        UI,
    );
    let tabs = nav::module_tabs(module);
    let body_h = CHILD_H * tabs.len() as f32 + CHILD_GAP * tabs.len().saturating_sub(1) as f32 + 4.;

    let mut items = div()
        .pt(px(4.))
        .flex()
        .flex_col()
        .gap(px(CHILD_GAP))
        .w_full()
        .opacity(t)
        .relative()
        .top(px(-6. * (1. - t)));
    for &tab in tabs {
        items = items.child(nav_item(app, tab, true, enabled_t, window, cx));
    }

    div()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .child(module_header(app, module, open, t, window, cx))
        .child(
            div()
                .h(px(body_h * t))
                .overflow_hidden()
                .flex_shrink_0()
                .child(items),
        )
}

fn module_header(
    app: &mut SettingsApp,
    module: ModuleId,
    open: bool,
    open_t: f32,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> impl IntoElement {
    let palette = app.palette;
    let pointer = anim::pointer(window, cx, ("nav-group-pointer", module.index()));
    let hovered = pointer.read(cx).hovered;
    let hover = anim::tween(
        window,
        cx,
        ("nav-group-hover", module.index()),
        hovered as u8 as f32,
        UI,
    );
    let contains = nav::module_tabs(module)
        .iter()
        .any(|tab| *tab == app.nav.active_tab);
    // Collapsed group that holds the current page keeps a quiet accent,
    // so the selection is still visible after the rows fold away.
    let parked = anim::tween(
        window,
        cx,
        ("nav-group-parked", module.index()),
        (contains && !open) as u8 as f32,
        UI,
    );
    let bg = lerp(TRANSPARENT, palette.surface_color(2), hover);
    let fg = lerp(palette.text, palette.accent_text, parked);
    let chevron = fg.opacity(0.62 + 0.38 * open_t);

    div()
        .id(("nav-group", module.index()))
        .h(px(PRIMARY_H))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(8.))
        .w_full()
        .px(px(12.))
        .rounded(px(8.))
        .bg(bg.hsla())
        .text_color(fg.hsla())
        .text_size(px(14.))
        .font_weight(FontWeight(600.))
        .cursor_pointer()
        .on_hover(move |hovered, _, cx| anim::set_hovered(&pointer, *hovered, cx))
        .on_click(cx.listener(move |app, _, _, cx| {
            app.nav.toggle_module_group(module);
            cx.notify();
        }))
        .child(
            svg()
                .size(px(21.))
                .flex_shrink_0()
                .path(icon(module.icon()))
                .text_color(fg.hsla()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .flex_none()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(module.name()),
                )
                .child(
                    svg()
                        .size(px(15.))
                        .flex_shrink_0()
                        .flex_none()
                        .path(icon("alt-arrow-down-linear"))
                        .with_transformation(Transformation::rotate(radians(
                            -std::f32::consts::FRAC_PI_2 * (1. - open_t),
                        )))
                        .text_color(chevron.hsla()),
                ),
        )
        .child(module_switch(app, module, window, cx))
}

/// Pill on the group header. Clicking it does not expand or collapse the group.
fn module_switch(
    app: &mut SettingsApp,
    module: ModuleId,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> impl IntoElement {
    let palette = app.palette;
    let enabled = modules::flags(app).get(module);
    let locked = enabled && !modules::flags(app).can_disable(module);
    let on = anim::tween(
        window,
        cx,
        ("nav-module-switch", module.index()),
        enabled as u8 as f32,
        UI,
    );
    let lock_t = anim::tween(
        window,
        cx,
        ("nav-module-lock", module.index()),
        locked as u8 as f32,
        UI,
    );
    let track = lerp(palette.surface_3, palette.accent, on);
    div()
        .id(("nav-module-switch", module.index()))
        .flex_shrink_0()
        .flex_none()
        .w(px(36.))
        .h(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .when(!locked, |this| this.cursor_pointer())
        .when(locked, |this| {
            this.tooltip(|_, cx| {
                controls::tooltip("At least one module has to stay on.", cx)
            })
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |app, _, _, cx| {
            cx.stop_propagation();
            if modules::set_enabled(app, module, !enabled, cx) {
                cx.notify();
            }
        }))
        .child(
            div()
                .relative()
                .w(px(28.))
                .h(px(16.))
                .rounded_full()
                .opacity(1. - 0.5 * lock_t)
                .bg(track.hsla())
                .child(
                    div()
                        .absolute()
                        .top(px(2.))
                        .left(px(2. + 12. * on))
                        .size(px(12.))
                        .rounded_full()
                        .bg(palette.on_accent.hsla()),
                ),
        )
}

fn nav_item(
    app: &mut SettingsApp,
    tab: SettingsTab,
    nested: bool,
    dim: f32,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> gpui::Stateful<gpui::Div> {
    let palette = app.palette;
    let key = tab.order();
    let pointer = anim::pointer(window, cx, ("nav-pointer", key));
    let hovered = pointer.read(cx).hovered;
    let active = app.nav.active_tab == tab;
    let hover = anim::tween(window, cx, ("nav-hover", key), hovered as u8 as f32, UI);
    let on = anim::tween(window, cx, ("nav-active", key), active as u8 as f32, UI);

    let rest_bg = lerp(TRANSPARENT, palette.surface_color(2), hover);
    let bg = lerp(rest_bg, palette.surface_color(3), on);
    let rest_fg = lerp(palette.muted, palette.text, hover);
    let fg = lerp(rest_fg, palette.accent_text, on);
    let height = if nested { CHILD_H } else { PRIMARY_H };
    let icon_size = if nested { 18. } else { 21. };
    let pad_l = if nested { 28. } else { 12. };

    div()
        .id(("nav-item", key))
        .relative()
        .flex()
        .items_center()
        .flex_shrink_0()
        .gap(px(10.))
        .w_full()
        .h(px(height))
        .pl(px(pad_l))
        .pr(px(12.))
        .rounded(px(8.))
        .bg(bg.hsla())
        .text_color(fg.hsla())
        .text_size(px(if nested { 13. } else { 14. }))
        .font_weight(if !nested || active {
            FontWeight(600.)
        } else {
            FontWeight(500.)
        })
        .opacity(if nested { 0.5 + 0.5 * dim } else { 1. })
        .cursor_pointer()
        .on_hover(move |hovered, _, cx| anim::set_hovered(&pointer, *hovered, cx))
        .on_click(cx.listener(move |app, _, _, cx| {
            app.nav.select(tab);
            cx.notify();
        }))
        .child(icon_pair(tab.icon(), tab.active_icon(), on, fg, icon_size))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(tab.label()),
        )
}

fn icon_pair(
    linear: &'static str,
    bold: &'static str,
    on: f32,
    fg: Oklch,
    icon_size: f32,
) -> impl IntoElement {
    let line_scale = 1. - 0.5 * on;
    let filled_scale = 0.5 + 0.6 * on;
    div()
        .relative()
        .size(px(icon_size))
        .flex_shrink_0()
        .child(
            svg()
                .absolute()
                .inset_0()
                .size(px(icon_size))
                .path(icon(linear))
                .with_transformation(Transformation::scale(size(line_scale, line_scale)))
                .text_color(fg.opacity(1. - on).hsla()),
        )
        .child(
            svg()
                .absolute()
                .inset_0()
                .size(px(icon_size))
                .path(icon(bold))
                .with_transformation(Transformation::scale(size(filled_scale, filled_scale)))
                .text_color(fg.opacity(on).hsla()),
        )
}
