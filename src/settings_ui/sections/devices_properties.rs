//! Slide-in Device Properties sub-view inside the Devices tab.
//! Replaces the most-used tabs of the classic Windows Sound properties dialog
//! (Levels, Balance, Test, Listen to this device, Boost/AGC, Format, Exclusive
//! mode, Enable/Disable) with immediate reactive application.

use crate::audio_devices::{self, DeviceEntry, DeviceProperties, ListenError};
use crate::settings_ui::{
    SettingsApp,
    assets::icon,
    color::{TRANSPARENT, lerp, mix_pct},
    controls::{
        self, RangeOptions, SelectOption, SelectOptions, TextInputEvent, TextInputOptions,
    },
    sections::form,
    theme::Palette,
};
use gpui::{prelude::*, *};

pub(crate) fn render(
    app: &mut SettingsApp,
    entry: DeviceEntry,
    props: DeviceProperties,
    listen_access_denied: bool,
    spatial_error: Option<String>,
    peak: f32,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> AnyElement {
    let p = app.palette;
    let mode = app
        .snapshot
        .config
        .advanced
        .audio_device_name_display
        .clone();
    let rename_allowed = mode != crate::AUDIO_DEVICE_NAME_SYSTEM;
    let title = entry.display_name(&mode);
    let pretty_name = entry.name.clone();
    let subtitle = entry.subtitle(&mode);
    let id = entry.id.clone();
    let input = entry.input;

    // Header: Back button + Windows properties link
    let back_id = ElementId::from("device-props-back");
    let (back_ptr, back_hover, back_press) = controls::hover_tween(back_id.clone(), window, cx);
    let back_btn = controls::track_pointer(
        div()
            .id(back_id)
            .h(px(34.))
            .pl(px(8.))
            .pr(px(12.))
            .rounded(px(9.))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .bg(lerp(
                lerp(TRANSPARENT, p.surface_3, back_hover),
                p.surface_2,
                back_press,
            )
            .hsla())
            .text_size(px(13.5))
            .font_weight(FontWeight(620.))
            .text_color(lerp(p.muted, p.text, back_hover).hsla())
            .child(
                svg()
                    .path(icon("alt-arrow-left-linear"))
                    .size(px(16.))
                    .text_color(lerp(p.muted, p.text, back_hover).hsla()),
            )
            .child("Devices"),
        back_ptr,
    )
    .on_click(cx.listener(|app, _, _, cx| {
        let snapshot = app.snapshot.clone();
        app.sections.devices.close_properties_with(&snapshot);
        cx.notify();
    }));

    let win_props_id = ElementId::from("device-props-win");
    let (win_ptr, win_hover, win_press) = controls::hover_tween(win_props_id.clone(), window, cx);
    let win_id = id.clone();
    let win_btn = controls::track_pointer(
        div()
            .id(win_props_id)
            .h(px(32.))
            .px(px(11.))
            .rounded(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .bg(lerp(
                lerp(mix_pct(p.text, 5.5, TRANSPARENT), p.surface_3, win_hover),
                p.surface_2,
                win_press,
            )
            .hsla())
            .text_size(px(12.5))
            .font_weight(FontWeight(600.))
            .text_color(lerp(p.muted, p.text, win_hover).hsla())
            .child("Windows properties")
            .child(
                svg()
                    .path(icon("square-arrow-right-up-linear"))
                    .size(px(14.))
                    .text_color(lerp(p.muted, p.text, win_hover).hsla()),
            ),
        win_ptr,
    )
    .on_click(cx.listener(move |_, _, _, _| {
        let _ = crate::open_audio_device_properties(&win_id, input);
    }));

    let top_bar = div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .mb(px(14.))
        .child(back_btn)
        .child(win_btn);

    // Device identity banner with inline rename
    let (renaming, rename_draft, rename_gen, rename_focus_pending) = app
        .sections
        .devices
        .open_properties_mut()
        .map(|o| {
            let focus = std::mem::take(&mut o.rename_focus_pending);
            (o.renaming, o.rename_draft.clone(), o.rename_generation, focus)
        })
        .unwrap_or((false, String::new(), 0, false));

    let title_block = if renaming {
        let id_ev = id.clone();
        let orig_ev = pretty_name.clone();
        let input_entity = controls::text_input(
            SharedString::from(format!("props-rename-input-{id}-{rename_gen}")),
            rename_draft,
            TextInputOptions {
                caret_color: Some(p.accent.hsla()),
                ..Default::default()
            },
            window,
            cx,
            move |app, event, window, cx| {
                let valid = app
                    .sections
                    .devices
                    .open_properties()
                    .is_some_and(|o| o.id == id_ev && o.renaming && o.rename_generation == rename_gen);
                if !valid {
                    return;
                }
                match event {
                    TextInputEvent::Change(value) => {
                        if let Some(o) = app.sections.devices.open_properties_mut() {
                            o.rename_draft = value.to_string();
                        }
                        cx.notify();
                    }
                    TextInputEvent::Blur(value) if value.trim().is_empty() => {
                        if let Some(o) = app.sections.devices.open_properties_mut() {
                            o.rename_draft = orig_ev.clone();
                        }
                        let old = orig_ev.clone();
                        let id_defer = id_ev.clone();
                        cx.defer_in(window, move |app, _, cx| {
                            if let Some(o) = app.sections.devices.open_properties()
                                && o.id == id_defer
                                && o.renaming
                                && o.rename_generation == rename_gen
                                && let Some(inp) = &o.rename_input
                            {
                                inp.update(cx, |inp, cx| inp.set_value(old, cx));
                            }
                        });
                        cx.notify();
                    }
                    TextInputEvent::Enter(_) => {
                        finish_props_rename(app, &id_ev, &orig_ev, true, window, cx)
                    }
                    TextInputEvent::Escape => {
                        finish_props_rename(app, &id_ev, &orig_ev, false, window, cx)
                    }
                    _ => {}
                }
            },
        );
        if let Some(o) = app.sections.devices.open_properties_mut() {
            o.rename_input = Some(input_entity.clone());
        }
        if rename_focus_pending {
            let id_focus = id.clone();
            let focus_inp = input_entity.clone();
            cx.defer_in(window, move |app, window, cx| {
                if app
                    .sections
                    .devices
                    .open_properties()
                    .is_some_and(|o| o.id == id_focus && o.renaming && o.rename_generation == rename_gen)
                {
                    focus_inp.update(cx, |inp, cx| inp.focus_at_end(window, cx));
                }
            });
        }
        let commit_id = id.clone();
        let commit_orig = pretty_name.clone();
        let cancel_id = id.clone();
        let cancel_orig = pretty_name.clone();
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .min_w_0()
            .text_size(px(15.5))
            .font_weight(FontWeight(700.))
            .child(
                controls::text_input_container(
                    SharedString::from(format!("props-rename-shell-{id}")),
                    &input_entity,
                )
                .flex_1(),
            )
            .child(
                div()
                    .id(SharedString::from(format!("props-rename-ok-{rename_gen}")))
                    .size(px(26.))
                    .rounded(px(7.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .bg(p.surface_3.hsla())
                    .child(
                        svg()
                            .path(icon("check-mark"))
                            .size(px(13.))
                            .text_color(p.success.hsla()),
                    )
                    .on_click(cx.listener(move |app, _, window, cx| {
                        finish_props_rename(app, &commit_id, &commit_orig, true, window, cx);
                    })),
            )
            .child(
                div()
                    .id(SharedString::from(format!("props-rename-cancel-{rename_gen}")))
                    .size(px(26.))
                    .rounded(px(7.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .bg(p.surface_3.hsla())
                    .child(
                        svg()
                            .path(icon("codicon_close"))
                            .size(px(13.))
                            .text_color(p.danger.hsla()),
                    )
                    .on_click(cx.listener(move |app, _, window, cx| {
                        finish_props_rename(app, &cancel_id, &cancel_orig, false, window, cx);
                    })),
            )
            .into_any_element()
    } else {
        let rename_btn = rename_allowed.then(|| {
            let btn_id = ElementId::Name(SharedString::from(format!("props-rename-btn-{id}")));
            let (r_ptr, r_hover, r_press) = controls::hover_tween(btn_id.clone(), window, cx);
            let orig_click = pretty_name.clone();
            controls::track_pointer(
                div()
                    .id(btn_id)
                    .size(px(24.))
                    .my(px(-2.))
                    .rounded(px(6.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .bg(lerp(
                        lerp(TRANSPARENT, p.surface_3, r_hover),
                        p.surface_2,
                        r_press,
                    )
                    .hsla())
                    .child(
                        svg()
                            .path(icon("pen-linear"))
                            .size(px(14.))
                            .text_color(lerp(p.muted, p.text, r_hover).hsla()),
                    ),
                r_ptr,
            )
            .on_click(cx.listener(move |app, _, _, cx| {
                if let Some(o) = app.sections.devices.open_properties_mut() {
                    o.rename_draft = orig_click.clone();
                    o.renaming = true;
                    o.rename_focus_pending = true;
                    o.rename_generation += 1;
                }
                cx.notify();
            }))
        });
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .min_w_0()
            .child(
                div()
                    .text_size(px(16.))
                    .line_height(px(20.))
                    .font_weight(FontWeight(700.))
                    .text_ellipsis()
                    .child(title),
            )
            .children(rename_btn)
            .into_any_element()
    };

    let status_pill = if entry.is_default {
        Some(status_badge("Default", true, p))
    } else if !entry.state.is_active() {
        Some(status_badge(entry.state.label(), false, p))
    } else {
        None
    };

    let identity = div()
        .flex()
        .items_center()
        .gap(px(14.))
        .px(px(form::ROW_PAD))
        .py(px(14.))
        .mb(px(20.))
        .rounded(px(14.))
        .bg(p.surface_2.hsla())
        .child(
            div()
                .size(px(42.))
                .rounded(px(11.))
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .bg(if entry.is_default {
                    p.accent_soft.hsla()
                } else {
                    p.surface_3.hsla()
                })
                .child(
                    svg()
                        .path(icon(entry.icon_name()))
                        .size(px(22.))
                        .text_color(if entry.is_default {
                            p.accent_text.hsla()
                        } else {
                            p.text.hsla()
                        }),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .flex_1()
                .gap(px(3.))
                .child(title_block)
                .when_some(subtitle, |this, sub| {
                    this.child(
                        div()
                            .text_size(px(12.5))
                            .line_height(px(16.))
                            .text_color(p.muted.hsla())
                            .text_ellipsis()
                            .child(sub),
                    )
                }),
        )
        .children(status_pill);

    let mut sections_col = div()
        .flex()
        .flex_col()
        .gap(px(form::SECTION_GAP))
        .w_full();

    // 1. Levels & Test (when active)
    if props.active {
        let vol_pct = (props.volume * 100.0).round().clamp(0.0, 100.0);
        let vol_id_cb = id.clone();
        let volume_icon = if props.muted {
            if input {
                "microphone-3-off-linear"
            } else {
                "volume-cross-linear"
            }
        } else if input {
            "microphone-3-linear"
        } else {
            "volume-loud-linear"
        };

        let mute_btn_id = ElementId::Name(SharedString::from(format!("props-mute-{id}")));
        let (mute_ptr, mute_hover, mute_press) = controls::hover_tween(mute_btn_id.clone(), window, cx);
        let mute_id_cb = id.clone();
        let next_muted = !props.muted;
        let mute_btn = controls::track_pointer(
            div()
                .id(mute_btn_id)
                .size(px(36.))
                .rounded(px(10.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .bg(if props.muted {
                    lerp(mix_pct(p.danger, 18., TRANSPARENT), mix_pct(p.danger, 28., TRANSPARENT), mute_hover).hsla()
                } else {
                    lerp(lerp(p.surface_3, p.surface_color[4], mute_hover), p.surface_2, mute_press).hsla()
                })
                .child(
                    svg()
                        .path(icon(volume_icon))
                        .size(px(17.))
                        .text_color(if props.muted {
                            p.danger.hsla()
                        } else {
                            p.text.hsla()
                        }),
                ),
            mute_ptr,
        )
        .on_click(cx.listener(move |app, _, _, cx| {
            if audio_devices::set_device_mute(&mute_id_cb, next_muted).is_ok() {
                if let Some(open) = app.sections.devices.open_properties_mut() {
                    open.props.muted = next_muted;
                }
                cx.notify();
            }
        }));

        let volume_row = controls::range_row(
            SharedString::from(format!("props-vol-{id}")),
            f64::from(vol_pct),
            RangeOptions {
                min: 0.,
                max: 100.,
                step: 1.,
                label: "Volume".into(),
                value_label: format!("{}%", vol_pct as u32).into(),
                icon: None,
            },
            window,
            cx,
            move |app, val, _, cx| {
                let scalar = (val as f32 / 100.0).clamp(0.0, 1.0);
                if audio_devices::set_device_volume(&vol_id_cb, scalar).is_ok() {
                    if let Some(open) = app.sections.devices.open_properties_mut() {
                        let prev_master = open.props.volume;
                        open.props.volume = scalar;
                        if open.props.channel_volumes.len() == 2 {
                            if prev_master > 0.001 {
                                let ratio = scalar / prev_master;
                                for ch in &mut open.props.channel_volumes {
                                    *ch = (*ch * ratio).clamp(0.0, 1.0);
                                }
                            } else {
                                open.props.channel_volumes[0] = scalar;
                                open.props.channel_volumes[1] = scalar;
                            }
                        }
                    }
                    cx.notify();
                }
            },
        )
        .child(mute_btn);

        let meter_row = div()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(form::ROW_PAD))
            .py(px(12.))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(FontWeight(600.))
                    .text_color(p.muted.hsla())
                    .child("Live level"),
            )
            .child(horizontal_peak_bar(peak, p));

        let mut levels_card = form::card(cx).row(meter_row).row(volume_row);

        // Stereo balance (L / R) — shown only when the endpoint actually supports
        // independent per-channel volume control.
        if props.supports_balance {
            let bal_pct = (props.balance * 100.0).round().clamp(-100.0, 100.0);
            let bal_label = if bal_pct.abs() < 1.0 {
                "Center".to_string()
            } else if bal_pct < 0.0 {
                format!("L {}%", (-bal_pct) as u32)
            } else {
                format!("R {}%", bal_pct as u32)
            };
            let bal_id_cb = id.clone();
            let mut bal_row = controls::range_row(
                SharedString::from(format!("props-bal-{id}")),
                f64::from(bal_pct),
                RangeOptions {
                    min: -100.,
                    max: 100.,
                    step: 1.,
                    label: "Balance (L / R)".into(),
                    value_label: bal_label.into(),
                    icon: None,
                },
                window,
                cx,
                move |app, val, _, cx| {
                    let b = (val as f32 / 100.0).clamp(-1.0, 1.0);
                    if audio_devices::set_device_balance(&bal_id_cb, b).is_ok() {
                        if let Some(open) = app.sections.devices.open_properties_mut() {
                            open.props.balance = b;
                        }
                        cx.notify();
                    }
                },
            );
            if bal_pct.abs() >= 1.0 {
                let reset_id = ElementId::Name(SharedString::from(format!("props-bal-reset-{id}")));
                let (r_ptr, r_hover, _) = controls::hover_tween(reset_id.clone(), window, cx);
                let bal_reset_cb = id.clone();
                bal_row = bal_row.child(
                    controls::track_pointer(
                        div()
                            .id(reset_id)
                            .h(px(30.))
                            .px(px(10.))
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .bg(lerp(p.surface_3, p.surface_color[4], r_hover).hsla())
                            .text_size(px(12.))
                            .font_weight(FontWeight(600.))
                            .text_color(p.text.hsla())
                            .child("Center"),
                        r_ptr,
                    )
                    .on_click(cx.listener(move |app, _, _, cx| {
                        if audio_devices::set_device_balance(&bal_reset_cb, 0.0).is_ok() {
                            if let Some(open) = app.sections.devices.open_properties_mut() {
                                open.props.balance = 0.0;
                            }
                            cx.notify();
                        }
                    })),
                );
            }
            levels_card = levels_card.row(bal_row);
        }

        // Output: Test speakers
        if !input {
            let test_id = ElementId::Name(SharedString::from(format!("props-test-{id}")));
            let (t_ptr, t_hover, t_press) = controls::hover_tween(test_id.clone(), window, cx);
            let test_dev_id = id.clone();
            let test_btn = controls::track_pointer(
                div()
                    .id(test_id)
                    .h(px(34.))
                    .px(px(14.))
                    .rounded(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .cursor_pointer()
                    .bg(lerp(
                        lerp(p.accent_soft, mix_pct(p.accent, 24., TRANSPARENT), t_hover),
                        p.accent_soft,
                        t_press,
                    )
                    .hsla())
                    .text_size(px(13.))
                    .font_weight(FontWeight(650.))
                    .text_color(p.accent_text.hsla())
                    .child(
                        svg()
                            .path(icon("play-circle-linear"))
                            .size(px(16.))
                            .text_color(p.accent_text.hsla()),
                    )
                    .child("Test"),
                t_ptr,
            )
            .on_click(cx.listener(move |_, _, _, _| {
                let _ = audio_devices::play_test_tone(&test_dev_id);
            }));

            levels_card = levels_card.row(form::setting_row(
                "Test audio",
                Some("Plays a short left-then-right chime directly on this device"),
                test_btn,
                cx,
            ));
        }

        sections_col = sections_col.child(
            form::block()
                .child(form::section_heading("Levels", cx))
                .child(levels_card),
        );
    }

    // 2. Input-specific: Listen to this device + Boost / AGC
    if input && props.active {
        let listen_on = props.listen_enabled.unwrap_or(false);
        let listen_dev_id = id.clone();
        let listen_target_for_toggle = props.listen_target_id.clone();
        let listen_toggle = controls::toggle(
            SharedString::from(format!("props-listen-{id}")),
            listen_on,
            window,
            cx,
            move |app, value, _, cx| match audio_devices::set_device_listen(
                &listen_dev_id,
                value,
                &listen_target_for_toggle,
            ) {
                Ok(()) => {
                    if let Some(open) = app.sections.devices.open_properties_mut() {
                        open.props.listen_enabled = Some(value);
                        open.listen_access_denied = false;
                    }
                    cx.notify();
                }
                Err(ListenError::AccessDenied) => {
                    if let Some(open) = app.sections.devices.open_properties_mut() {
                        open.listen_access_denied = true;
                    }
                    cx.notify();
                }
                Err(_) => {}
            },
        );

        let mut target_options = vec![
            SelectOption::new("", "Default playback device").icon("volume-loud-linear"),
        ];
        for out in &app.snapshot.output_devices {
            target_options.push(
                SelectOption::new(out.id.clone(), out.display_name(&mode))
                    .icon("volume-loud-linear"),
            );
        }
        let mut target_select_opts = SelectOptions::new(target_options)
            .row("Playback through this device");
        target_select_opts.show_current_detail = false;
        let target_dev_id = id.clone();
        let target_select = controls::select(
            format!("props-listen-target-{id}"),
            props.listen_target_id.clone(),
            target_select_opts,
            app,
            window,
            cx,
            move |app, value, _, cx| {
                let enabled = app
                    .sections
                    .devices
                    .open_properties()
                    .and_then(|o| o.props.listen_enabled)
                    .unwrap_or(true);
                match audio_devices::set_device_listen(&target_dev_id, enabled, &value) {
                    Ok(()) => {
                        if let Some(open) = app.sections.devices.open_properties_mut() {
                            open.props.listen_target_id = value.to_string();
                            open.listen_access_denied = false;
                        }
                        cx.notify();
                    }
                    Err(ListenError::AccessDenied) => {
                        if let Some(open) = app.sections.devices.open_properties_mut() {
                            open.listen_access_denied = true;
                        }
                        cx.notify();
                    }
                    Err(_) => {}
                }
            },
        );

        let mut input_card = form::card(cx)
            .row(form::setting_row(
                "Listen to this device",
                Some("Routes this microphone directly to a playback device via Windows"),
                listen_toggle,
                cx,
            ))
            .reveal("props-listen-target-reveal", listen_on, target_select, window, cx);

        if listen_access_denied {
            let win_id_notice = id.clone();
            let btn_id = ElementId::from("props-listen-open-win");
            let (b_ptr, b_hover, b_press) = controls::hover_tween(btn_id.clone(), window, cx);
            let notice = div()
                .w_full()
                .min_w_0()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .px(px(form::ROW_PAD))
                .py(px(12.))
                .bg(mix_pct(p.danger, 10., TRANSPARENT).hsla())
                .when(props.mic_boost.is_none() && props.mic_agc.is_none(), |this| {
                    this.rounded_bl(px(14.)).rounded_br(px(14.))
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.5))
                        .line_height(relative(1.35))
                        .text_color(p.text.hsla())
                        .child("Windows denied permission to change Listen to this device without administrator rights."),
                )
                .child(
                    controls::track_pointer(
                        div()
                            .id(btn_id)
                            .px(px(10.))
                            .py(px(6.))
                            .rounded(px(8.))
                            .flex_shrink_0()
                            .cursor_pointer()
                            .bg(lerp(
                                lerp(p.surface_3, p.surface_color[4], b_hover),
                                p.surface_2,
                                b_press,
                            )
                            .hsla())
                            .text_size(px(12.))
                            .font_weight(FontWeight(600.))
                            .text_color(p.text.hsla())
                            .child("Open Windows properties"),
                        b_ptr,
                    )
                    .on_click(cx.listener(move |_, _, _, _| {
                        let _ = crate::open_audio_device_properties(&win_id_notice, true);
                    })),
                );
            input_card = input_card.row(notice);
        }

        if let Some(boost) = props.mic_boost.clone() {
            let boost_dev_id = id.clone();
            let boost_row = controls::range_row(
                SharedString::from(format!("props-boost-{id}")),
                f64::from(boost.current_db),
                RangeOptions {
                    min: f64::from(boost.min_db),
                    max: f64::from(boost.max_db),
                    step: f64::from(boost.step_db),
                    label: "Microphone boost".into(),
                    value_label: format!("+{:.0} dB", boost.current_db).into(),
                    icon: None,
                },
                window,
                cx,
                move |app, val, _, cx| {
                    let db = val as f32;
                    if audio_devices::set_device_mic_boost(&boost_dev_id, db).is_ok() {
                        if let Some(open) = app.sections.devices.open_properties_mut()
                            && let Some(b) = open.props.mic_boost.as_mut()
                        {
                            b.current_db = db;
                        }
                        cx.notify();
                    }
                },
            );
            input_card = input_card.row(boost_row);
        }

        if let Some(agc_on) = props.mic_agc {
            let agc_dev_id = id.clone();
            let agc_toggle = controls::toggle(
                SharedString::from(format!("props-agc-{id}")),
                agc_on,
                window,
                cx,
                move |app, value, _, cx| {
                    if audio_devices::set_device_mic_agc(&agc_dev_id, value).is_ok() {
                        if let Some(open) = app.sections.devices.open_properties_mut() {
                            open.props.mic_agc = Some(value);
                        }
                        cx.notify();
                    }
                },
            );
            input_card = input_card.row(form::setting_row(
                "Automatic gain control",
                Some("Let the audio driver adjust microphone gain automatically"),
                agc_toggle,
                cx,
            ));
        }

        sections_col = sections_col.child(
            form::block()
                .child(form::section_heading("Input", cx))
                .child(input_card),
        );
    }

    // 2.5 Enhancements & Spatial audio (when active)
    if props.active && (props.enhancements_enabled.is_some() || props.spatial_supported) {
        let mut fx_card = form::card(cx);

        if let Some(enh_on) = props.enhancements_enabled {
            let enh_dev_id = id.clone();
            let enh_toggle = controls::toggle(
                SharedString::from(format!("props-enhancements-{id}")),
                enh_on,
                window,
                cx,
                move |app, value, _, cx| {
                    if audio_devices::set_device_enhancements(&enh_dev_id, value).is_ok() {
                        if let Some(open) = app.sections.devices.open_properties_mut() {
                            open.props.enhancements_enabled = Some(value);
                        }
                        cx.notify();
                    }
                },
            );
            fx_card = fx_card.row(form::setting_row(
                "Audio enhancements",
                Some("Enable system audio effects and APO processing for this device"),
                enh_toggle,
                cx,
            ));
        }

        if props.spatial_supported && !props.spatial_options.is_empty() {
            let spatial_opts: Vec<SelectOption> = props
                .spatial_options
                .iter()
                .map(|o| {
                    SelectOption::new(o.id.clone(), o.label.clone())
                        .icon("headphones-round-sound-linear")
                })
                .collect();
            let mut select_opts = SelectOptions::new(spatial_opts).row("Spatial sound");
            select_opts.show_current_detail = false;
            let spatial_dev_id = id.clone();
            let spatial_select = controls::select(
                format!("props-spatial-{id}"),
                props.spatial_current.clone(),
                select_opts,
                app,
                window,
                cx,
                move |app, value, _, cx| {
                    match audio_devices::set_device_spatial_audio(&spatial_dev_id, &value) {
                        Ok(()) => {
                            if let Some(open) = app.sections.devices.open_properties_mut() {
                                open.props.spatial_current = value.to_string();
                                open.spatial_error = None;
                            }
                            cx.notify();
                        }
                        Err(msg) => {
                            if let Some(open) = app.sections.devices.open_properties_mut() {
                                open.spatial_error = Some(msg);
                            }
                            cx.notify();
                        }
                    }
                },
            );
            fx_card = fx_card.row(spatial_select);

            if let Some(err_msg) = spatial_error {
                let win_id_spatial = id.clone();
                let btn_id = ElementId::from("props-spatial-open-win");
                let (b_ptr, b_hover, b_press) = controls::hover_tween(btn_id.clone(), window, cx);
                let notice = div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .px(px(form::ROW_PAD))
                    .py(px(12.))
                    .rounded_bl(px(14.))
                    .rounded_br(px(14.))
                    .bg(mix_pct(p.danger, 10., TRANSPARENT).hsla())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.5))
                            .line_height(relative(1.35))
                            .text_color(p.text.hsla())
                            .child(err_msg),
                    )
                    .child(
                        controls::track_pointer(
                            div()
                                .id(btn_id)
                                .px(px(10.))
                                .py(px(6.))
                                .rounded(px(8.))
                                .flex_shrink_0()
                                .cursor_pointer()
                                .bg(lerp(
                                    lerp(p.surface_3, p.surface_color[4], b_hover),
                                    p.surface_2,
                                    b_press,
                                )
                                .hsla())
                                .text_size(px(12.))
                                .font_weight(FontWeight(600.))
                                .text_color(p.text.hsla())
                                .child("Open Windows properties"),
                            b_ptr,
                        )
                        .on_click(cx.listener(move |_, _, _, _| {
                            let _ = crate::open_audio_device_properties(&win_id_spatial, false);
                        })),
                    );
                fx_card = fx_card.row(notice);
            }
        }

        let heading = if props.spatial_supported {
            "Enhancements & Spatial sound"
        } else {
            "Enhancements"
        };
        sections_col = sections_col.child(
            form::block()
                .child(form::section_heading(heading, cx))
                .child(fx_card),
        );
    }

    // 3. Format & Exclusive Mode (when active)
    if props.active {
        let mut adv_card = form::card(cx);

        if !props.supported_formats.is_empty() {
            let fmt_options: Vec<SelectOption> = props
                .supported_formats
                .iter()
                .map(|f| SelectOption::new(f.id(), f.label()).icon("tuning-2-linear"))
                .collect();
            let current_fmt_id = props
                .current_format
                .as_ref()
                .map(|f| f.id())
                .unwrap_or_default();
            let mut select_opts = SelectOptions::new(fmt_options).row("Default format");
            select_opts.show_current_detail = false;
            let fmt_dev_id = id.clone();
            let formats_for_cb = props.supported_formats.clone();
            let fmt_select = controls::select(
                format!("props-format-{id}"),
                current_fmt_id,
                select_opts,
                app,
                window,
                cx,
                move |app, value, _, cx| {
                    if let Some(chosen) = formats_for_cb.iter().find(|f| f.id() == value.as_ref()) {
                        if audio_devices::set_device_format(&fmt_dev_id, chosen).is_ok() {
                            if let Some(open) = app.sections.devices.open_properties_mut() {
                                open.props.current_format = Some(chosen.clone());
                            }
                            cx.notify();
                        }
                    }
                },
            );
            adv_card = adv_card.row(fmt_select);
        }

        let allow_excl = props.allow_exclusive.unwrap_or(true);
        let excl_prio = props.exclusive_priority.unwrap_or(true);
        let excl_dev_id_1 = id.clone();
        let allow_toggle = controls::toggle(
            SharedString::from(format!("props-excl-allow-{id}")),
            allow_excl,
            window,
            cx,
            move |app, value, _, cx| {
                let prio = app
                    .sections
                    .devices
                    .open_properties()
                    .and_then(|o| o.props.exclusive_priority)
                    .unwrap_or(true);
                if audio_devices::set_device_exclusive(&excl_dev_id_1, value, prio).is_ok() {
                    if let Some(open) = app.sections.devices.open_properties_mut() {
                        open.props.allow_exclusive = Some(value);
                    }
                    cx.notify();
                }
            },
        );
        let excl_dev_id_2 = id.clone();
        let prio_toggle = controls::toggle(
            SharedString::from(format!("props-excl-prio-{id}")),
            excl_prio,
            window,
            cx,
            move |app, value, _, cx| {
                let allow = app
                    .sections
                    .devices
                    .open_properties()
                    .and_then(|o| o.props.allow_exclusive)
                    .unwrap_or(true);
                if audio_devices::set_device_exclusive(&excl_dev_id_2, allow, value).is_ok() {
                    if let Some(open) = app.sections.devices.open_properties_mut() {
                        open.props.exclusive_priority = Some(value);
                    }
                    cx.notify();
                }
            },
        );

        adv_card = adv_card
            .row(form::setting_row(
                "Allow applications to take exclusive control",
                None,
                allow_toggle,
                cx,
            ))
            .reveal(
                "props-excl-prio-reveal",
                allow_excl,
                form::setting_row(
                    "Give exclusive mode applications priority",
                    None,
                    prio_toggle,
                    cx,
                ),
                window,
                cx,
            );

        sections_col = sections_col.child(
            form::block()
                .child(form::section_heading("Format & Exclusive mode", cx))
                .child(adv_card),
        );
    }

    // 4. Device state (Enable / Disable)
    if entry.state.is_active() || entry.state.is_disabled() {
        let is_enabled = entry.state.is_active();
        let vis_dev_id = id.clone();
        let vis_toggle = controls::toggle(
            SharedString::from(format!("props-enabled-{id}")),
            is_enabled,
            window,
            cx,
            move |app, value, _, cx| {
                if audio_devices::set_device_enabled(&vis_dev_id, value).is_ok() {
                    app.refresh_devices(cx);
                    app.sections.devices.refresh_open_properties();
                    cx.notify();
                }
            },
        );
        sections_col = sections_col.child(
            form::block()
                .child(form::section_heading("Device state", cx))
                .child(form::card(cx).row(form::setting_row(
                    "Use this device (enabled)",
                    Some("Disabling hides the device from Windows and apps until re-enabled"),
                    vis_toggle,
                    cx,
                ))),
        );
    }

    div()
        .flex()
        .flex_col()
        .max_w(px(720.))
        .w_full()
        .child(top_bar)
        .child(identity)
        .child(sections_col)
        .into_any_element()
}

fn finish_props_rename(
    app: &mut SettingsApp,
    device_id: &str,
    original_pretty_name: &str,
    commit: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) {
    let Some(o) = app.sections.devices.open_properties_mut() else {
        return;
    };
    if !o.renaming {
        return;
    }
    let next = o.rename_draft.trim().to_string();
    o.renaming = false;
    o.rename_focus_pending = false;
    o.rename_draft = original_pretty_name.to_string();
    o.rename_input = None;
    if commit && !next.is_empty() && next != original_pretty_name {
        let _ = crate::rename_audio_device(device_id, &next);
        app.refresh_devices(cx);
    }
    app.focus_handle.focus(window, cx);
    cx.notify();
}

fn status_badge(label: &'static str, accent: bool, p: Palette) -> Div {
    div()
        .h(px(26.))
        .px(px(11.))
        .rounded_full()
        .flex()
        .items_center()
        .flex_shrink_0()
        .bg(if accent {
            p.accent_soft.hsla()
        } else {
            p.surface_3.hsla()
        })
        .text_size(px(12.))
        .font_weight(FontWeight(620.))
        .text_color(if accent {
            p.accent_text.hsla()
        } else {
            p.muted.hsla()
        })
        .child(label)
}

fn horizontal_peak_bar(peak: f32, p: Palette) -> Div {
    // Same zones as the device list's meter: green, then accent, red only
    // near full scale. A solid bar turned red on ordinary loud audio.
    const SEGMENTS: usize = 32;
    let clamped = peak.clamp(0.0, 1.0);
    let mut row = div().flex_1().h(px(8.)).flex().gap(px(2.));
    for i in 0..SEGMENTS {
        let position = (i as f32 + 0.5) / SEGMENTS as f32;
        let color = if clamped < position {
            p.surface_3
        } else if position > 0.875 {
            p.danger
        } else if position > 0.625 {
            p.accent
        } else {
            p.success
        };
        row = row.child(div().flex_1().h_full().rounded(px(1.5)).bg(color.hsla()));
    }
    row
}
