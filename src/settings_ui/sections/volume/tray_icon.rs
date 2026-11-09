//! Volume → Tray icon.
use super::super::form::{self, field};
use super::{column, heading};
use crate::settings_ui::{
    SettingsApp,
    anim::UI,
    color::{TRANSPARENT, lerp},
    controls::{self, SelectOption, SelectOptions, TextInputEvent, TextInputOptions},
    style,
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
    let style = crate::volume_icon_style(&tray);
    let light = app.palette.bg.l > 0.6;
    let mut skins = div().flex().gap(px(12.));
    for (skin, label) in SKINS {
        let skin = *skin;
        let preview = skin_image(app, skin, style.accent, light, window);
        skins = skins.child(super::super::tray_icon::tile(
            format!("volume-skin-{label}").into(),
            label,
            tray.skin == skin,
            img(preview).size(px(PREVIEW_PX)).into_any_element(),
            window,
            cx,
            move |app, _, cx| {
                app.update_config(cx, |config| config.volume.tray_icon.skin = skin);
            },
        ));
    }
    let mode = match tray.color {
        crate::TrayColorMode::Custom => "Custom",
        crate::TrayColorMode::Accent => "Accent",
    };
    let color_mode = controls::select(
        "volume-tray-color",
        mode,
        SelectOptions::new(vec![
            SelectOption::new("Accent", "Windows accent")
                .detail("Follows your Windows accent color")
                .icon("pallete-2-linear"),
            SelectOption::new("Custom", "Custom color")
                .detail("Pick any color below")
                .icon("pallete-2-linear"),
        ]),
        app,
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
        .gap(px(0.))
        .child(heading("Tray icon", "", Some(show), cx))
        .child(
            form::panel(18., true, cx)
                .child(field("Style", skins))
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
                .child(
                    style::p(
                        "Scroll over the icon to change the volume. Clicks are set on Wheel & clicks.",
                        cx,
                    )
                    .text_size(px(13.)),
                ),
        )
        .into_any_element()]
}

const PREVIEW_PX: f32 = 32.;
const PREVIEW_LEVEL: u8 = 66;

/// The real tray rasterizer at the tile's device-pixel size, cached so
/// re-renders and hover animations don't redraw it.
fn skin_image(
    app: &mut SettingsApp,
    skin: crate::VolumeTraySkin,
    accent: [u8; 3],
    light: bool,
    window: &Window,
) -> std::sync::Arc<Image> {
    let size = (PREVIEW_PX * window.scale_factor()).round().max(16.) as u32;
    let key = (skin as u8, accent, light, size, PREVIEW_LEVEL);
    app.sections
        .volume
        .skin_images
        .entry(key)
        .or_insert_with(|| {
            let style = crate::volume_tray_icons::VolumeIconStyle::new(
                crate::map_volume_tray_skin(skin),
                accent,
                light,
            );
            let rgba = crate::volume_tray_icons::render_volume_icon_rgba(
                &style,
                PREVIEW_LEVEL,
                false,
                size,
            );
            std::sync::Arc::new(Image::from_bytes(ImageFormat::Png, rgba_to_png(&rgba, size)))
        })
        .clone()
}

fn rgba_to_png(rgba: &[u8], size: u32) -> Vec<u8> {
    let mut premultiplied = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        let a = u16::from(px[3]);
        for channel in &px[..3] {
            premultiplied.push(((u16::from(*channel) * a + 127) / 255) as u8);
        }
        premultiplied.push(px[3]);
    }
    resvg::tiny_skia::Pixmap::from_vec(
        premultiplied,
        resvg::tiny_skia::IntSize::from_wh(size, size).expect("non-zero preview size"),
    )
    .and_then(|pixmap| pixmap.encode_png().ok())
    .unwrap_or_default()
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
