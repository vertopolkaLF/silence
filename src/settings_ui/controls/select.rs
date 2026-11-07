//! Floating select from `controls.rs` and `.ui-select-*` in `controls.css`.
use super::{
    SettingsApp, anim, hover_tween,
    text_input::{TextInputEvent, TextInputOptions, text_input},
    track_pointer,
};
use crate::settings_ui::{
    assets::icon,
    color::{TRANSPARENT, lerp, mix_pct},
    style::palette,
};
use gpui::{prelude::*, *};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

type Callback = Rc<dyn Fn(&mut SettingsApp, SharedString, &mut Window, &mut Context<SettingsApp>)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SelectOption {
    pub value: SharedString,
    pub label: SharedString,
    pub group_label: Option<SharedString>,
    pub detail: Option<SharedString>,
    /// Embedded asset names without `icons/` and `.svg`.
    pub icon: Option<SharedString>,
    pub end_icon: Option<SharedString>,
    pub font_family: Option<SharedString>,
}

impl SelectOption {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            group_label: None,
            detail: None,
            icon: None,
            end_icon: None,
            font_family: None,
        }
    }
    pub fn group(mut self, value: impl Into<SharedString>) -> Self {
        self.group_label = Some(value.into());
        self
    }
    pub fn detail(mut self, value: impl Into<SharedString>) -> Self {
        self.detail = Some(value.into());
        self
    }
    pub fn icon(mut self, value: impl Into<SharedString>) -> Self {
        self.icon = Some(value.into());
        self
    }
    pub fn end_icon(mut self, value: impl Into<SharedString>) -> Self {
        self.end_icon = Some(value.into());
        self
    }
    pub fn font_family(mut self, value: impl Into<SharedString>) -> Self {
        self.font_family = Some(value.into());
        self
    }
}

#[derive(Clone)]
pub(crate) struct SelectOptions {
    pub options: Vec<SelectOption>,
    pub searchable: bool,
    pub disabled: bool,
    pub show_current_detail: bool,
    /// Some sections override the original 44px minimum with a 48px trigger.
    pub trigger_height: Option<f32>,
    pub on_option_action: Option<Callback>,
}

impl SelectOptions {
    pub fn new(options: Vec<SelectOption>) -> Self {
        Self {
            options,
            searchable: false,
            disabled: false,
            show_current_detail: true,
            trigger_height: None,
            on_option_action: None,
        }
    }
    pub fn on_option_action(
        mut self,
        callback: impl Fn(&mut SettingsApp, SharedString, &mut Window, &mut Context<SettingsApp>)
        + 'static,
    ) -> Self {
        self.on_option_action = Some(Rc::new(callback));
        self
    }
}

struct State {
    bounds: Option<Bounds<Pixels>>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    query: SharedString,
    highlighted: usize,
    highlighted_child: usize,
    center_pending: bool,
    open: bool,
    closing: Option<Instant>,
    menu_height: Option<f32>,
    current: Option<SelectOption>,
    previous: Option<SelectOption>,
    swapped: Option<Instant>,
    scroll_top: bool,
    scroll_bottom: bool,
}

fn close(
    app: &mut SettingsApp,
    state: &Entity<State>,
    id: &SharedString,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) {
    if app.open_select.as_ref() == Some(id) {
        app.open_select = None;
    }
    state.update(cx, |state, cx| {
        state.query = "".into();
        state.highlighted = 0;
        cx.notify();
    });
    // Restore keyboard ownership to the trigger before unmounting the search entity.
    let focus = state.read(cx).focus.clone();
    focus.focus(window, cx);
    cx.notify();
}

