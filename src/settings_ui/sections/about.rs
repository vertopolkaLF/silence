//! About (`src/gui/sections/about.rs`, `assets/styles/about.css`).
use super::form;
use crate::settings_ui::{
    SettingsApp,
    anim::{self, Easing, Transition},
    assets::icon,
    color::{TRANSPARENT, lerp, mix_pct},
    controls::{self, ButtonKind},
    live, style,
};
use gpui::{
    AnyElement, BoxShadow, Context, Div, FontWeight, InteractiveElement, IntoElement,
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
const AUTHOR: &str = "https://github.com/vertopolkaLF";
const DONATE: &str = "http://vertopolkalf.ru/donate";
const LAYER: Transition = Transition::new(400, Easing::Bezier(0.4, 0., 0.2, 1.));

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
                Err(error) => set_update(app, UpdateState::Failed(failure_text(&error))),
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
            Err(error) => UpdateState::Failed(failure_text(&error)),
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
    let hero = div()
        .flex()
        .flex_col()
        .items_center()
        .w_full()
        .pt(px(8.))
        .child(
            div()
                .size(px(LOGO))
                .flex_shrink_0()
                .rounded(px(22.))
                .shadow(vec![shadow(
                    mix_pct(palette.shadow_medium, 60., TRANSPARENT).hsla(),
                    10.,
                    28.,
                )])
                .child(
                    img(crate::settings_ui::assets::logo(LOGO, window))
                        .size(px(LOGO))
                        .rounded(px(22.)),
                ),
        )
        .child(style::h1("silence!").mt(px(18.)).text_size(px(28.)))
        .child(
            div()
                .mt(px(8.))
                .max_w(px(400.))
                .text_center()
                .text_size(px(14.))
                .line_height(relative(1.5))
                .text_color(palette.muted.hsla())
                .child("Free, open-source mic mute and volume control for Windows, with global hotkeys, an on-screen overlay and sounds."),
        )
        .child(
            div()
                .mt(px(14.))
                .h(px(26.))
                .px(px(12.))
                .flex()
                .items_center()
                .rounded_full()
                .bg(palette.accent_soft.hsla())
                .text_color(palette.accent_text.hsla())
                .text_size(px(12.))
                .font_weight(FontWeight(600.))
                .child(format!("Version {}", env!("CARGO_PKG_VERSION"))),
        );

    let state = app.sections.about.update.clone();
    let notify = !app.snapshot.config.advanced.disable_auto_updates;
    let notify_toggle =
        controls::toggle("about-notify", notify, window, cx, |app, checked, _, cx| {
            app.update_config(cx, |config| config.advanced.disable_auto_updates = !checked)
        });
    let updates = form::block()
        .child(form::section_heading("Updates", cx))
        .child(
            form::card(cx)
                .row(status_row(app, &state, window, cx))
                .row(form::setting_row(
                    "Notify when a new version is found",
                    Some("Update checks still run when this is off"),
                    notify_toggle,
                    cx,
                )),
        );

    let grid_row =
        |a: Stateful<Div>, b: Stateful<Div>| div().flex().gap(px(form::TILE_GAP)).child(a).child(b);
    let github = resource(
        "about-github",
        "github",
        "GitHub",
        "Source code",
        GITHUB,
        window,
        cx,
    );
    let donate = resource(
        "about-donate",
        "hand-heart-bold",
        "Donate",
        "Support development",
        DONATE,
        window,
        cx,
    );
    let issues = resource(
        "about-issues",
        "bug-bold",
        "Report an issue",
        "Found a bug?",
        ISSUES,
        window,
        cx,
    );
    let author = resource(
        "about-author",
        "user-bold",
        "Author",
        "@vertopolkaLF",
        AUTHOR,
        window,
        cx,
    );
    let resources = form::block()
        .child(form::section_heading("Resources", cx))
        .child(grid_row(github, donate))
        .child(grid_row(issues, author));

    vec![
        div()
            .id("about-overview")
            .w_full()
            .max_w(px(720.))
            .flex()
            .flex_col()
            .gap(px(form::SECTION_GAP + 4.))
            .child(hero)
            .child(updates)
            .child(resources)
            .into_any_element(),
    ]
}

const LOGO: f32 = 88.;

