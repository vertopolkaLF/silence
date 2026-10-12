//! The content area (`assets/styles/layout.css`): sliding tab panels, the
//! scroll container with a thin scrollbar, and active sub-section tracking.
use super::nav::{SettingsTab, TabSlideDirection};
use crate::settings_ui::{
    SettingsApp,
    anim::{self, Easing, Transition, cubic_bezier},
    sections,
};
use gpui::{
    AnyElement, BoxShadow, Context, InteractiveElement, IntoElement, ParentElement, Pixels,
    ScrollHandle, Styled, Window, div, point, prelude::*, px,
};
use std::time::Instant;

/// `[data-settings-section] { scroll-margin-top: 50px }`.
const SCROLL_MARGIN_TOP: f32 = 50.;
const SMOOTH_SCROLL_MS: f32 = 450.;
/// Width reserved by `scrollbar-gutter: stable` with a thin scrollbar.
const SCROLLBAR_GUTTER: f32 = 10.;

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> impl IntoElement {
    let palette = app.palette;
    let animating = app.nav.tick();
    if animating {
        window.request_animation_frame();
    }
    // A hotkey editor requested from another tab opens once Hotkeys has slid in.
    if app.nav.transition.is_none() && app.nav.displayed_tab == SettingsTab::Hotkeys {
        if let Some(request) = app.pending_hotkey_modal_after_nav.take() {
            app.hotkey_modal_request = Some(request);
        }
    }
    apply_smooth_scroll(app, window);
    track_active_section(app);

    let mut content = div()
        .id("content")
        .relative()
        .flex_1()
        .h_full()
        .min_w_0()
        .overflow_hidden()
        .bg(palette.content_bg.hsla())
        .rounded_tl(px(16.))
        .rounded_bl(px(16.))
        .shadow(vec![BoxShadow {
            color: palette.highlight_line.hsla(),
            offset: point(px(-1.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
            inset: false,
        }]);

    if let Some((transition, t)) = app.nav.progress() {
        // `--tab-transition-ease: cubic-bezier(1, 0.1, 0, 1)` over 300ms.
        let eased = cubic_bezier(1., 0.1, 0., 1., t);
        let width = window.viewport_size().width.as_f32() - 240.;
        let (out_x, in_x) = match transition.direction {
            TabSlideDirection::Left => (-eased, 1. - eased),
            TabSlideDirection::Right => (eased, eased - 1.),
        };
        let outgoing = app.nav.outgoing_scroll.clone();
        let incoming = app.nav.scroll.clone();
        content = content
            .child(panel(app, transition.from, &outgoing, false, out_x * width, window, cx))
            .child(panel(app, transition.to, &incoming, false, in_x * width, window, cx));
    } else {
        let scroll = app.nav.scroll.clone();
        let tab = app.nav.displayed_tab;
        content = content.child(panel(app, tab, &scroll, true, 0., window, cx));
    }
    content
}

fn panel(
    app: &mut SettingsApp,
    tab: SettingsTab,
    scroll: &ScrollHandle,
    resting: bool,
    offset_x: f32,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> AnyElement {
    let palette = app.palette;
    let blocks = sections::render(app, tab, window, cx);
    // The thumb fades in once the panel rests (`scrollbar-color` transition).
    let thumb_alpha = anim::tween(
        window,
        cx,
        ("scroll-thumb", tab.order()),
        resting as u8 as f32,
        Transition::new(200, Easing::Ease),
    );
    let thumb = scrollbar_thumb(scroll, window).map(|(top, height)| {
        div()
            .absolute()
            .right(px(2.))
            .top(px(top))
            .w(px(6.))
            .h(px(height))
            .rounded_full()
            .bg(palette.surface_color(5).opacity(thumb_alpha).hsla())
    });

    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(offset_x))
        .w_full()
        .child(
            div()
                .id(("content-scroll", tab.order()))
                .size_full()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .pr(px(SCROLLBAR_GUTTER))
                .child({
                    let page = div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .min_h_full()
                        .px(px(38.))
                        .py(px(44.))
                        .children(blocks);
                    if resting {
                        page.on_children_prepainted(|bounds, _, _| {
                            record_section_tops(bounds.iter().map(|b| b.origin.y).collect())
                        })
                    } else {
                        page
                    }
                }),
        )
        .children(thumb)
        .into_any_element()
}

/// Thumb geometry for a vertical scroll container, or `None` without overflow.
fn scrollbar_thumb(scroll: &ScrollHandle, _window: &Window) -> Option<(f32, f32)> {
    let viewport = scroll.bounds().size.height.as_f32();
    let max = scroll.max_offset().y.as_f32();
    if viewport <= 0. || max <= 0.5 {
        return None;
    }
    let content = viewport + max;
    let height = (viewport * viewport / content).max(24.);
    let progress = (-scroll.offset().y.as_f32() / max).clamp(0., 1.);
    Some((progress * (viewport - height), height))
}

fn track_active_section(app: &mut SettingsApp) {
    if app.nav.transition.is_some() {
        return;
    }
    let sections = app.nav.displayed_tab.sections();
    if sections.len() < 2 {
        return;
    }
    let scroll = &app.nav.scroll;
    let viewport = scroll.bounds().size.height.as_f32();
    let offset = -scroll.offset().y.as_f32();
    let activation = offset + viewport * 0.35;
    let tops = section_tops(scroll);
    let mut active = sections[0].id;
    for (section, top) in sections.iter().zip(tops) {
        if top <= activation {
            active = section.id;
        }
    }
    app.nav.active_section = active;
}

/// Content-relative tops of the section blocks inside the page column.
fn section_tops(scroll: &ScrollHandle) -> Vec<f32> {
    SECTION_TOPS.with(|tops| tops.borrow().clone())
        .into_iter()
        .map(|top| top - scroll.bounds().origin.y.as_f32() - scroll.offset().y.as_f32())
        .collect()
}

thread_local! {
    /// Window-space tops of the current page's section blocks, recorded during
    /// prepaint by [`crate::settings_ui::shell::content::record_section_tops`].
    static SECTION_TOPS: std::cell::RefCell<Vec<f32>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub(crate) fn record_section_tops(tops: Vec<Pixels>) {
    SECTION_TOPS.with(|cell| *cell.borrow_mut() = tops.into_iter().map(|top| top.as_f32()).collect());
}

/// Sub-section click: smooth-scroll the section to the top of the viewport.
pub(crate) fn scroll_to_section(
    app: &mut SettingsApp,
    id: &'static str,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) {
    let sections = app.nav.displayed_tab.sections();
    let Some(index) = sections.iter().position(|section| section.id == id) else {
        return;
    };
    let scroll = &app.nav.scroll;
    let tops = section_tops(scroll);
    let Some(top) = tops.get(index).copied() else {
        return;
    };
    let max = scroll.max_offset().y.as_f32();
    let current = -scroll.offset().y.as_f32();
    let target = (top - SCROLL_MARGIN_TOP).clamp(0., max);
    app.nav.smooth_scroll = Some((current, target, Instant::now()));
    window.request_animation_frame();
    cx.notify();
}

fn apply_smooth_scroll(app: &mut SettingsApp, window: &mut Window) {
    let Some((from, to, started)) = app.nav.smooth_scroll else {
        return;
    };
    let t = (started.elapsed().as_secs_f32() * 1000. / SMOOTH_SCROLL_MS).min(1.);
    let y = from + (to - from) * Easing::EaseInOut.apply(t);
    app.nav.scroll.set_offset(point(px(0.), px(-y)));
    if t >= 1. {
        app.nav.smooth_scroll = None;
    } else {
        window.request_animation_frame();
    }
}