fn option_copy(
    option: &SelectOption,
    detail: bool,
    current: bool,
    p: crate::settings_ui::theme::Palette,
) -> Div {
    div()
        .min_w_0()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(
            div()
                .min_w_0()
                .text_size(px(if current { 14. } else { 16. }))
                .font_weight(FontWeight(if current { 560. } else { 400. }))
                .line_height(relative(1.18))
                .text_color(p.text.hsla())
                .when(!current, |this| this.line_clamp(2))
                .when(!detail || option.detail.is_none(), |this| {
                    this.relative().top(px(-1.))
                })
                .when_some(
                    option
                        .font_family
                        .clone()
                        .filter(|font| !font.trim().is_empty()),
                    |this, font| this.font_family(SharedString::from(font.trim().to_string())),
                )
                .child(option.label.clone()),
        )
        .when(detail, |this| {
            this.when_some(option.detail.clone(), |this, detail| {
                this.child(
                    div()
                        .text_size(px(12.))
                        .line_height(relative(1.2))
                        .text_color(p.muted.hsla())
                        .text_ellipsis()
                        .child(detail),
                )
            })
        })
}

/// Select with a portal menu, native search and independent option actions.
/// `app` supplies the window-wide `open_select` arbitration; ids are SharedStrings.
/// Icon names are asset names, rather than the original CSS classes.
/// ```ignore
/// select("sound", selected, SelectOptions::new(vec![
///     SelectOption::new("bell", "Bell").group("Built in").icon("bell-linear").end_icon("play-bold"),
/// ]).on_option_action(|app, value, _, cx| { /* preview without selecting */ }),
/// app, window, cx, |app, value, _, cx| { /* apply immediately */ })
/// ```
pub(crate) fn select(
    id: impl Into<SharedString>,
    value: impl Into<SharedString>,
    options: SelectOptions,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_change: impl Fn(&mut SettingsApp, SharedString, &mut Window, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    let id = id.into();
    let value = value.into();
    let p = palette(cx);
    if options.disabled && app.open_select.as_ref() == Some(&id) {
        app.open_select = None;
    }
    let open = !options.disabled && app.open_select.as_ref() == Some(&id);
    let state = window.use_keyed_state(
        (ElementId::from(id.clone()), "select-state"),
        cx,
        |_, cx| State {
            bounds: None,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            query: "".into(),
            highlighted: 0,
            highlighted_child: 0,
            center_pending: true,
            open: false,
            closing: None,
            menu_height: None,
            current: None,
            previous: None,
            swapped: None,
            scroll_top: false,
            scroll_bottom: false,
        },
    );
    let current = options
        .options
        .iter()
        .find(|o| o.value == value)
        .or(options.options.first())
        .cloned();
    state.update(cx, |s, _| {
        if s.open != open {
            s.open = open;
            if open {
                s.closing = None;
                s.menu_height = None;
                s.center_pending = true;
            } else {
                s.closing = Some(Instant::now());
                s.query = "".into();
            }
        }
        if s.current.as_ref().map(|o| &o.value) != current.as_ref().map(|o| &o.value) {
            s.previous = s.current.take();
            s.swapped = s.previous.as_ref().map(|_| Instant::now());
        }
        s.current = current.clone();
    });
    let visibility = anim::tween(
        window,
        cx,
        (ElementId::from(id.clone()), "menu-open"),
        open as u8 as f32,
        anim::UI,
    );
    let callback: Callback = Rc::new(on_change);
    let query = state.read(cx).query.clone();
    let filtered = options
        .options
        .iter()
        .filter(|o| {
            let query = query.trim().to_ascii_lowercase();
            query.is_empty()
                || o.label.to_ascii_lowercase().contains(&query)
                || o.value.to_ascii_lowercase().contains(&query)
                || o.detail
                    .as_ref()
                    .is_some_and(|d| d.to_ascii_lowercase().contains(&query))
        })
        .cloned()
        .collect::<Vec<_>>();
    let highlighted = state
        .read(cx)
        .highlighted
        .min(filtered.len().saturating_sub(1));
    let search_state = state.clone();
    let search_id = id.clone();
    let search_callback = callback.clone();
    let search_filtered = filtered.clone();
    let search = text_input(
        (ElementId::from(id.clone()), "search"),
        query,
        TextInputOptions {
            placeholder: current
                .as_ref()
                .map(|o| o.label.clone())
                .unwrap_or_else(|| "Search".into()),
            ..Default::default()
        },
        window,
        cx,
        move |app, event, window, cx| match event {
            TextInputEvent::Change(query) => {
                search_state.update(cx, |s, cx| {
                    s.query = query;
                    s.highlighted = 0;
                    s.center_pending = true;
                    cx.notify();
                });
                cx.notify();
            }
            TextInputEvent::Down | TextInputEvent::Up => {
                if search_filtered.is_empty() {
                    return;
                }
                let down = matches!(event, TextInputEvent::Down);
                search_state.update(cx, |s, cx| {
                    s.highlighted = if down {
                        (s.highlighted + 1) % search_filtered.len()
                    } else {
                        (s.highlighted + search_filtered.len() - 1) % search_filtered.len()
                    };
                    cx.notify();
                });
                cx.notify();
            }
            TextInputEvent::Enter(_) => {
                if let Some(option) = search_filtered.get(
                    search_state
                        .read(cx)
                        .highlighted
                        .min(search_filtered.len().saturating_sub(1)),
                ) {
                    close(app, &search_state, &search_id, window, cx);
                    search_callback(app, option.value.clone(), window, cx);
                }
            }
            TextInputEvent::Escape => close(app, &search_state, &search_id, window, cx),
            _ => {}
        },
    );
    let (pointer, hover, _) = hover_tween(id.clone(), window, cx);
    let mut trigger = div()
        .id((ElementId::from(id.clone()), "trigger"))
        .relative()
        .w_full()
        .min_h(px(44.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(14.))
        .pt(px(10.))
        .pb(px(10.))
        .pl(px(14.))
        .pr(px(12.))
        .border_1()
        .border_color(TRANSPARENT.hsla())
        .rounded(px(12.))
        .bg(lerp(
            p.surface_2,
            p.surface_3,
            if options.disabled {
                0.
            } else {
                hover.max(visibility)
            },
        )
        .hsla())
        .text_color(p.text.hsla())
        .track_focus(&state.read(cx).focus)
        .focus_visible(move |style| super::focus_outline(style, p.focus.hsla()))
        .when_some(options.trigger_height, |this, height| {
            this.h(px(height)).min_h(px(height))
        })
        .when(!options.disabled, |this| this.cursor_pointer())
        .when(options.disabled, |this| this.cursor_not_allowed())
        .when(!options.disabled, |this| {
            this.shadow(vec![
                BoxShadow {
                    color: p.highlight_line.opacity(visibility).hsla(),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(0.),
                    spread_radius: px(0.),
                    inset: true,
                },
                BoxShadow {
                    color: p.accent.opacity(0.2 * visibility).hsla(),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(1.),
                    inset: false,
                },
                BoxShadow {
                    color: p.shadow_faint.opacity(visibility).hsla(),
                    offset: point(px(0.), px(8.)),
                    blur_radius: px(18.),
                    spread_radius: px(0.),
                    inset: false,
                },
            ])
        });
    if options.searchable && open {
        trigger = trigger.child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(14.))
                .font_weight(FontWeight(560.))
                .child(search.clone()),
        );
    } else {
        let s = state.read(cx);
        let elapsed = s.swapped.map(|t| t.elapsed().as_secs_f32());
        let entering = elapsed
            .map(|t| anim::Easing::Ease.apply(((t - 0.055) / 0.24).clamp(0., 1.)))
            .unwrap_or(1.);
        let exiting = elapsed
            .map(|t| anim::Easing::Ease.apply((t / 0.22).clamp(0., 1.)))
            .unwrap_or(1.);
        let icon_enter = elapsed
            .map(|t| anim::Easing::Ease.apply(((t - 0.055) / 0.22).clamp(0., 1.)))
            .unwrap_or(1.);
        let icon_exit = elapsed
            .map(|t| anim::Easing::Ease.apply((t / 0.2).clamp(0., 1.)))
            .unwrap_or(1.);
        if elapsed.is_some_and(|t| t < 0.31) {
            window.request_animation_frame();
        }
        let mut content = div().flex_1().min_w_0().flex().items_center().gap(px(12.));
        let previous = s.previous.clone();
        if current.as_ref().and_then(|o| o.icon.as_ref()).is_some()
            || (exiting < 1. && previous.as_ref().and_then(|o| o.icon.as_ref()).is_some())
        {
            content = content.child(
                div()
                    .relative()
                    .size(px(18.))
                    .flex_shrink_0()
                    .when_some(
                        previous
                            .as_ref()
                            .and_then(|o| o.icon.clone())
                            .filter(|_| icon_exit < 1.),
                        |this, name| {
                            this.child(
                                svg()
                                    .absolute()
                                    .path(icon(&name))
                                    .size(px(18.))
                                    .text_color(p.accent_text.opacity(1. - icon_exit).hsla())
                                    .with_transformation(Transformation::scale(size(
                                        1. - icon_exit * 0.5,
                                        1. - icon_exit * 0.5,
                                    ))),
                            )
                        },
                    )
                    .when_some(
                        current.as_ref().and_then(|o| o.icon.clone()),
                        |this, name| {
                            this.child(
                                svg()
                                    .path(icon(&name))
                                    .size(px(18.))
                                    .text_color(p.accent_text.opacity(icon_enter).hsla())
                                    .with_transformation(Transformation::scale(size(
                                        0.5 + icon_enter * 0.5,
                                        0.5 + icon_enter * 0.5,
                                    ))),
                            )
                        },
                    ),
            );
        }
        content = content.child(
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .when_some(previous.filter(|_| exiting < 1.), |this, old| {
                    this.child(
                        option_copy(&old, options.show_current_detail, true, p)
                            .absolute()
                            .top(px(-14. * exiting))
                            .opacity(1. - exiting),
                    )
                })
                .when_some(current.clone(), |this, current| {
                    this.child(
                        option_copy(&current, options.show_current_detail, true, p)
                            .relative()
                            .top(px(14. * (1. - entering)))
                            .opacity(entering),
                    )
                }),
        );
        trigger = trigger.child(content);
    }
    trigger = trigger.child(
        svg()
            .path(icon("alt-arrow-down-linear"))
            .size(px(17.))
            .flex_shrink_0()
            .text_color(lerp(p.faint, p.accent_text, visibility).hsla())
            .with_transformation(Transformation::rotate(gpui::Radians(
                std::f32::consts::PI * visibility,
            ))),
    );
    let click_id = id.clone();
    let click_state = state.clone();
    let click_search = search.clone();
    let selected_index = options
        .options
        .iter()
        .position(|o| o.value == value)
        .unwrap_or(0);
    let disabled = options.disabled;
    let searchable = options.searchable;
    let key_id = id.clone();
    let key_state = state.clone();
    let key_callback = callback.clone();
    let key_search = search.clone();
    let key_filtered = filtered.clone();
    let trigger = track_pointer(trigger, pointer)
        .on_key_down(cx.listener(move |app, event: &KeyDownEvent, window, cx| {
            if disabled {
                return;
            }
            let opened = app.open_select.as_ref() == Some(&key_id);
            match event.keystroke.key.as_str() {
                "escape" if opened => close(app, &key_state, &key_id, window, cx),
                "enter" | "space" if !opened => {
                    app.open_select = Some(key_id.clone());
                    key_state.update(cx, |s, cx| {
                        s.query = "".into();
                        s.highlighted = selected_index;
                        cx.notify();
                    });
                    if searchable {
                        key_search.update(cx, |input, cx| input.focus_and_select_all(window, cx));
                    }
                    cx.notify();
                }
                "enter" | "space" if opened => {
                    if let Some(option) = key_filtered.get(
                        key_state
                            .read(cx)
                            .highlighted
                            .min(key_filtered.len().saturating_sub(1)),
                    ) {
                        close(app, &key_state, &key_id, window, cx);
                        key_callback(app, option.value.clone(), window, cx);
                    }
                }
                "up" | "down" if opened && !key_filtered.is_empty() => {
                    key_state.update(cx, |s, cx| {
                        s.highlighted = if event.keystroke.key == "down" {
                            (s.highlighted + 1) % key_filtered.len()
                        } else {
                            (s.highlighted + key_filtered.len() - 1) % key_filtered.len()
                        };
                        cx.notify();
                    });
                    cx.notify();
                }
                _ => return,
            }
            window.prevent_default();
            cx.stop_propagation();
        }))
        .on_click(cx.listener(move |app, _, window, cx| {
            if disabled || (searchable && app.open_select.as_ref() == Some(&click_id)) {
                return;
            }
            if app.open_select.as_ref() == Some(&click_id) {
                close(app, &click_state, &click_id, window, cx);
            } else {
                app.open_select = Some(click_id.clone());
                click_state.update(cx, |s, cx| {
                    s.query = "".into();
                    s.highlighted = selected_index;
                    cx.notify();
                });
                if searchable {
                    click_search.update(cx, |input, cx| {
                        input.set_value("", cx);
                        input.focus_and_select_all(window, cx);
                    });
                } else {
                    let focus = click_state.read(cx).focus.clone();
                    focus.focus(window, cx);
                }
                cx.notify();
            }
        }));
    let bounds_state = state.clone();
    let mut root = div()
        .relative()
        .w_full()
        .min_w_0()
        .opacity(if disabled { 0.45 } else { 1. })
        .on_children_prepainted(move |bounds, window, cx| {
            if let Some(bounds) = bounds.first().copied() {
                let changed = bounds_state.read(cx).bounds != Some(bounds);
                if changed {
                    bounds_state.update(cx, |s, _| s.bounds = Some(bounds));
                    window.request_animation_frame();
                }
            }
        })
        .id(id.clone())
        .child(trigger);
    let dismiss_state = state.clone();
    let dismiss_id = id.clone();
    // Outside listener does not consume the click: another select can open on the same click.
    root = root.on_mouse_down_out(cx.listener(move |app, event: &MouseDownEvent, window, cx| {
        let s = dismiss_state.read(cx);
        let bounds = s.scroll.bounds();
        if app.open_select.as_ref() == Some(&dismiss_id) && !bounds.contains(&event.position) {
            close(app, &dismiss_state, &dismiss_id, window, cx);
        }
    }));
    let show_menu = open
        || state
            .read(cx)
            .closing
            .is_some_and(|t| t.elapsed() < Duration::from_millis(220));
    if !open && show_menu {
        window.request_animation_frame();
    }
    if show_menu && !disabled {
        if let Some(bounds) = state.read(cx).bounds {
            root = root.child(menu(
                &id,
                &value,
                &options,
                filtered,
                highlighted,
                bounds,
                visibility,
                open,
                state,
                callback,
                window,
                cx,
            ));
        }
    }
    root
}

