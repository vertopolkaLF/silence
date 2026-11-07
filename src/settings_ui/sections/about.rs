//! About (`src/gui/sections/about.rs`, `assets/styles/about.css`).
use crate::settings_ui::{
    SettingsApp,
    anim::{self, Easing, Transition},
    assets::icon,
    color::{Oklch, TRANSPARENT, lerp, mix_pct},
    controls, live, style,
};
use gpui::{
    AnyElement, App, BoxShadow, Context, Div, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Stateful, Styled, Transformation, Window, div, img, point, prelude::*, px,
    radians, relative, svg,
};
use std::time::Instant;
use windows::{
    Win32::Globalization::{GetLocaleInfoEx, GetTimeFormatEx, LOCALE_SSHORTTIME, TIME_NOSECONDS},
    core::PCWSTR,
};

const GITHUB: &str = "https://github.com/vertopolkaLF/silence";
const RELEASES: &str = "https://github.com/vertopolkaLF/silence/releases";
const ISSUES: &str = "https://github.com/vertopolkaLF/silence/issues";
const LAYER: Transition = Transition::new(400, Easing::Bezier(0.4, 0., 0.2, 1.));
const BUTTON: Transition = Transition::new(300, Easing::Bezier(0.4, 0., 0.2, 1.));

#[derive(Clone, Default)]
enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpToDate(String),
    Available(crate::updater::UpdateInfo),
    Downloading(f32),
    Installing,
    Failed(String),
}

#[derive(Default)]
pub(crate) struct State {
    update: UpdateState,
    checking_started: Option<Instant>,
}

impl State {
    fn busy(&self) -> bool {
        matches!(
            self.update,
            UpdateState::Checking | UpdateState::Downloading(_) | UpdateState::Installing
        )
    }
}

/// The passive startup check must not overwrite a newer user-initiated flow.
/// Like the old parent signal, a passive up-to-date result leaves About idle.
pub(crate) fn background_check_completed(
    app: &mut SettingsApp,
    result: anyhow::Result<crate::updater::UpdateCheck>,
) {
    if !matches!(app.sections.about.update, UpdateState::Idle) {
        return;
    }
    match result {
        Ok(crate::updater::UpdateCheck::Available(update)) => {
            set_update(app, UpdateState::Available(update))
        }
        Ok(crate::updater::UpdateCheck::UpToDate) => app.available_update = None,
        Err(error) => eprintln!("settings update check failed: {error:?}"),
    }
}

fn set_update(app: &mut SettingsApp, state: UpdateState) {
    app.available_update = match &state {
        UpdateState::Available(update) => Some(update.clone()),
        _ => None,
    };
    app.sections.about.update = state;
}

/// Manual check, or check-and-install for `--about-update` / the titlebar pill.
pub(crate) fn check(app: &mut SettingsApp, install: bool, cx: &mut Context<SettingsApp>) {
    if app.sections.about.busy() {
        return;
    }
    set_update(app, UpdateState::Checking);
    app.sections.about.checking_started = Some(Instant::now());
    let (sender, receiver) = flume::bounded(1);
    live::runtime().spawn(async move {
        let _ = sender.send_async(crate::check_for_update().await).await;
    });
    cx.spawn(async move |this, cx| {
        let Ok(result) = receiver.recv_async().await else {
            return;
        };
        let _ = this.update(cx, |app, cx| {
            app.sections.about.checking_started = None;
            match result {
                Ok(crate::updater::UpdateCheck::Available(update)) if install => {
                    download(app, update, cx)
                }
                Ok(crate::updater::UpdateCheck::Available(update)) => {
                    set_update(app, UpdateState::Available(update))
                }
                Ok(crate::updater::UpdateCheck::UpToDate) => {
                    set_update(app, UpdateState::UpToDate(current_check_time()))
                }
                Err(error) => set_update(app, UpdateState::Failed(error.to_string())),
            }
            cx.notify();
        });
    })
    .detach();
    cx.notify();
}