fn shadow(color: gpui::Hsla, y: f32, blur: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.),
        inset: false,
    }
}

/// Link tile: icon chip, title and hint, an external-link mark on the right.
/// Hover lifts the tile and brightens it; the border never changes.
fn resource(
    id: &'static str,
    name: &'static str,
    title: &'static str,
    hint: &'static str,
    url: &'static str,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Stateful<Div> {
    let p = style::palette(cx);
    let (pointer, hover, _) = controls::hover_tween(id, window, cx);
    controls::track_pointer(
        div()
            .id(id)
            .relative()
            .top(px(-hover))
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(14.))
            .px(px(16.))
            .py(px(14.))
            .rounded(px(14.))
            .bg(lerp(p.surface_2, p.surface_hover, hover * 0.6).hsla())
            .shadow(vec![shadow(p.shadow_faint.opacity(hover).hsla(), 8., 20.)])
            .cursor_pointer()
            .child(
                div()
                    .size(px(40.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.))
                    .bg(p.accent_soft.hsla())
                    .child(
                        svg()
                            .path(icon(name))
                            .size(px(20.))
                            .text_color(p.accent_text.hsla()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .flex_1()
                    .min_w_0()
                    .child(style::label(title))
                    .child(style::desc(hint, cx)),
            )
            .child(
                svg()
                    .path(icon("square-arrow-right-up-linear"))
                    .size(px(18.))
                    .flex_shrink_0()
                    .text_color(lerp(p.muted, p.text, hover).hsla()),
            ),
        pointer,
    )
    .on_click(move |_, _, _| {
        let _ = crate::open_external(url);
    })
}

/// Widths of the control sets that share the status row's right edge.
const CHECK_W: f32 = 168.;
const RELEASE_W: f32 = 124.;
const INSTALL_W: f32 = 104.;
const PROGRESS_W: f32 = 180.;

/// Status icon and text on the left, the action for the current state on the
/// right. Actions swap by sliding and fading; the slot's width follows them.
fn status_row(
    app: &SettingsApp,
    state: &UpdateState,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let p = app.palette;
    let checking = matches!(state, UpdateState::Checking);
    let available = matches!(state, UpdateState::Available(_));
    let downloading = matches!(state, UpdateState::Downloading(_) | UpdateState::Installing);
    let initial = !available && !downloading;

    let (title, detail): (String, Option<String>) = match state {
        UpdateState::Idle => ("Ready to check for updates".into(), None),
        UpdateState::Checking => ("Checking for updates…".into(), None),
        UpdateState::UpToDate(time) => (
            "You're up to date".into(),
            Some(format!("Checked at {time}")),
        ),
        UpdateState::Available(update) => (
            format!("Version {} is available", update.version),
            Some("Install it now or read what changed".into()),
        ),
        UpdateState::Downloading(progress) => (
            "Downloading update…".into(),
            Some(format!("{}%", progress.round() as u32)),
        ),
        UpdateState::Installing => ("Launching installer…".into(), None),
        UpdateState::Failed(error) => ("Update failed".into(), Some(error.clone())),
    };
    let (icon_name, ok, highlight, error) = match state {
        UpdateState::Idle => ("info-circle-linear", 0., 0., 0.),
        UpdateState::Checking => ("refresh-linear", 0., 0., 0.),
        UpdateState::UpToDate(_) => ("check-circle-bold", 1., 0., 0.),
        UpdateState::Available(_) | UpdateState::Downloading(_) | UpdateState::Installing => {
            ("download-minimalistic-bold", 0., 1., 0.)
        }
        UpdateState::Failed(_) => ("solar-danger-triangle-linear", 0., 0., 1.),
    };
    let ok = anim::tween(window, cx, "about-status-ok", ok, anim::UI);
    let highlight = anim::tween(window, cx, "about-status-highlight", highlight, anim::UI);
    let error = anim::tween(window, cx, "about-status-error", error, anim::UI);
    let tone = lerp(
        lerp(lerp(p.muted, p.success, ok), p.accent_text, highlight),
        p.danger,
        error,
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

    let (check_layer, check_opacity) = layer(
        "about-check-layer",
        initial,
        if initial { 0. } else { -10. },
        window,
        cx,
    );
    let (release_layer, release_opacity) = layer(
        "about-release-layer",
        available,
        if available {
            0.
        } else if downloading {
            -10.
        } else {
            10.
        },
        window,
        cx,
    );
    let (progress_layer, progress_opacity) = layer(
        "about-progress-layer",
        downloading,
        if downloading { 0. } else { 10. },
        window,
        cx,
    );
    let slot_w = anim::tween(
        window,
        cx,
        "about-actions-width",
        if available {
            RELEASE_W + 8. + INSTALL_W
        } else if downloading {
            PROGRESS_W
        } else {
            CHECK_W
        },
        LAYER,
    );

    let mut slot = div().relative().flex_shrink_0().h(px(38.)).w(px(slot_w));
    if check_opacity > 0. {
        let button = controls::button(
            "about-check",
            ButtonKind::Primary,
            None,
            Some("Check for updates".into()),
            checking,
            window,
            cx,
            |app, _, _, cx| {
                // Exiting layers stay visible while they fade; only the
                // states this button belongs to may start a check.
                if !app.sections.about.busy()
                    && !matches!(app.sections.about.update, UpdateState::Available(_))
                {
                    check(app, false, cx);
                }
            },
        )
        .w(px(CHECK_W));
        slot = slot.child(check_layer.child(button));
    }
    if release_opacity > 0. {
        let release_url = match state {
            UpdateState::Available(update) => update.release_url.clone(),
            _ => RELEASES.into(),
        };
        let release = controls::button(
            "about-release",
            ButtonKind::Tonal,
            None,
            Some("View release".into()),
            false,
            window,
            cx,
            move |_, _, _, _| {
                let _ = crate::open_external(&release_url);
            },
        )
        .w(px(RELEASE_W));
        let install = controls::button(
            "about-install",
            ButtonKind::Primary,
            None,
            Some("Update".into()),
            false,
            window,
            cx,
            |app, _, _, cx| {
                if let UpdateState::Available(update) = app.sections.about.update.clone() {
                    download(app, update, cx);
                }
            },
        )
        .w(px(INSTALL_W));
        slot = slot.child(release_layer.child(release).child(install));
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
        slot = slot.child(
            progress_layer.child(
                div()
                    .relative()
                    .w(px(PROGRESS_W))
                    .h(px(38.))
                    .rounded(px(8.))
                    .overflow_hidden()
                    .bg(p.surface_3.hsla())
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left_0()
                            .w(relative(fill / 100.))
                            .bg(mix_pct(p.accent, 35., p.surface_3).hsla()),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(13.))
                            .font_weight(FontWeight(600.))
                            .child(format!("{}%", progress.round() as u32)),
                    ),
            ),
        );
    }

    div()
        .flex()
        .items_center()
        .gap(px(14.))
        .min_h(px(form::ROW_HEIGHT + 8.))
        .px(px(form::ROW_PAD))
        .py(px(12.))
        .flex_shrink_0()
        .child(
            svg()
                .path(icon(icon_name))
                .size(px(22.))
                .flex_shrink_0()
                .text_color(tone.hsla())
                .with_transformation(Transformation::rotate(radians(angle))),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .flex_1()
                .min_w_0()
                .child(style::label(title))
                .children(detail.map(|text| style::desc(text, cx))),
        )
        .child(slot)
}

/// One set of actions in the status row's slot, anchored to its right edge.
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
            .right_0()
            .top(px(y))
            .h(px(38.))
            .flex()
            .gap(px(8.))
            .opacity(opacity),
        opacity,
    )
}

/// A sentence for the status row. The anyhow chain is for the log: its top
/// context ("latest GitHub release response") means nothing to a person.
fn failure_text(error: &anyhow::Error) -> String {
    eprintln!("update failed: {error:?}");
    let http = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<reqwest::Error>());
    match http {
        Some(http) if matches!(http.status().map(|s| s.as_u16()), Some(403 | 429)) => {
            "GitHub is limiting update checks right now. Try again in an hour.".into()
        }
        Some(http) if http.status().is_some_and(|s| s.is_server_error()) => {
            "GitHub is having trouble. Try again later.".into()
        }
        Some(http) if http.is_connect() || http.is_timeout() || http.is_request() => {
            "Couldn't reach GitHub. Check your internet connection.".into()
        }
        _ => "Something went wrong. Try again later.".into(),
    }
}
