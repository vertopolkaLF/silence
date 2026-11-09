//! Volume → Sounds. One tick, bound to `config.volume.sound`.
use super::super::sounds::{self, SoundContext};
use super::{column, heading};
use crate::settings_ui::{SettingsApp, controls};
use gpui::{prelude::*, *};

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let sound = app.snapshot.config.volume.sound.clone();
    let enabled = controls::toggle(
        "volume-sounds-enabled",
        sound.enabled,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| config.volume.sound.enabled = checked);
        },
    );
    // The tick is `volume.sound.mute_theme`. Preview plays that theme's mute sample.
    let tick = sounds::sound_picker(
        "volume-sound-tick",
        "Sound",
        true,
        SoundContext::Volume,
        app,
        window,
        cx,
    );
    vec![column()
        .child(heading(
            "Sounds",
            "A short tick after you change the volume.",
            Some(enabled),
            cx,
        ))
        .child(sounds::card(false, cx).child(tick))
        .into_any_element()]
}