fn download(
    app: &mut SettingsApp,
    update: crate::updater::UpdateInfo,
    cx: &mut Context<SettingsApp>,
) {
    set_update(app, UpdateState::Downloading(0.));
    let (sender, receiver) = flume::unbounded();
    live::runtime().spawn(async move {
        let progress_sender = sender.clone();
        // The shared helper persists the release notification, launches the
        // installer, exits the other app processes, then exits this process.
        let install_sender = sender.clone();
        let result = crate::download_and_install_update_with_status(
            update,
            move |progress| {
                let _ = progress_sender.send(UpdateState::Downloading(progress));
            },
            move || {
                let _ = install_sender.send(UpdateState::Installing);
            },
        )
        .await;
        let state = match result {
            Ok(()) => UpdateState::Installing,
            Err(error) => UpdateState::Failed(error.to_string()),
        };
        let _ = sender.send(state);
    });
    cx.spawn(async move |this, cx| {
        while let Ok(first) = receiver.recv_async().await {
            // Chunk callbacks can be much faster than the render loop.
            let state = receiver.try_iter().last().unwrap_or(first);
            if this
                .update(cx, |app, cx| {
                    set_update(app, state);
                    cx.notify();
                })
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
    cx.notify();
}

/// Windows short-time pattern, padded like JS hour/minute: '2-digit'.
/// Preserve the user's separators, clock convention and localized AM/PM.
fn current_check_time() -> String {
    let mut pattern = [0_u16; 128];
    let count = unsafe { GetLocaleInfoEx(PCWSTR::null(), LOCALE_SSHORTTIME, Some(&mut pattern)) };
    let format = if count > 1 {
        let pattern = String::from_utf16_lossy(&pattern[..count as usize - 1]);
        let mut chars = pattern.chars().peekable();
        let mut quoted = false;
        let mut padded = String::new();
        while let Some(ch) = chars.next() {
            if ch == '\'' {
                quoted = !quoted;
            }
            padded.push(ch);
            if !quoted && matches!(ch, 'h' | 'H' | 'm') {
                if chars.peek() == Some(&ch) {
                    while chars.peek() == Some(&ch) {
                        chars.next();
                    }
                }
                padded.push(ch);
            }
        }
        Some(padded.encode_utf16().chain(Some(0)).collect::<Vec<_>>())
    } else {
        None
    };
    let mut text = [0_u16; 128];
    let count = unsafe {
        GetTimeFormatEx(
            PCWSTR::null(),
            TIME_NOSECONDS,
            None,
            format
                .as_ref()
                .map_or(PCWSTR::null(), |s| PCWSTR(s.as_ptr())),
            Some(&mut text),
        )
    };
    if count > 1 {
        String::from_utf16_lossy(&text[..count as usize - 1])
    } else {
        "now".into()
    }
}

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let palette = app.palette;
    let hero =
        div()
            .flex()
            .items_center()
            .gap(px(20.))
            .child(
                div()
                    .size(px(66.))
                    .flex_shrink_0()
                    .rounded(px(16.))
                    .shadow(vec![shadow(
                        mix_pct(palette.shadow_medium, 60., TRANSPARENT).hsla(),
                        10.,
                        24.,
                    )])
                    .child(img("app.png").size(px(66.)).rounded(px(16.))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_wrap()
                            .gap_x(px(12.))
                            .gap_y(px(10.))
                            .child(style::h1("silence!"))
                            .child(
                                div()
                                    .h(px(26.))
                                    .px(px(12.))
                                    .flex()
                                    .items_center()
                                    .rounded_full()
                                    .border_1()
                                    .border_color(TRANSPARENT.hsla())
                                    .bg(palette.accent_soft.hsla())
                                    .text_color(palette.accent_text.hsla())
                                    .text_size(px(12.))
                                    .font_weight(FontWeight(600.))
                                    .child(crate::updater::current_version_text()),
                            ),
                    )
                    .child(div().text_size(px(14.)).line_height(relative(1.5)).child(
                        "A simple microphone mute/unmute utility with global hotkey support.",
                    )),
            );
    let state = app.sections.about.update.clone();
    let status: String = match &state {
        UpdateState::Idle => "Ready to check for updates.".into(),
        UpdateState::Checking => "Checking updates...".into(),
        UpdateState::UpToDate(time) => format!("No updates found at {time}"),
        UpdateState::Available(update) => format!("{} is available.", update.version),
        UpdateState::Downloading(progress) => {
            format!("Downloading update... {}%", progress.round() as u32)
        }
        UpdateState::Installing => "Launching installer...".into(),
        UpdateState::Failed(error) => format!("Update failed: {error}"),
    };
    let highlighted = matches!(
        state,
        UpdateState::Available(_) | UpdateState::Downloading(_) | UpdateState::Installing
    );
    let failed = matches!(state, UpdateState::Failed(_));
    let highlight = anim::tween(
        window,
        cx,
        "about-status-highlight",
        highlighted as u8 as f32,
        anim::UI,
    );
    let error = anim::tween(
        window,
        cx,
        "about-status-error",
        failed as u8 as f32,
        anim::UI,
    );
    let status_color = lerp(
        lerp(palette.muted, palette.accent, highlight),
        Oklch::from_srgb8(0xd9, 0x2d, 0x20),
        error,
    );
    let updates = card("Updates", "refresh-linear", cx)
        .child(
            div()
                .text_size(px(13.))
                .line_height(relative(1.25))
                .font_weight(FontWeight(if highlighted || failed { 600. } else { 500. }))
                .text_color(status_color.hsla())
                .child(status),
        )
        .child(update_controls(app, &state, window, cx));
    let resources = card("Resources", "info-circle-linear", cx).child(
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(resource(
                        "about-github",
                        "github",
                        "GitHub",
                        "View source code",
                        GITHUB,
                        window,
                        cx,
                    ))
                    .child(resource(
                        "about-changelog",
                        "bill-list-bold",
                        "Changelog",
                        "See what's new",
                        RELEASES,
                        window,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(resource(
                        "about-issues",
                        "bug-bold",
                        "Report Issue",
                        "Found a bug?",
                        ISSUES,
                        window,
                        cx,
                    ))
                    .child(resource(
                        "about-author",
                        "user-bold",
                        "Author",
                        "@vertopolkaLF",
                        "https://github.com/vertopolkaLF",
                        window,
                        cx,
                    )),
            ),
    );
    vec![
        div()
            .id("about-overview")
            .w_full()
            .max_w(px(720.))
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(hero)
            .child(updates)
            .child(resources)
            .into_any_element(),
    ]
}

fn card(title: &'static str, name: &'static str, cx: &App) -> Div {
    let palette = style::palette(cx);
    div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .py(px(18.))
        .px(px(20.))
        .rounded(px(14.))
        .border_1()
        .border_color(TRANSPARENT.hsla())
        .bg(palette.surface_2.hsla())
        .shadow(vec![shadow(palette.shadow_faint.hsla(), 8., 26.)])
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    svg()
                        .path(icon(name))
                        .size(px(20.))
                        .flex_shrink_0()
                        .text_color(palette.accent_text.hsla()),
                )
                .child(style::h2(title)),
        )
}

fn shadow(color: gpui::Hsla, y: f32, blur: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.),
        inset: false,
    }
}

