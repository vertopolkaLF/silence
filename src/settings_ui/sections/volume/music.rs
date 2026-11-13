//! Volume → Music: the track in the volume OSD.
use super::super::form;
use super::column;
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
) -> controls::Toggle {
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
    let duration = controls::range_row(
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
    let dim = form::governed_opacity("music-governed", music.enabled, window, cx);
    let master = form::master_switch(
        "music-master",
        "vinyl-bold",
        "Show the track in the OSD",
        music.enabled,
        enabled,
        window,
        cx,
    );
    let governed = div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(px(form::SECTION_GAP))
        .mt(px(form::SECTION_GAP))
        .opacity(dim)
        .child(
            form::block()
                .child(form::section_heading("Show it when", cx))
                .child(
                    form::card(cx)
                        .row(form::setting_row(
                            "Play and pause",
                            Some("A media key that plays or pauses shows the track."),
                            play_pause,
                            cx,
                        ))
                        .row(form::setting_row(
                            "Next and previous",
                            Some("A media key that skips shows the track it switched to."),
                            skip,
                            cx,
                        ))
                        .row(form::setting_row(
                            "Up next",
                            Some("Shows the next song when the playlist moves on by itself."),
                            up_next,
                            cx,
                        ))
                        .row(duration),
                ),
        )
        .child(
            form::block()
                .child(form::section_heading("With the volume", cx))
                .child(form::card(cx).row(form::setting_row(
                    "Show the track on volume change",
                    Some("Cover, title and artist above the volume bar while something plays."),
                    on_volume,
                    cx,
                ))),
        );
    vec![column()
        .child(form::page_title("Music", None))
        .child(master)
        .child(governed)
        .into_any_element()]
}
