//! Sounds tab (`src/gui/sections/sounds.rs`, `assets/styles/sounds.css`).
use crate::settings_ui::{
    SettingsApp,
    anim::{self, Easing, Transition},
    assets::icon,
    color::lerp,
    controls::{self, ButtonKind, RangeOptions, SelectOption, SelectOptions},
    style,
};
use gpui::{prelude::*, *};
use std::time::{Duration, Instant};

const DEFAULT_SOUND_OPTION: &str = "__default__";

#[derive(Default)]
pub(crate) struct PreviewState {
    playing: Option<String>,
    // A previous completion must not clear a newer play of the same sound.
    generation: u64,
}

#[derive(Clone)]
struct LibraryRow {
    sound: crate::CustomSound,
    entering: bool,
    removing: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct State {
    pub preview: PreviewState,
    choosing_files: bool,
    library_initialized: bool,
    library: Vec<LibraryRow>,
}

/// The preview sink is shared by both tabs; their UI state lives in Sections.
#[derive(Clone, Copy)]
pub(crate) enum SoundContext {
    Sounds,
    HoldToMute,
}

impl SoundContext {
    fn preview(self, app: &mut SettingsApp) -> &mut PreviewState {
        match self {
            Self::Sounds => &mut app.sections.sounds.preview,
            Self::HoldToMute => &mut app.sections.hold_to_mute.preview,
        }
    }
}

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let sounds = app.snapshot.config.sound_settings.clone();
    let enabled = controls::toggle(
        "sounds-enabled",
        sounds.enabled,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| config.sound_settings.enabled = checked);
        },
    );
    let volume = controls::range(
        "sounds-volume",
        f64::from(sounds.volume.min(100)),
        volume_options(format!("{}%", sounds.volume.min(100))),
        window,
        cx,
        |app, value, _, cx| {
            app.update_config(cx, |config| config.sound_settings.volume = value as u8);
        },
    );
    let mute = sound_picker(
        "sounds-mute",
        "Mute Sound",
        true,
        SoundContext::Sounds,
        app,
        window,
        cx,
    );
    let unmute = sound_picker(
        "sounds-unmute",
        "Unmute Sound",
        false,
        SoundContext::Sounds,
        app,
        window,
        cx,
    );
    let add = controls::button(
        "sounds-add",
        ButtonKind::Secondary,
        Some("plus-linear"),
        Some("Add sounds".into()),
        app.sections.sounds.choosing_files,
        window,
        cx,
        |app, _, _, cx| choose_files(app, cx),
    );
    let library = library(app, window, cx);
    let files = div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(6.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(style::h2("Files"))
                        .child(
                            style::p("Add custom sounds", cx)
                                .text_size(px(13.))
                                .line_height(relative(1.4)),
                        ),
                )
                .child(add),
        )
        .child(library);
    vec![
        panel()
            .child(header().child(style::h1("Sounds")).child(enabled))
            .child(card(true, cx).child(volume))
            .child(card(true, cx).gap(px(22.)).child(mute).child(unmute))
            .child(files)
            .into_any_element(),
    ]
}

pub(crate) fn panel() -> Div {
    div()
        .w_full()
        .max_w(px(720.))
        .flex()
        .flex_col()
        .gap(px(18.))
}

