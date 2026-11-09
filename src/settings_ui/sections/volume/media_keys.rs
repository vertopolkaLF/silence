//! Volume → Media keys.
use super::{column, heading};
use crate::settings_ui::{SettingsApp, controls, style};
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
        .child(heading(
            "Media keys",
            "What the keyboard volume keys do.",
            None::<Div>,
            cx,
        ))
        .child(super::super::general::card_row(
            "Use silence! for the keyboard volume keys",
            Some("Replaces the Windows volume flyout with silence!'s OSD."),
            toggle,
            true,
            cx,
        ))
        .child(
            style::p(
                "While an app running as administrator is focused, Windows handles the keys, unless silence! runs as administrator.",
                cx,
            )
            .text_size(px(13.)),
        )
        .into_any_element()]
}