fn action_surface(
    id: &'static str,
    spec: Transition,
    enabled: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Stateful<Div> {
    let palette = style::palette(cx);
    let pointer = anim::pointer(window, cx, (gpui::ElementId::from(id), "pointer"));
    let hovered = enabled && pointer.read(cx).hovered;
    let hover = anim::tween(
        window,
        cx,
        (gpui::ElementId::from(id), "hover"),
        hovered as u8 as f32,
        spec,
    );
    let element = div()
        .id(id)
        .relative()
        .top(px(-hover))
        .flex_1()
        .min_w_0()
        .rounded(px(12.))
        .text_color(palette.text.hsla())
        .bg(lerp(
            mix_pct(palette.accent, 6., palette.surface_2),
            mix_pct(palette.accent, 10., palette.surface_2),
            hover,
        )
        .hsla())
        .shadow(vec![shadow(
            palette.shadow_faint.opacity(hover).hsla(),
            8.,
            20.,
        )])
        .when(enabled, |this| this.cursor_pointer());
    // Exiting layers are visible for 400ms, but must already stop taking input
    // so they cannot cover a newly active layer (CSS pointer-events: none).
    if enabled {
        controls::track_pointer(element, pointer)
    } else {
        element
    }
}

fn resource(
    id: &'static str,
    name: &'static str,
    title: &'static str,
    hint: &'static str,
    url: &'static str,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Stateful<Div> {
    let palette = style::palette(cx);
    action_surface(id, Transition::new(200, Easing::EaseOut), true, window, cx)
        .flex()
        .flex_col()
        .items_start()
        .gap(px(6.))
        .p(px(16.))
        .child(
            svg()
                .path(icon(name))
                .size(px(20.))
                .mb(px(4.))
                .text_color(palette.accent_text.hsla()),
        )
        .child(
            div()
                .text_size(px(14.))
                .line_height(relative(1.25))
                .font_weight(FontWeight(600.))
                .child(title),
        )
        .child(
            div()
                .text_size(px(12.))
                .line_height(relative(1.25))
                .text_color(palette.muted.hsla())
                .child(hint),
        )
        .on_click(move |_, _, _| {
            let _ = crate::open_external(url);
        })
}

fn update_button(
    id: &'static str,
    enabled: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Stateful<Div> {
    action_surface(id, BUTTON, enabled, window, cx)
        .h(px(48.))
        .px(px(12.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .overflow_hidden()
        .border_1()
        .border_color(TRANSPARENT.hsla())
        .text_size(px(14.))
        .font_weight(FontWeight(600.))
        .line_height(relative(1.25))
}

fn layer(
    id: &'static str,
    active: bool,
    offset: f32,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> (Div, f32) {
    let opacity = anim::tween(
        window,
        cx,
        (gpui::ElementId::from(id), "opacity"),
        active as u8 as f32,
        LAYER,
    );
    let y = anim::tween(window, cx, (gpui::ElementId::from(id), "y"), offset, LAYER);
    (
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(y))
            .h(px(48.))
            .flex()
            .gap(px(12.))
            .opacity(opacity),
        opacity,
    )
}

fn update_controls(
    app: &SettingsApp,
    state: &UpdateState,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let palette = app.palette;
    let checking = matches!(state, UpdateState::Checking);
    let available = matches!(state, UpdateState::Available(_));
    let downloading = matches!(state, UpdateState::Downloading(_) | UpdateState::Installing);
    let initial = !available && !downloading;
    let (check_layer, check_opacity) = layer(
        "about-check-layer",
        initial,
        if initial { 0. } else { -12. },
        window,
        cx,
    );
    let (release_layer, release_opacity) = layer(
        "about-release-layer",
        available,
        if available {
            0.
        } else if downloading {
            -12.
        } else {
            12.
        },
        window,
        cx,
    );
    let (progress_layer, progress_opacity) = layer(
        "about-progress-layer",
        downloading,
        if downloading { 0. } else { 12. },
        window,
        cx,
    );
    let mut wrapper = div().relative().w_full().h(px(48.));
    if check_opacity > 0. {
        // CSS grid sizes the switcher to the wider of both labels, including
        // the currently transparent label. Measure with the actual UI font.
        let mut font = gpui::font(crate::settings_ui::theme::FONT_TEXT);
        font.weight = FontWeight(600.);
        let text_width = ["Check Update", "Checking updates..."]
            .into_iter()
            .map(|text| {
                window
                    .text_system()
                    .shape_line(
                        text.into(),
                        px(14.),
                        &[gpui::TextRun {
                            len: text.len(),
                            font: font.clone(),
                            color: palette.text.hsla(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    )
                    .width
            })
            .fold(px(0.), |width, next| width.max(next));
        let checking_t = anim::tween(
            window,
            cx,
            "about-checking-text",
            checking as u8 as f32,
            BUTTON,
        );
        let angle = app
            .sections
            .about
            .checking_started
            .map(|start| start.elapsed().as_secs_f32() % 1. * std::f32::consts::TAU)
            .unwrap_or(0.);
        if checking {
            window.request_animation_frame();
        }
        let button = update_button("about-check", initial, window, cx)
            .when(checking, |this| this.cursor_default())
            .child(
                svg()
                    .path(icon("refresh-linear"))
                    .size(px(20.))
                    .text_color(palette.accent_text.hsla())
                    .with_transformation(Transformation::rotate(radians(angle))),
            )
            .child(
                div()
                    .relative()
                    .w(text_width)
                    .h(px(18.))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .h(px(18.))
                            .top(px(-10. * checking_t))
                            .flex()
                            .items_center()
                            .justify_center()
                            .opacity(1. - checking_t)
                            .child("Check Update"),
                    )
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .h(px(18.))
                            .top(px(10. * (1. - checking_t)))
                            .flex()
                            .items_center()
                            .justify_center()
                            .opacity(checking_t)
                            .child("Checking updates..."),
                    ),
            )
            .when(initial && !checking, |this| {
                this.on_click(cx.listener(|app, _, _, cx| check(app, false, cx)))
            });
        wrapper = wrapper.child(check_layer.child(button));
    }
    if release_opacity > 0. {
        let release_url = match state {
            UpdateState::Available(update) => update.release_url.clone(),
            _ => RELEASES.into(),
        };
        let download_button = update_button("about-download", available, window, cx)
            .child(
                svg()
                    .path(icon("download-minimalistic-bold"))
                    .size(px(20.))
                    .text_color(palette.accent_text.hsla()),
            )
            .child("Update silence!")
            .when(available, |this| {
                this.on_click(cx.listener(|app, _, _, cx| {
                    if let UpdateState::Available(update) = app.sections.about.update.clone() {
                        download(app, update, cx);
                    }
                }))
            });
        let release_button = update_button("about-release", available, window, cx)
            .child(
                svg()
                    .path(icon("bill-list-bold"))
                    .size(px(20.))
                    .text_color(palette.accent_text.hsla()),
            )
            .child("View Release")
            .when(available, |this| {
                this.on_click(move |_, _, _| {
                    let _ = crate::open_external(&release_url);
                })
            });
        wrapper = wrapper.child(release_layer.child(download_button).child(release_button));
    }
    let progress = match state {
        UpdateState::Downloading(progress) => *progress,
        UpdateState::Installing => 100.,
        _ => 0.,
    };
    let fill = anim::tween(
        window,
        cx,
        "about-progress-width",
        progress,
        Transition::new(240, Easing::Ease),
    );
    if progress_opacity > 0. {
        wrapper = wrapper.child(
            progress_layer.child(
                div()
                    .relative()
                    .w_full()
                    .h(px(48.))
                    .rounded(px(12.))
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left_0()
                            .w(relative(fill / 100.))
                            .bg(mix_pct(palette.accent, 18., palette.surface).hsla()),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .px(px(16.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_size(px(14.))
                            .line_height(relative(1.25))
                            .font_weight(FontWeight(600.))
                            .child(if matches!(state, UpdateState::Installing) {
                                "Launching installer..."
                            } else {
                                "Downloading update..."
                            })
                            .child(
                                div()
                                    .font_weight(FontWeight(650.))
                                    .child(format!("{}%", progress.round() as u32)),
                            ),
                    ),
            ),
        );
    }
    wrapper
}