pub(crate) fn header() -> Div {
    div()
        .h(px(40.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(14.))
}

/// `.sound-card`; the last card has no bottom padding or rule.
pub(crate) fn card(divider: bool, cx: &App) -> Div {
    div()
        .w_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(px(16.))
        .when(divider, |this| {
            this.pb(px(18.))
                .border_b_1()
                .border_color(style::palette(cx).border.hsla())
        })
}

pub(crate) fn volume_options(label: String) -> RangeOptions {
    RangeOptions {
        min: 0.,
        max: 100.,
        step: 1.,
        label: "Volume".into(),
        value_label: label.into(),
        icon: Some("volume-loud-linear".into()),
    }
}

pub(crate) fn sound_options(custom_sounds: &[crate::CustomSound]) -> Vec<SelectOption> {
    custom_sounds
        .iter()
        .map(|sound| {
            SelectOption::new(
                format!("custom:{}", sound.id),
                sound.original_file_name.clone(),
            )
            .group("Custom Sounds")
            .icon("volume-loud-linear")
        })
        .chain(crate::sound_themes().iter().map(|theme| {
            SelectOption::new(theme.id, theme.label)
                .group("Built-in Sounds")
                .icon("volume-loud-linear")
        }))
        .collect()
}

/// Shared mute/unmute selector, with optional inherited Hold to Mute value,
/// preview and Reset. Only Sounds has independent actions in its option menu,
/// matching the original two tabs.
pub(crate) fn sound_picker(
    id: &'static str,
    title: &'static str,
    muted: bool,
    context: SoundContext,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let sounds = &app.snapshot.config.sound_settings;
    let hold = &app.snapshot.config.hold_to_mute;
    let base = if muted {
        &sounds.mute_theme
    } else {
        &sounds.unmute_theme
    };
    let override_theme = if muted {
        &hold.mute_theme_override
    } else {
        &hold.unmute_theme_override
    };
    let inherited = matches!(context, SoundContext::HoldToMute);
    let value = if inherited {
        override_theme.as_deref().unwrap_or(DEFAULT_SOUND_OPTION)
    } else {
        base
    }
    .to_string();
    let selection = if inherited {
        override_theme.as_ref().unwrap_or(base)
    } else {
        base
    }
    .clone();
    let volume = if inherited {
        hold.volume_override.unwrap_or(sounds.volume)
    } else {
        sounds.volume
    }
    .min(100);
    let has_override = override_theme.is_some();
    let preview_title = if inherited {
        format!("Preview {title}")
    } else {
        format!("Preview {}", crate::sound_selection_label(base, sounds))
    };
    let mut options = sound_options(&sounds.custom_sounds);
    if inherited {
        options.insert(
            0,
            SelectOption::new(
                DEFAULT_SOUND_OPTION,
                format!("Default ({})", crate::sound_selection_label(base, sounds)),
            ),
        );
    } else {
        options = options
            .into_iter()
            .map(|option| option.end_icon("play-bold"))
            .collect();
    }
    let mut options = SelectOptions::new(options);
    options.disabled = inherited && !hold.play_sounds;
    if !inherited {
        options = options.on_option_action(move |app, value, _, cx| {
            toggle_preview(app, context, value.to_string(), muted, volume, cx);
        });
    }
    let select = controls::select(
        id,
        value,
        options,
        app,
        window,
        cx,
        move |app, value, _, cx| {
            app.update_config(cx, |config| {
                if inherited {
                    let value = (value.as_ref() != DEFAULT_SOUND_OPTION).then(|| value.to_string());
                    if muted {
                        config.hold_to_mute.mute_theme_override = value;
                    } else {
                        config.hold_to_mute.unmute_theme_override = value;
                    }
                } else if muted {
                    config.sound_settings.mute_theme = value.to_string();
                } else {
                    config.sound_settings.unmute_theme = value.to_string();
                }
            });
        },
    );
    let preview = preview_button(
        format!("{id}-preview"),
        preview_title,
        selection,
        muted,
        volume,
        context,
        44.,
        app,
        window,
        cx,
    );
    let mut row = div()
        .flex()
        .items_stretch()
        .gap(px(6.))
        .child(div().flex_1().min_w_0().child(select))
        .child(preview);
    if inherited {
        row = row.child(
            controls::button(
                format!("{id}-reset"),
                ButtonKind::Secondary,
                None,
                Some("Reset".into()),
                !has_override,
                window,
                cx,
                move |app, _, _, cx| {
                    app.update_config(cx, |config| {
                        if muted {
                            config.hold_to_mute.mute_theme_override = None;
                        } else {
                            config.hold_to_mute.unmute_theme_override = None;
                        }
                    });
                },
            )
            .h(px(44.))
            .px(px(14.)),
        );
    }
    div()
        .w_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(style::label(title))
        .child(row)
}

fn preview_key(selection: &str, muted: bool) -> String {
    format!("{}:{selection}", if muted { "mute" } else { "unmute" })
}

fn preview_button(
    id: String,
    title: String,
    selection: String,
    muted: bool,
    volume: u8,
    context: SoundContext,
    size: f32,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Stateful<Div> {
    let playing =
        context.preview(app).playing.as_deref() == Some(preview_key(&selection, muted).as_str());
    let p = app.palette;
    controls::button(
        id,
        ButtonKind::Icon,
        None,
        None,
        false,
        window,
        cx,
        move |app, _, _, cx| {
            toggle_preview(app, context, selection.clone(), muted, volume, cx);
        },
    )
    .w(px(size))
    .h(px(size))
    .tooltip(move |_, cx| controls::tooltip(title.clone(), cx))
    .child(
        svg()
            .path(icon(if playing { "pause-bold" } else { "play-bold" }))
            .size(px(if size == 44. { 19. } else { 15. }))
            .text_color(p.muted.hsla()),
    )
}

fn clear_preview(state: &mut PreviewState) {
    state.playing = None;
    state.generation = state.generation.wrapping_add(1);
}

fn toggle_preview(
    app: &mut SettingsApp,
    context: SoundContext,
    selection: String,
    muted: bool,
    volume: u8,
    cx: &mut Context<SettingsApp>,
) {
    let key = preview_key(&selection, muted);
    let stop = context.preview(app).playing.as_ref() == Some(&key);
    if stop {
        crate::stop_preview_sound();
        clear_preview(&mut app.sections.sounds.preview);
        clear_preview(&mut app.sections.hold_to_mute.preview);
        cx.notify();
        return;
    }
    match crate::preview_sound(&selection, muted, volume) {
        Ok(duration_ms) => {
            clear_preview(&mut app.sections.sounds.preview);
            clear_preview(&mut app.sections.hold_to_mute.preview);
            let state = context.preview(app);
            state.playing = Some(key);
            let generation = state.generation;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(duration_ms))
                    .await;
                let _ = this.update(cx, |app, cx| {
                    let state = context.preview(app);
                    if state.generation == generation {
                        state.playing = None;
                        cx.notify();
                    }
                });
            })
            .detach();
            cx.notify();
        }
        Err(error) => eprintln!("failed to preview sound: {error:?}"),
    }
}

