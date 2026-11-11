//! Volume → Music: the track in the volume OSD.
use super::{column, heading};
use crate::settings_ui::{
    SettingsApp,
    controls::{self, RangeOptions},
};
use gpui::{prelude::*, *};

/// Saves a Music setting and shows the current track so the change is visible.
fn update_and_preview(
    app: &mut SettingsApp,
    cx: &mut Context<SettingsApp>,
    update: impl FnOnce(&mut crate::MusicConfig),
) {
    app.update_config(cx, |config| update(&mut config.volume.music));
    crate::request_music_osd_preview();
}

fn switch(
    id: &'static str,
    value: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    write: impl Fn(&mut crate::MusicConfig, bool) + 'static,
) -> Stateful<Div> {
    controls::toggle(id, value, window, cx, move |app, checked, _, cx| {
        app.update_config(cx, |config| write(&mut config.volume.music, checked));
    })
}

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let music = app.snapshot.config.volume.music.clone();
    let enabled = controls::toggle(
        "music-enabled",
        music.enabled,
        window,
        cx,
        |app, checked, _, cx| {
            update_and_preview(app, cx, |music| music.enabled = checked);
        },
    );
    let play_pause = switch(
        "music-play-pause",
        music.on_play_pause,
        window,
        cx,
        |music, value| music.on_play_pause = value,
    );
    let skip = switch("music-skip", music.on_skip, window, cx, |music, value| {
        music.on_skip = value
    });
    let up_next = switch("music-up-next", music.up_next, window, cx, |music, value| {
        music.up_next = value
    });
    let on_volume = controls::toggle(
        "music-on-volume",
        music.on_volume_change,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| config.volume.music.on_volume_change = checked);
            crate::request_volume_osd_preview();
        },
    );
    let duration_ms = music.duration_ms.clamp(1_000, 10_000);
    let duration = controls::range(
        "music-duration",
        f64::from(duration_ms),
        RangeOptions {
            min: 1_000.,
            max: 10_000.,
            step: 500.,
            label: "Duration".into(),
            value_label: format!("{:.1} s", f64::from(duration_ms) / 1000.).into(),
            icon: None,
        },
        window,
        cx,
        |app, value, _, cx| {
            let ms = (value / 500.).round() as u32 * 500;
            update_and_preview(app, cx, |music| music.duration_ms = ms.clamp(1_000, 10_000));
        },
    );
    let row = super::super::general::card_row;

    vec![column()
        .gap(px(0.))
        .child(heading("Music", "", Some(enabled), cx))
        .child(
            form_panel(false, cx)
                .child(row(
                    "Play and pause",
                    Some("A media key that plays or pauses shows the track."),
                    play_pause,
                    false,
                    cx,
                ))
                .child(row(
                    "Next and previous",
                    Some("A media key that skips shows the track it switched to."),
                    skip,
                    false,
                    cx,
                ))
                .child(row(
                    "Up next",
                    Some("Shows the next song when the playlist moves on by itself."),
                    up_next,
                    true,
                    cx,
                )),
        )
        .child(form_panel(false, cx).mt(px(18.)).child(duration))
        .child(
            form_panel(true, cx).mt(px(18.)).child(row(
                "Show the track on volume change",
                Some("Cover, title and artist above the volume bar while something plays."),
                on_volume,
                true,
                cx,
            )),
        )
        .into_any_element()]
}

fn form_panel(last: bool, cx: &mut Context<SettingsApp>) -> Div {
    super::super::form::panel(18., last, cx)
}