fn menu(
    id: &SharedString,
    value: &SharedString,
    options: &SelectOptions,
    filtered: Vec<SelectOption>,
    highlighted: usize,
    bounds: Bounds<Pixels>,
    visibility: f32,
    open: bool,
    state: Entity<State>,
    callback: Callback,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> AnyElement {
    let p = palette(cx);
    let viewport = window.viewport_size();
    let mut rows = Vec::<AnyElement>::new();
    let mut group = None;
    let mut selected_child = 0;
    let mut desired_height: f32 = 12.;
    for (index, option) in filtered.into_iter().enumerate() {
        if option.group_label != group {
            group = option.group_label.clone();
            if let Some(group) = &group {
                desired_height += 27.;
                rows.push(
                    div()
                        .flex_shrink_0()
                        .mx(px(6.))
                        .pt(px(8.))
                        .pb(px(2.))
                        .px(px(10.))
                        .text_color(p.faint.hsla())
                        .text_size(px(11.))
                        .font_weight(FontWeight(700.))
                        .child(group.to_uppercase())
                        .into_any_element(),
                );
            }
        }
        if index == highlighted {
            selected_child = rows.len();
        }
        desired_height += if option.detail.is_some() { 53. } else { 48. };
        let selected = option.value == *value;
        let key = ElementId::from((ElementId::from(id.clone()), option.value.clone()));
        let (pointer, hover, _) = hover_tween(key.clone(), window, cx);
        let highlight = anim::tween(
            window,
            cx,
            (key.clone(), "highlight"),
            (index == highlighted) as u8 as f32,
            anim::UI,
        )
        .max(hover);
        let bg = if selected {
            mix_pct(p.accent, 14. + 8. * highlight, p.surface_2)
        } else {
            lerp(TRANSPARENT, p.surface_3, highlight)
        };
        let choose_state = state.clone();
        let choose_id = id.clone();
        let choose_value = option.value.clone();
        let choose_callback = callback.clone();
        let mut row = track_pointer(
            div()
                .id(key.clone())
                .flex_shrink_0()
                .mx(px(6.))
                .min_h(px(44.))
                .flex()
                .items_center()
                .gap(px(12.))
                .rounded(px(10.))
                .bg(bg.hsla())
                .text_color(lerp(p.muted, p.text, highlight).hsla())
                .child(
                    div()
                        .id((key.clone(), "choose"))
                        .flex_1()
                        .min_w_0()
                        .min_h(px(44.))
                        .py(px(8.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .cursor_pointer()
                        .when_some(option.icon.clone(), |this, name| {
                            this.child(
                                svg()
                                    .path(icon(&name))
                                    .size(px(18.))
                                    .flex_shrink_0()
                                    .text_color(p.accent_text.hsla()),
                            )
                        })
                        .child(option_copy(&option, true, false, p))
                        .on_click(cx.listener(move |app, _, window, cx| {
                            if !open {
                                return;
                            }
                            close(app, &choose_state, &choose_id, window, cx);
                            choose_callback(app, choose_value.clone(), window, cx);
                        })),
                ),
            pointer,
        );
        if let Some(name) = option.end_icon {
            if let Some(action) = options.on_option_action.clone() {
                let action_value = option.value.clone();
                let (pointer, hover, _) = hover_tween((key.clone(), "action"), window, cx);
                row = row.child(
                    track_pointer(
                        div()
                            .id((key, "action"))
                            .size(px(28.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.))
                            .cursor_pointer()
                            .bg(lerp(TRANSPARENT, p.surface_color[1], hover).hsla())
                            .child(
                                svg()
                                    .path(icon(&name))
                                    .size(px(15.))
                                    .text_color(lerp(p.muted, p.text, hover).hsla()),
                            ),
                        pointer,
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        if open {
                            action(app, action_value.clone(), window, cx);
                        }
                    })),
                );
            } else {
                row = row.child(
                    svg()
                        .path(icon(&name))
                        .size(px(15.))
                        .flex_shrink_0()
                        .text_color(lerp(p.muted, p.text, highlight).hsla()),
                );
            }
        }
        rows.push(row.into_any_element());
    }
    if rows.is_empty() {
        desired_height = 52.;
        rows.push(
            div()
                .mx(px(6.))
                .px(px(10.))
                .py(px(12.))
                .text_size(px(13.))
                .text_color(p.muted.hsla())
                .child("No results")
                .into_any_element(),
        );
    }
    let above = (f32::from(bounds.top()) - 20.).max(0.);
    let below = (f32::from(viewport.height - bounds.bottom()) - 20.).max(0.);
    let desired_height = state
        .read(cx)
        .menu_height
        .unwrap_or(desired_height.min(280.));
    let down = below >= desired_height || (below >= desired_height.min(140.) && below >= above);
    let height = desired_height.min(if down { below } else { above }).max(1.);
    let width = f32::from(bounds.size.width).min(f32::from(viewport.width) - 24.);
    let left =
        f32::from(bounds.left()).clamp(12., (f32::from(viewport.width) - 12. - width).max(12.));
    let top = if down {
        f32::from(bounds.bottom()) + 8.
    } else {
        f32::from(bounds.top()) - 8. - height
    };
    let scroll = state.read(cx).scroll.clone();
    state.update(cx, |s, _| {
        if s.highlighted_child != selected_child {
            s.center_pending = true;
        }
        s.highlighted_child = selected_child;
    });
    let show_top = state.read(cx).scroll_top as u8 as f32;
    let show_bottom = state.read(cx).scroll_bottom as u8 as f32;
    let top_shadow = anim::tween(
        window,
        cx,
        (ElementId::from(id.clone()), "shadow-top"),
        show_top,
        anim::Transition::new(180, anim::Easing::Ease),
    );
    let bottom_shadow = anim::tween(
        window,
        cx,
        (ElementId::from(id.clone()), "shadow-bottom"),
        show_bottom,
        anim::Transition::new(180, anim::Easing::Ease),
    );
    let measured = state.clone();
    let shadow_scroll = scroll.clone();
    let measure = canvas(
        |_, _, _| (),
        move |_, _, window, cx| {
            if measured.read(cx).center_pending {
                let index = measured.read(cx).highlighted_child;
                if let Some(child) = shadow_scroll.bounds_for_item(index) {
                    let viewport = shadow_scroll.bounds();
                    let offset = shadow_scroll.offset();
                    if child.top() + offset.y < viewport.top()
                        || child.bottom() + offset.y > viewport.bottom()
                    {
                        let target = viewport.top()
                            + (viewport.size.height - child.size.height) / 2.
                            - child.top();
                        shadow_scroll.set_offset(point(
                            offset.x,
                            target.clamp(-shadow_scroll.max_offset().y, px(0.)),
                        ));
                        window.request_animation_frame();
                    }
                    measured.update(cx, |s, _| s.center_pending = false);
                }
            }
            let offset = -f32::from(shadow_scroll.offset().y);
            let max = f32::from(shadow_scroll.max_offset().y);
            let (top, bottom) = (max > 1. && offset > 1., max > 1. && offset < max - 1.);
            if measured.read(cx).scroll_top != top || measured.read(cx).scroll_bottom != bottom {
                measured.update(cx, |s, _| {
                    s.scroll_top = top;
                    s.scroll_bottom = bottom;
                });
                window.request_animation_frame();
            }
        },
    )
    .absolute()
    .size_full();
    let content_state = state.clone();
    let menu = div()
        .id((ElementId::from(id.clone()), "menu"))
        .relative()
        .w(px(width))
        .h(px(height))
        .rounded(px(14.))
        .border_1()
        .border_color(TRANSPARENT.hsla())
        .bg(p.select_menu_bg.hsla())
        .opacity(visibility)
        .top(px((if down { -6. } else { 6. }) * (1. - visibility)))
        .shadow(vec![BoxShadow {
            color: p.shadow_medium.hsla(),
            offset: point(px(0.), px(18.)),
            blur_radius: px(36.),
            spread_radius: px(0.),
            inset: false,
        }])
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .size_full()
                .py(px(6.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .on_children_prepainted(move |children, window, cx| {
                    if content_state.read(cx).menu_height.is_none() {
                        let measured = children
                            .first()
                            .zip(children.last())
                            .map(|(first, last)| f32::from(last.bottom() - first.top()) + 12.)
                            .unwrap_or(52.);
                        content_state.update(cx, |s, _| s.menu_height = Some(measured.min(280.)));
                        window.request_animation_frame();
                    }
                })
                .id((ElementId::from(id.clone()), "list"))
                .overflow_y_scroll()
                .track_scroll(&scroll)
                .children(rows),
        )
        .child(
            div()
                .absolute()
                .top(px(0.))
                .w_full()
                .h(px(40.))
                .rounded_t(px(14.))
                .opacity(top_shadow)
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(p.accent.opacity(0.14).hsla(), 0.),
                    linear_color_stop(TRANSPARENT.hsla(), 1.),
                )),
        )
        .child(
            div()
                .absolute()
                .bottom(px(0.))
                .w_full()
                .h(px(40.))
                .rounded_b(px(14.))
                .opacity(bottom_shadow)
                .bg(linear_gradient(
                    0.,
                    linear_color_stop(p.accent.opacity(0.14).hsla(), 0.),
                    linear_color_stop(TRANSPARENT.hsla(), 1.),
                )),
        )
        .child(measure);
    deferred(anchored().position(point(px(left), px(top))).child(menu))
        .with_priority(100)
        .into_any_element()
}
