//! Volume → Media keys.
use super::super::form;
use super::column;
use crate::settings_ui::{SettingsApp, controls};
use gpui::{prelude::*, *};

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let intercept = app.snapshot.config.volume.media_keys.intercept;
    let toggle = controls::toggle(
        "volume-media-keys",
        intercept,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| config.volume.media_keys.intercept = checked);
        },
    );
    vec![column()
        .child(form::page_title("Media keys", None))
        .child(super::super::general::card_row(
            "Use silence! for the volume keys",
            Some(
                "Volume up, down and mute show silence!'s OSD instead of the Windows flyout and step like the wheel. Apps running as administrator keep the Windows flyout unless silence! runs as administrator too.",
            ),
            toggle,
            true,
            cx,
        ))
        .into_any_element()]
}
