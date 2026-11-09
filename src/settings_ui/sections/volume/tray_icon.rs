//! Volume → Tray icon.
use super::super::form::{self, field};
use super::{column, heading};
use crate::settings_ui::{
    SettingsApp,
    anim::UI,
    assets::icon,
    color::{TRANSPARENT, lerp},
    controls::{self, SegmentedToggleOption, TextInputEvent, TextInputOptions},
    style,
    theme::FONT_DISPLAY,
};
use gpui::{prelude::*, *};

const SKINS: &[(crate::VolumeTraySkin, &str)] = &[
    (crate::VolumeTraySkin::Rings, "Rings"),
    (crate::VolumeTraySkin::Digits, "Digits"),
    (crate::VolumeTraySkin::Bar, "Bar"),
    (crate::VolumeTraySkin::Speaker, "Speaker"),
];

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let tray = app.snapshot.config.volume.tray_icon.clone();
    let show = controls::toggle(
        "volume-tray-enabled",
        tray.enabled,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| config.volume.tray_icon.enabled = checked);
        },
    );
    let mut skins = div().w_full().flex().gap(px(10.));
    for (skin, label) in SKINS {
        skins = skins.child(skin_card(*skin, label, tray.skin == *skin, window, cx));
    }
    let mode = match tray.color {
        crate::TrayColorMode::Custom => "Custom",
        crate::TrayColorMode::Accent => "Accent",
    };
    let color_mode = controls::segmented_toggle(
        "volume-tray-color",
        mode,
        vec![
            SegmentedToggleOption::new("Accent", "Windows accent").icon("widget-linear"),
            SegmentedToggleOption::new("Custom", "Custom").icon("pallete-2-linear"),
        ],
        window,
        cx,
        |app, value, window, cx| {
            let custom = value.as_ref() == "Custom";
            if !custom && app.sections.volume.color_focused {
                app.sections.volume.color_focused = false;
                app.focus_handle.focus(window, cx);
            }
            app.update_config(cx, |config| {
                config.volume.tray_icon.color = if custom {
                    crate::TrayColorMode::Custom
                } else {
                    crate::TrayColorMode::Accent
                };
            });
        },
    );
    let custom = tray.color == crate::TrayColorMode::Custom;
    let color = custom_color(tray.custom_color, app, window, cx);
    vec![column()
        .child(heading(
            "Tray icon",
            "How volume shows up in the tray.",
            Some(show),
            cx,
        ))
        .child(field("Skin", skins))
        .child(
            div()
                .flex()
                .flex_col()
                .child(style::label("Color").mb(px(10.)))
                .child(color_mode)
                .child(form::collapse(
                    "volume-tray-custom-color",
                    custom,
                    div().pt(px(10.)).child(color),
                    window,
                    cx,
                )),
        )
        .into_any_element()]
}

fn skin_card(
    skin: crate::VolumeTraySkin,
    label: &'static str,
    selected: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Stateful<Div> {
    let p = style::palette(cx);
    let id = SharedString::from(format!("volume-skin-{label}"));
    let (pointer, hover, _) = controls::hover_tween(id.clone(), window, cx);
    let active = crate::settings_ui::anim::tween(
        window,
        cx,
        (ElementId::from(id.clone()), "selected"),
        selected as u8 as f32,
        UI,
    );
    let focus = window
        .use_keyed_state((ElementId::from(id.clone()), "focus"), cx, |_, cx| {
            cx.focus_handle()
        })
        .read(cx)
        .clone();
    let ink = lerp(p.muted, p.accent, active.max(hover * 0.35)).hsla();
    let preview = skin_preview(skin, 0.66, false, ink, p.danger.hsla());
    controls::track_pointer(
        div()
            .id(id)
            .track_focus(&focus)
            .tab_index(0)
            .focus_visible(move |style| controls::focus_outline(style, p.focus.hsla()))
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .px(px(8.))
            .py(px(12.))
            .rounded(px(12.))
            .border_1()
            .border_color(lerp(p.border, p.accent, active).hsla())
            .bg(lerp(p.surface_2, p.surface_hover, hover).hsla())
            .cursor_pointer()
            .child(
                div()
                    .h(px(56.))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .child(preview),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(relative(1.2))
                    .font_weight(FontWeight(600.))
                    .text_color(lerp(p.muted, p.text, active.max(hover)).hsla())
                    .child(label),
            ),
        pointer,
    )
    .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _, cx| {
        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
            app.update_config(cx, |config| config.volume.tray_icon.skin = skin);
            cx.stop_propagation();
        }
    }))
    .on_click(cx.listener(move |app, _, _, cx| {
        app.update_config(cx, |config| config.volume.tray_icon.skin = skin);
    }))
}

/// Sample glyph for a skin card.
///
/// TODO(lead): render preview with render_volume_icon_rgba
fn skin_preview(
    skin: crate::VolumeTraySkin,
    level: f32,
    muted: bool,
    ink: Hsla,
    danger: Hsla,
) -> AnyElement {
    // `render_volume_icon_rgba(style, level, muted, size) -> Vec<u8>`
    // in `src/main_parts/volume_tray_icons.rs` is not merged yet.
    let level = if muted { 0. } else { level.clamp(0., 1.) };
    let ink = if muted { danger } else { ink };
    match skin {
        crate::VolumeTraySkin::Rings => rings(ink, level),
        crate::VolumeTraySkin::Digits => digits(ink, level),
        crate::VolumeTraySkin::Bar => bars(ink, level),
        crate::VolumeTraySkin::Speaker => speaker(ink, muted),
    }
}

fn rings(ink: Hsla, level: f32) -> AnyElement {
    let inner = 8. + 12. * level;
    div()
        .size(px(34.))
        .rounded_full()
        .border_1()
        .border_color(ink)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .size(px(inner))
                .rounded_full()
                .border_1()
                .border_color(ink),
        )
        .into_any_element()
}

