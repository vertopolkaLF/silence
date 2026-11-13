//! Hold to Mute tab (`src/gui/sections/hold_to_mute.rs`, `sounds.css`).
use super::{
    form,
    sounds::{self, PreviewState, SoundContext},
};
use crate::settings_ui::{
    HotkeyModalRequest, SettingsApp,
    anim::{self, Easing, Transition},
    controls::{self, ButtonKind},
    shell::nav::SettingsTab,
};
use gpui::{prelude::*, *};

pub(crate) struct State {
    pub preview: PreviewState,
    /// Measured expanded height; used by the CSS 0fr -> 1fr collapse port.
    sound_settings_height: f32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            preview: PreviewState::default(),
            sound_settings_height: 283.05,
        }
    }
}

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let hold = app.snapshot.config.hold_to_mute.clone();
    let configure = controls::button(
        "hold-configure-hotkeys",
        ButtonKind::Secondary,
        Some("keyboard-linear"),
        Some("Configure hotkeys".into()),
        false,
        window,
        cx,
        |app, _, _, cx| {
            app.pending_hotkey_modal_after_nav = Some(HotkeyModalRequest::Add {
                preset_action: Some(crate::HotkeyAction::HoldToMute),
            });
            app.open_select = None;
            app.nav.select(SettingsTab::Hotkeys);
            cx.notify();
        },
    );
    let play = controls::toggle(
        "hold-play-sounds",
        hold.play_sounds,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| config.hold_to_mute.play_sounds = checked)
        },
    );
    let overlay = controls::toggle(
        "hold-show-overlay",
        hold.show_overlay,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| config.hold_to_mute.show_overlay = checked)
        },
    );
    let options = form::card(cx)
        .row(form::setting_row(
            "Show overlay",
            Some("The overlay appears while the key is held."),
            overlay,
            cx,
        ))
        .row(form::setting_row(
            "Play sounds",
            Some("Mute and unmute sounds for the held key."),
            play,
            cx,
        ));

    let expanded = anim::tween(
        window,
        cx,
        "hold-sound-settings-height",
        hold.play_sounds as u8 as f32,
        Transition::new(220, Easing::Ease),
    );
    let opacity = anim::tween(
        window,
        cx,
        "hold-sound-settings-opacity",
        hold.play_sounds as u8 as f32,
        Transition::new(180, Easing::Ease),
    );
    if !hold.play_sounds
        && app
            .open_select
            .as_deref()
            .is_some_and(|id| matches!(id, "hold-mute" | "hold-unmute"))
    {
        app.open_select = None;
    }
    let mut collapse = div()
        .w_full()
        .flex_shrink_0()
        .overflow_hidden()
        .h(px(
            app.sections.hold_to_mute.sound_settings_height * expanded
        ))
        .opacity(opacity);
    if hold.play_sounds || expanded > 0. {
        let sounds_config = &app.snapshot.config.sound_settings;
        let volume = hold
            .volume_override
            .unwrap_or(sounds_config.volume)
            .min(100);
        let volume_label = if hold.volume_override.is_some() {
            format!("{volume}%")
        } else {
            format!("Default ({}%)", sounds_config.volume.min(100))
        };
        let range = controls::range(
            "hold-volume",
            f64::from(volume),
            sounds::volume_options(volume_label),
            window,
            cx,
            |app, value, _, cx| {
                app.update_config(cx, |config| {
                    config.hold_to_mute.volume_override = Some(value as u8)
                })
            },
        );
        let reset = controls::button(
            "hold-volume-reset",
            ButtonKind::Secondary,
            None,
            Some("Reset".into()),
            hold.volume_override.is_none(),
            window,
            cx,
            |app, _, _, cx| {
                app.update_config(cx, |config| config.hold_to_mute.volume_override = None)
            },
        );
        let mute = sounds::sound_picker(
            "hold-mute",
            "Mute sound",
            true,
            SoundContext::HoldToMute,
            app,
            window,
            cx,
        );
        let unmute = sounds::sound_picker(
            "hold-unmute",
            "Unmute sound",
            false,
            SoundContext::HoldToMute,
            app,
            window,
            cx,
        );
        // The gap above is inside the measured height, so the closed block
        // leaves no hole under the card.
        let content = form::block()
            .pt(px(form::SECTION_GAP))
            .child(form::section_heading("Sounds", cx))
            .child(form::card(cx).row(mute).row(unmute))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().flex_1().min_w_0().child(range))
                    .child(reset),
            );
        let app_entity = cx.weak_entity();
        collapse = collapse.child(
            div()
                .w_full()
                .on_children_prepainted(move |bounds, _, cx| {
                    if let Some(bounds) = bounds.first() {
                        let height = bounds.size.height.as_f32();
                        let _ = app_entity.update(cx, |app, cx| {
                            if (app.sections.hold_to_mute.sound_settings_height - height).abs()
                                > 0.1
                            {
                                app.sections.hold_to_mute.sound_settings_height = height;
                                cx.notify();
                            }
                        });
                    }
                })
                .child(content),
        );
    }
    vec![
        div()
            .w_full()
            .max_w(px(720.))
            .flex()
            .flex_col()
            .child(form::page_title("Hold to mute", Some(configure.into_any_element())))
            .child(options)
            .child(collapse)
            .into_any_element(),
    ]
}