fn choose_files(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    if app.sections.sounds.choosing_files {
        return;
    }
    app.sections.sounds.choosing_files = true;
    // rfd's synchronous Windows dialog and copying files run off the UI thread.
    let task = cx
        .background_executor()
        .spawn(async { crate::choose_custom_sounds() });
    cx.spawn(async move |this, cx| {
        let result = task.await;
        let _ = this.update(cx, |app, cx| {
            app.sections.sounds.choosing_files = false;
            match result {
                Ok(sounds) if !sounds.is_empty() => {
                    app.update_config(cx, |config| {
                        config.sound_settings.custom_sounds.extend(sounds)
                    });
                }
                Ok(_) => {}
                Err(error) => eprintln!("failed to add custom sounds: {error:?}"),
            }
            cx.notify();
        });
    })
    .detach();
    cx.notify();
}

fn remove_sound(app: &mut SettingsApp, id: &str, cx: &mut Context<SettingsApp>) {
    app.update_config(cx, |config| {
        config
            .sound_settings
            .custom_sounds
            .retain(|sound| sound.id != id);
        let removed = format!("custom:{id}");
        if config.sound_settings.mute_theme == removed {
            config.sound_settings.mute_theme = "8bit".into();
        }
        if config.sound_settings.unmute_theme == removed {
            config.sound_settings.unmute_theme = "8bit".into();
        }
        if config.hold_to_mute.mute_theme_override.as_deref() == Some(&removed) {
            config.hold_to_mute.mute_theme_override = None;
        }
        if config.hold_to_mute.unmute_theme_override.as_deref() == Some(&removed) {
            config.hold_to_mute.unmute_theme_override = None;
        }
    });
}

fn library(app: &mut SettingsApp, window: &mut Window, cx: &mut Context<SettingsApp>) -> Div {
    let sounds = &app.snapshot.config.sound_settings.custom_sounds;
    let state = &mut app.sections.sounds;
    state.library.retain(|row| {
        !row.removing
            .is_some_and(|started| started.elapsed() >= Duration::from_millis(220))
    });
    for row in &mut state.library {
        if let Some(sound) = sounds.iter().find(|sound| sound.id == row.sound.id) {
            row.sound = sound.clone();
            row.removing = None;
        } else if row.removing.is_none() {
            row.removing = Some(Instant::now());
        }
    }
    for sound in sounds {
        if !state.library.iter().any(|row| row.sound.id == sound.id) {
            state.library.push(LibraryRow {
                sound: sound.clone(),
                entering: state.library_initialized,
                removing: None,
            });
        }
    }
    state.library_initialized = true;
    let rows = state.library.clone();
    let count = rows.len();
    let mut library = div().w_full().flex().flex_col();
    for (index, row) in rows.into_iter().enumerate() {
        let id = format!("sounds-file-{}", row.sound.id);
        let progress = anim::tween_from(
            window,
            cx,
            SharedString::from(format!("{id}-presence")),
            if row.entering { 0. } else { 1. },
            if row.removing.is_some() { 0. } else { 1. },
            Transition::new(220, Easing::Ease),
        );
        let preview = preview_button(
            format!("{id}-preview"),
            format!("Preview {}", row.sound.original_file_name),
            format!("custom:{}", row.sound.id),
            true,
            app.snapshot.config.sound_settings.volume.min(100),
            SoundContext::Sounds,
            34.,
            app,
            window,
            cx,
        );
        let p = app.palette;
        let (_, hover, _) =
            controls::hover_tween(SharedString::from(format!("{id}-remove")), window, cx);
        let fg = lerp(p.muted, p.danger_hover, hover).hsla();
        let remove_title = format!("Remove {}", row.sound.original_file_name);
        let remove = controls::button(
            format!("{id}-remove"),
            ButtonKind::Icon,
            None,
            None,
            row.removing.is_some(),
            window,
            cx,
            move |app, _, _, cx| remove_sound(app, &row.sound.id, cx),
        )
        .w(px(34.))
        .h(px(34.))
        .bg(lerp(p.surface_2, p.surface_danger_hover, hover).hsla())
        .text_color(fg)
        .tooltip(move |_, cx| controls::tooltip(remove_title.clone(), cx))
        .child(
            svg()
                .path(icon("trash-bin-trash-linear"))
                .size(px(18.))
                .text_color(fg),
        );
        library = library.child(
            div()
                .w_full()
                .h(px(72. * progress))
                .flex_shrink_0()
                .overflow_hidden()
                .opacity(progress)
                .child(
                    div()
                        .w_full()
                        .h(px(72.))
                        .py(px(10.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.))
                        .when(index + 1 < count, |this| {
                            this.border_b_1().border_color(p.border.hsla())
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(13.))
                                .font_weight(FontWeight(600.))
                                .line_height(relative(1.25))
                                .line_clamp(2)
                                .child(row.sound.original_file_name),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .min_h(px(42.))
                                .child(preview)
                                .child(remove),
                        ),
                ),
        );
    }
    library
}