fn digits(ink: Hsla, level: f32) -> AnyElement {
    let percent = (level * 100.).round() as u8;
    div()
        .font_family(FONT_DISPLAY)
        .text_size(px(22.))
        .line_height(relative(1.))
        .font_weight(FontWeight(650.))
        .text_color(ink)
        .child(percent.to_string())
        .into_any_element()
}

fn bars(ink: Hsla, level: f32) -> AnyElement {
    let heights = [0.35, 0.55, 0.85, 0.62, 0.4];
    div()
        .h(px(28.))
        .flex()
        .items_end()
        .gap(px(3.))
        .children(heights.into_iter().map(|height| {
            let height = 6. + 22. * height * (0.35 + 0.65 * level);
            div()
                .w(px(4.))
                .h(px(height))
                .rounded(px(2.))
                .bg(ink)
        }))
        .into_any_element()
}

fn speaker(ink: Hsla, muted: bool) -> AnyElement {
    svg()
        .path(icon(if muted {
            "volume-cross-linear"
        } else {
            "volume-loud-linear"
        }))
        .size(px(30.))
        .text_color(ink)
        .into_any_element()
}

fn custom_color(
    saved: String,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    {
        let state = &mut app.sections.volume;
        if !state.color_focused && state.color_backing != saved {
            state.color_backing = saved.clone();
            state.color_text = saved.clone();
        }
    }
    let text = if app.sections.volume.color_focused {
        app.sections.volume.color_text.clone()
    } else {
        saved.clone()
    };
    let input = controls::text_input(
        "volume-tray-custom-hex",
        text.clone(),
        TextInputOptions {
            placeholder: "#5CA8FF".into(),
            max_length: Some(7),
            ..Default::default()
        },
        window,
        cx,
        move |app, event, window, cx| match event {
            TextInputEvent::Focus => app.sections.volume.color_focused = true,
            TextInputEvent::Change(raw) => {
                app.sections.volume.color_text = raw.to_string();
                if let Some(color) = normalize_hex(raw.as_ref()) {
                    app.sections.volume.color_backing = color.clone();
                    app.update_config(cx, |config| config.volume.tray_icon.custom_color = color);
                }
                cx.notify();
            }
            TextInputEvent::Blur(raw) => {
                commit_color(app, raw.as_ref(), false, window, cx);
            }
            TextInputEvent::Enter(raw) => {
                commit_color(app, raw.as_ref(), true, window, cx);
            }
            TextInputEvent::Escape => {
                let color = app.sections.volume.color_backing.clone();
                app.sections.volume.color_focused = false;
                app.sections.volume.color_text = color.clone();
                cx.defer_in(window, move |app, window, cx| {
                    if let Some(input) = &app.sections.volume.color_input {
                        input.update(cx, |input, cx| input.set_value(color, cx));
                    }
                    app.focus_handle.focus(window, cx);
                });
                cx.notify();
            }
            _ => {}
        },
    );
    app.sections.volume.color_input = Some(input.clone());
    let p = app.palette;
    let shown = if app.sections.volume.color_focused {
        app.sections.volume.color_text.clone()
    } else {
        saved
    };
    let swatch = hex_color(&shown).unwrap_or_else(|| p.windows_accent.hsla());
    let focused = input.read(cx).focus_handle(cx).is_focused(window);
    let focus = crate::settings_ui::anim::tween(
        window,
        cx,
        "volume-tray-hex-focus",
        focused as u8 as f32,
        UI,
    );
    let (pointer, hover, _) = controls::hover_tween("volume-tray-hex-shell", window, cx);
    let focus_handle = input.read(cx).focus_handle(cx);
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(
            div()
                .size(px(28.))
                .flex_shrink_0()
                .rounded(px(8.))
                .bg(swatch)
                .border_1()
                .border_color(p.border.hsla()),
        )
        .child(
            controls::track_pointer(
                controls::text_input_container("volume-tray-hex-shell", &input)
                    .w(px(148.))
                    .h(px(42.))
                    .px(px(12.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(lerp(TRANSPARENT, p.focus, focus).hsla())
                    .bg(lerp(p.surface_2, p.surface_3, hover).hsla())
                    .text_size(px(14.))
                    .font_weight(FontWeight(600.))
                    .text_color(p.text.hsla())
                    .cursor(CursorStyle::IBeam)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        focus_handle.focus(window, cx);
                    }),
                pointer,
            ),
        )
}

fn commit_color(
    app: &mut SettingsApp,
    raw: &str,
    keep_focus: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) {
    let color = normalize_hex(raw)
        .or_else(|| normalize_hex(&app.sections.volume.color_backing))
        .unwrap_or_else(|| "#5CA8FF".to_string());
    app.sections.volume.color_focused = keep_focus;
    app.sections.volume.color_text = color.clone();
    app.sections.volume.color_backing = color.clone();
    app.update_config(cx, |config| config.volume.tray_icon.custom_color = color.clone());
    if raw != color {
        cx.defer_in(window, move |app, _, cx| {
            if let Some(input) = &app.sections.volume.color_input {
                input.update(cx, |input, cx| input.set_value(color, cx));
            }
        });
    }
    cx.notify();
}

fn normalize_hex(value: &str) -> Option<String> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() == 6 && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(format!("#{hex}").to_ascii_uppercase())
    } else {
        None
    }
}

fn hex_color(value: &str) -> Option<Hsla> {
    let hex = normalize_hex(value)?;
    let packed = u32::from_str_radix(hex.trim_start_matches('#'), 16).ok()?;
    Some(gpui::rgb(packed).into())
}
