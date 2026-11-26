//! Devices tab: modern replacement for Windows Sound Control Panel (`mmsys.cpl`).
//!
//! Features Output/Input tabs, disabled & disconnected visibility, FormFactor
//! icons, live peak meters, optional separate Default Communication role,
//! right-click context menu, and a slide-in Properties sub-view.

use crate::audio_devices::{self, DeviceEntry, DeviceProperties};
use crate::settings_ui::{
    SettingsApp, SettingsSnapshot,
    anim::{self, Easing, Transition, UI},
    assets::icon,
    color::{TRANSPARENT, lerp, mix_pct},
    controls::{
        self, SegmentedToggleOption, SelectOption, SelectOptions, TextInput, TextInputEvent,
        TextInputOptions,
    },
    style::palette,
    theme::Palette,
};
use gpui::{prelude::*, *};
use std::{
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

const VIEW_SLIDE_DURATION: Duration = Duration::from_millis(240);
const TAB_SLIDE_DURATION: Duration = Duration::from_millis(210);
const SLIDE_EASE: Easing = Easing::Bezier(0.22, 1., 0.36, 1.);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DeviceFlowTab {
    #[default]
    Output,
    Input,
}

impl DeviceFlowTab {
    fn as_str(self) -> &'static str {
        match self {
            Self::Output => "output",
            Self::Input => "input",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "input" => Self::Input,
            _ => Self::Output,
        }
    }
}

pub(crate) struct OpenDevice {
    pub id: String,
    pub input: bool,
    pub props: DeviceProperties,
    pub listen_access_denied: bool,
    pub spatial_error: Option<String>,
    pub renaming: bool,
    pub rename_draft: String,
    pub rename_input: Option<Entity<TextInput>>,
    pub rename_focus_pending: bool,
    pub rename_generation: u64,
}

struct ContextMenu {
    device_id: String,
    input: bool,
    position: Point<Pixels>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewSlideDirection {
    Enter,
    Exit,
}

struct ViewTransition {
    direction: ViewSlideDirection,
    started_at: Instant,
    outgoing_props: Option<(DeviceEntry, DeviceProperties, bool, Option<String>, f32)>,
}

struct TabSlide {
    from: DeviceFlowTab,
    to: DeviceFlowTab,
    started_at: Instant,
}

#[derive(Default)]
pub(crate) struct State {
    active_tab: DeviceFlowTab,
    tab_slide: Option<TabSlide>,
    rows: HashMap<String, Rename>,
    peaks: HashMap<String, f32>,
    open_device: Option<OpenDevice>,
    view_transition: Option<ViewTransition>,
    context_menu: Option<ContextMenu>,
}

impl State {
    pub(crate) fn open_properties(&self) -> Option<&OpenDevice> {
        self.open_device.as_ref()
    }

    pub(crate) fn open_properties_mut(&mut self) -> Option<&mut OpenDevice> {
        self.open_device.as_mut()
    }

    pub(crate) fn open_device_properties(&mut self, id: String, input: bool) {
        self.context_menu = None;
        if let Ok(props) = audio_devices::read_device_properties(&id, input) {
            self.open_device = Some(OpenDevice {
                id,
                input,
                props,
                listen_access_denied: false,
                spatial_error: None,
                renaming: false,
                rename_draft: String::new(),
                rename_input: None,
                rename_focus_pending: false,
                rename_generation: 0,
            });
            self.view_transition = Some(ViewTransition {
                direction: ViewSlideDirection::Enter,
                started_at: Instant::now(),
                outgoing_props: None,
            });
        }
    }

    pub(crate) fn close_properties_with(&mut self, snapshot: &SettingsSnapshot) {
        let outgoing = self.open_device.take().and_then(|o| {
            let entry = if o.input {
                snapshot
                    .all_input_devices
                    .iter()
                    .find(|d| d.id == o.id)
                    .cloned()
            } else {
                snapshot
                    .all_output_devices
                    .iter()
                    .find(|d| d.id == o.id)
                    .cloned()
            }?;
            let peak = self.peaks.get(&o.id).copied().unwrap_or(0.0);
            Some((entry, o.props, o.listen_access_denied, o.spatial_error, peak))
        });
        if outgoing.is_some() {
            self.view_transition = Some(ViewTransition {
                direction: ViewSlideDirection::Exit,
                started_at: Instant::now(),
                outgoing_props: outgoing,
            });
        } else {
            self.view_transition = None;
        }
    }

    pub(crate) fn close_properties(&mut self) {
        self.open_device = None;
        self.view_transition = None;
    }

    pub(crate) fn refresh_open_properties(&mut self) {
        let Some(open) = self.open_device.as_mut() else {
            return;
        };
        if let Ok(props) = audio_devices::read_device_properties(&open.id, open.input) {
            open.props = props;
        }
    }

    /// Returns `(all_metered_ids, active_input_ids_needing_capture_stream)` for
    /// the devices currently visible on screen.
    pub(crate) fn metered_device_ids(
        &self,
        snapshot: &SettingsSnapshot,
    ) -> (Vec<String>, Vec<String>) {
        if let Some(open) = &self.open_device {
            if !open.props.active {
                return (Vec::new(), Vec::new());
            }
            let ids = vec![open.id.clone()];
            let inputs = if open.input { ids.clone() } else { Vec::new() };
            return (ids, inputs);
        }
        let list = match self.active_tab {
            DeviceFlowTab::Output => &snapshot.all_output_devices,
            DeviceFlowTab::Input => &snapshot.all_input_devices,
        };
        let ids: Vec<String> = list
            .iter()
            .filter(|d| d.state.is_active())
            .map(|d| d.id.clone())
            .collect();
        let inputs = if self.active_tab == DeviceFlowTab::Input {
            ids.clone()
        } else {
            Vec::new()
        };
        (ids, inputs)
    }

    /// Smoothly blends sampled peaks (instant attack, gentle decay). Returns
    /// true when any meter visibly moved.
    pub(crate) fn update_peaks(&mut self, sampled: HashMap<String, f32>) -> bool {
        let mut changed = false;
        self.peaks.retain(|id, _| sampled.contains_key(id));
        for (id, raw) in sampled {
            let prev = self.peaks.get(&id).copied().unwrap_or(0.0);
            let next = if raw >= prev {
                raw
            } else {
                let decayed = prev * 0.68;
                if decayed < 0.008 { 0.0 } else { decayed }
            };
            if (next - prev).abs() > 0.004 {
                changed = true;
            }
            self.peaks.insert(id, next);
        }
        changed
    }
}

#[derive(Default)]
struct Rename {
    editing: bool,
    draft: String,
    input: Option<Entity<TextInput>>,
    focus_pending: bool,
    generation: u64,
}

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    // Advance bidirectional List <-> Properties slide transition.
    let view_anim = if let Some(vt) = app.sections.devices.view_transition.as_ref() {
        let raw = (vt.started_at.elapsed().as_secs_f32() / VIEW_SLIDE_DURATION.as_secs_f32())
            .clamp(0., 1.);
        if raw < 1. {
            window.request_animation_frame();
            Some((vt.direction, SLIDE_EASE.apply(raw), vt.outgoing_props.clone()))
        } else {
            app.sections.devices.view_transition = None;
            None
        }
    } else {
        None
    };

    // Resolve open device properties if present.
    let open_props = app
        .sections
        .devices
        .open_device
        .as_ref()
        .map(|o| {
            (
                o.id.clone(),
                o.input,
                o.props.clone(),
                o.listen_access_denied,
                o.spatial_error.clone(),
            )
        })
        .and_then(|(open_id, open_input, props, listen_denied, spatial_err)| {
            let entry = if open_input {
                app.snapshot
                    .all_input_devices
                    .iter()
                    .find(|d| d.id == open_id)
                    .cloned()
            } else {
                app.snapshot
                    .all_output_devices
                    .iter()
                    .find(|d| d.id == open_id)
                    .cloned()
            };
            if let Some(entry) = entry {
                let peak = app
                    .sections
                    .devices
                    .peaks
                    .get(&open_id)
                    .copied()
                    .unwrap_or(0.0);
                Some((entry, props, listen_denied, spatial_err, peak))
            } else {
                app.sections.devices.close_properties();
                None
            }
        });

    if let Some((entry, props, listen_denied, spatial_err, peak)) = open_props {
        let props_el = super::devices_properties::render(
            app,
            entry,
            props,
            listen_denied,
            spatial_err,
            peak,
            window,
            cx,
        );
        if let Some((ViewSlideDirection::Enter, eased, _)) = view_anim {
            let list_el = render_list_view(app, window, cx);
            return vec![
                div()
                    .relative()
                    .max_w(px(720.))
                    .w_full()
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left(px(-32. * eased))
                            .w_full()
                            .opacity(1. - eased)
                            .child(list_el),
                    )
                    .child(
                        div()
                            .relative()
                            .left(px(40. * (1. - eased)))
                            .opacity(eased)
                            .child(props_el),
                    )
                    .into_any_element(),
            ];
        }
        return vec![props_el];
    }

    let list_el = render_list_view(app, window, cx);
    if let Some((ViewSlideDirection::Exit, eased, Some((entry, props, listen_denied, spatial_err, peak)))) =
        view_anim
    {
        let outgoing_el = super::devices_properties::render(
            app,
            entry,
            props,
            listen_denied,
            spatial_err,
            peak,
            window,
            cx,
        );
        return vec![
            div()
                .relative()
                .max_w(px(720.))
                .w_full()
                .child(
                    div()
                        .relative()
                        .left(px(-32. * (1. - eased)))
                        .opacity(eased)
                        .child(list_el),
                )
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(40. * eased))
                        .w_full()
                        .opacity(1. - eased)
                        .child(outgoing_el),
                )
                .into_any_element(),
        ];
    }

    vec![list_el]
}

fn render_list_view(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> AnyElement {
    let mode = app
        .snapshot
        .config
        .advanced
        .audio_device_name_display
        .clone();
    let rename_allowed = mode != crate::AUDIO_DEVICE_NAME_SYSTEM;
    let split_comm = app.snapshot.config.advanced.separate_communication_device;
    let active_tab = app.sections.devices.active_tab;

    // Advance Output <-> Input slide transition.
    let tab_anim = if let Some(ts) = app.sections.devices.tab_slide.as_ref() {
        let raw = (ts.started_at.elapsed().as_secs_f32() / TAB_SLIDE_DURATION.as_secs_f32())
            .clamp(0., 1.);
        if raw < 1. {
            window.request_animation_frame();
            Some((ts.from, ts.to, SLIDE_EASE.apply(raw)))
        } else {
            app.sections.devices.tab_slide = None;
            None
        }
    } else {
        None
    };

    // Unplugged/hidden rows lose their inline rename editor.
    app.sections.devices.rows.retain(|id, _| {
        app.snapshot
            .all_output_devices
            .iter()
            .chain(&app.snapshot.all_input_devices)
            .any(|d| &d.id == id)
    });

    let tab_switch = controls::segmented_toggle(
        "devices-flow-tabs",
        active_tab.as_str(),
        vec![
            SegmentedToggleOption::new("output", "Output").icon("volume-loud-linear"),
            SegmentedToggleOption::new("input", "Input").icon("microphone-3-linear"),
        ],
        window,
        cx,
        |app, value, _, cx| {
            let next = DeviceFlowTab::from_str(&value);
            let prev = app.sections.devices.active_tab;
            if prev != next {
                app.sections.devices.active_tab = next;
                app.sections.devices.tab_slide = Some(TabSlide {
                    from: prev,
                    to: next,
                    started_at: Instant::now(),
                });
                app.sections.devices.context_menu = None;
                cx.notify();
            }
        },
    )
    .mb(px(14.));

    let devices_for = |tab: DeviceFlowTab, snap: &SettingsSnapshot| match tab {
        DeviceFlowTab::Output => (snap.all_output_devices.clone(), "No output devices found"),
        DeviceFlowTab::Input => (snap.all_input_devices.clone(), "No input devices found"),
    };

    let (current_devices, empty_text) = devices_for(active_tab, &app.snapshot);
    let incoming_list = device_list(
        empty_text,
        current_devices,
        &mode,
        rename_allowed,
        split_comm,
        app,
        window,
        cx,
    );

    let measured_h_state =
        window.use_keyed_state("devices-list-height-measure", cx, |_, _| 0f32);
    let measured_h = *measured_h_state.read(cx);
    let measured_h_cb = measured_h_state.clone();

    let incoming_measured = div()
        .w_full()
        .child(incoming_list.w_full().flex_shrink_0())
        .on_children_prepainted(move |bounds, _, cx| {
            if let Some(b) = bounds.first() {
                let h = b.size.height.as_f32();
                measured_h_cb.update(cx, |old, cx| {
                    if (*old - h).abs() > 0.5 {
                        *old = h;
                        cx.notify();
                    }
                });
            }
        });

    let inner_list: AnyElement = if let Some((from_tab, to_tab, eased)) = tab_anim
        && from_tab != to_tab
    {
        let (out_devices, out_empty) = devices_for(from_tab, &app.snapshot);
        let outgoing_list = device_list(
            out_empty,
            out_devices,
            &mode,
            rename_allowed,
            split_comm,
            app,
            window,
            cx,
        );
        // Output (left) -> Input (right): outgoing moves left, incoming enters from right.
        let dir = if to_tab == DeviceFlowTab::Input { 1. } else { -1. };
        div()
            .relative()
            .w_full()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(-28. * dir * eased))
                    .w_full()
                    .opacity(1. - eased)
                    .child(outgoing_list),
            )
            .child(
                div()
                    .relative()
                    .left(px(28. * dir * (1. - eased)))
                    .opacity(eased)
                    .child(incoming_measured),
            )
            .into_any_element()
    } else {
        incoming_measured.into_any_element()
    };

    let list_block: AnyElement = if measured_h > 0.5 {
        let animated_h = anim::tween(
            window,
            cx,
            "devices-list-height-anim",
            measured_h,
            Transition::new(220, SLIDE_EASE),
        );
        let animating = (animated_h - measured_h).abs() > 0.5 || tab_anim.is_some();
        div()
            .relative()
            .w_full()
            .flex_shrink_0()
            .h(px(animated_h))
            .when(animating, |d| d.overflow_hidden())
            .child(inner_list)
            .into_any_element()
    } else {
        div()
            .relative()
            .w_full()
            .flex_shrink_0()
            .child(inner_list)
            .into_any_element()
    };

    // Options card
    let mut name_options = SelectOptions::new(vec![
        SelectOption::new(crate::AUDIO_DEVICE_NAME_PRETTY, "Pretty Name")
            .icon("microphone-3-linear"),
        SelectOption::new(crate::AUDIO_DEVICE_NAME_SYSTEM, "System Name").icon("widget-linear"),
        SelectOption::new(crate::AUDIO_DEVICE_NAME_BOTH, "Both").icon("volume-loud-linear"),
    ])
    .row("Displayed device name");
    name_options.show_current_detail = false;
    let name_select = controls::select(
        "devices-name-display",
        mode.clone(),
        name_options,
        app,
        window,
        cx,
        |app, value, _, cx| {
            app.update_config(cx, |c| {
                c.advanced.audio_device_name_display = value.to_string()
            })
        },
    );

    let show_disabled_toggle = controls::toggle(
        "devices-show-disabled",
        app.snapshot.config.advanced.show_disabled_devices,
        window,
        cx,
        |app, value, _, cx| {
            app.update_config(cx, |c| c.advanced.show_disabled_devices = value)
        },
    );
    let show_disconnected_toggle = controls::toggle(
        "devices-show-disconnected",
        app.snapshot.config.advanced.show_disconnected_devices,
        window,
        cx,
        |app, value, _, cx| {
            app.update_config(cx, |c| c.advanced.show_disconnected_devices = value)
        },
    );
    let split_comm_toggle = controls::toggle(
        "devices-split-comm",
        split_comm,
        window,
        cx,
        |app, value, _, cx| {
            app.update_config(cx, |c| c.advanced.separate_communication_device = value)
        },
    );
    let ungroup_toggle = controls::toggle(
        "devices-ungroup",
        app.snapshot.config.advanced.ungroup_tray_devices,
        window,
        cx,
        |app, value, _, cx| app.update_config(cx, |c| c.advanced.ungroup_tray_devices = value),
    );
    let handsfree_toggle = controls::toggle(
        "devices-bt-handsfree",
        app.snapshot.config.advanced.block_bluetooth_handsfree,
        window,
        cx,
        |app, value, _, cx| {
            app.update_config(cx, |c| c.advanced.block_bluetooth_handsfree = value)
        },
    );

    let tray_target = app.snapshot.config.advanced.tray_devices_target.clone();
    let mut tray_target_options = SelectOptions::new(vec![
        SelectOption::new(crate::TRAY_DEVICES_TARGET_APP, "silence! Devices")
            .icon("headphones-round-sound-linear"),
        SelectOption::new(crate::TRAY_DEVICES_TARGET_WINDOWS, "Windows Sound")
            .icon("microsoft-windows"),
    ])
    .row("Tray sound devices opens");
    tray_target_options.show_current_detail = false;
    let tray_target_select = controls::select(
        "devices-tray-target",
        tray_target,
        tray_target_options,
        app,
        window,
        cx,
        |app, value, _, cx| {
            app.update_config(cx, |c| {
                c.advanced.tray_devices_target = value.to_string()
            })
        },
    );

    let p = app.palette;
    let win_btn_id = ElementId::from("devices-open-windows-sound");
    let (win_ptr, win_hover, win_press) = controls::hover_tween(win_btn_id.clone(), window, cx);
    let win_input = active_tab == DeviceFlowTab::Input;
    let win_sound_btn = controls::track_pointer(
        div()
            .id(win_btn_id)
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
            .child("Windows sound")
            .child(
                svg()
                    .path(icon("square-arrow-right-up-linear"))
                    .size(px(14.))
                    .text_color(lerp(p.muted, p.text, win_hover).hsla()),
            ),
        win_ptr,
    )
    .on_click(cx.listener(move |_, _, _, _| {
        let _ = crate::open_audio_device_control_panel(win_input);
    }))
    .into_any_element();

    let context_menu = render_context_menu(app, &mode, rename_allowed, split_comm, window, cx);

    div()
        .flex()
        .flex_col()
        .max_w(px(720.))
        .w_full()
        .child(super::form::page_title("Devices", Some(win_sound_btn)))
        .child(tab_switch)
        .child(list_block)
        .child(
            super::form::block()
                .mt(px(super::form::SECTION_GAP))
                .child(super::form::section_heading("Options", cx))
                .child(
                    super::form::card(cx)
                        .row(name_select)
                        .row(super::form::setting_row(
                            "Show disabled devices",
                            Some("Include manually disabled devices so you can re-enable them here"),
                            show_disabled_toggle,
                            cx,
                        ))
                        .row(super::form::setting_row(
                            "Show disconnected devices",
                            Some("Include unplugged ports and offline Bluetooth devices"),
                            show_disconnected_toggle,
                            cx,
                        ))
                        .row(super::form::setting_row(
                            "Separate communication device",
                            Some("Set default audio and default communications devices independently"),
                            split_comm_toggle,
                            cx,
                        ))
                        .row(super::form::setting_row(
                            "Ungroup devices in tray menu",
                            None,
                            ungroup_toggle,
                            cx,
                        )),
                ),
        )
        .child(
            super::form::block()
                .mt(px(super::form::SECTION_GAP))
                .child(super::form::section_heading("Advanced", cx))
                .child(
                    super::form::card(cx)
                        .row(tray_target_select)
                        .row(super::form::setting_row(
                            "Keep Bluetooth headphones in high quality",
                            Some("Turns off hands-free mode, so calls can't drop sound to mono. The headset's own mic stops working"),
                            handsfree_toggle,
                            cx,
                        )),
                ),
        )
        .children(context_menu)
        .into_any_element()
}

fn device_list(
    empty: &'static str,
    rows: Vec<DeviceEntry>,
    mode: &str,
    rename_allowed: bool,
    split_comm: bool,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let p = app.palette;
    let body = if rows.is_empty() {
        div()
            .min_h(px(super::form::ROW_HEIGHT))
            .flex()
            .items_center()
            .px(px(super::form::ROW_PAD))
            .rounded(px(14.))
            .border_1()
            .border_dashed()
            .border_color(p.border_strong.hsla())
            .bg(mix_pct(p.surface_2, 76., TRANSPARENT).hsla())
            .text_color(p.muted.hsla())
            .child(empty)
    } else {
        let total = rows.len();
        let mut card = super::form::card(cx);
        for (index, entry) in rows.into_iter().enumerate() {
            card = card.row(device_row(
                entry,
                index == 0,
                index + 1 == total,
                mode,
                rename_allowed,
                split_comm,
                app,
                window,
                cx,
            ));
        }
        card.into_element()
    };
    super::form::block().child(body)
}

fn finish_rename(
    app: &mut SettingsApp,
    device_id: &str,
    original_pretty_name: &str,
    commit: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) {
    let Some(state) = app.sections.devices.rows.get_mut(device_id) else {
        return;
    };
    if !state.editing {
        return;
    }
    let next = state.draft.trim().to_string();
    state.editing = false;
    state.focus_pending = false;
    state.draft = original_pretty_name.to_string();
    state.input = None;
    if commit && !next.is_empty() && next != original_pretty_name {
        let _ = crate::rename_audio_device(device_id, &next);
        app.refresh_devices(cx);
    }
    app.focus_handle.focus(window, cx);
    cx.notify();
}

fn start_rename(app: &mut SettingsApp, device_id: &str, pretty_name: &str, cx: &mut Context<SettingsApp>) {
    let state = app
        .sections
        .devices
        .rows
        .entry(device_id.to_string())
        .or_default();
    state.draft = pretty_name.to_string();
    state.editing = true;
    state.focus_pending = true;
    state.generation += 1;
    cx.notify();
}

#[allow(clippy::too_many_arguments)]
fn device_row(
    entry: DeviceEntry,
    is_first: bool,
    is_last: bool,
    mode: &str,
    rename_allowed: bool,
    split_comm: bool,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Stateful<Div> {
    let p = app.palette;
    let key = ElementId::from(SharedString::from(format!("device-row-{}", entry.id)));
    let display_name = entry.display_name(mode);
    let subtitle = entry.subtitle(mode);
    let is_active = entry.state.is_active();
    let is_disabled = entry.state.is_disabled();
    let peak = app
        .sections
        .devices
        .peaks
        .get(&entry.id)
        .copied()
        .unwrap_or(0.0);

    let state = app
        .sections
        .devices
        .rows
        .entry(entry.id.clone())
        .or_default();
    let editing = state.editing;
    let draft = state.draft.clone();
    let generation = state.generation;
    let focus_pending = std::mem::take(&mut state.focus_pending);

    let (row_ptr, row_hover, _) = controls::hover_tween((key.clone(), "row-hover"), window, cx);
    let active_t = anim::tween(
        window,
        cx,
        (key.clone(), "active"),
        entry.is_default as u8 as f32,
        UI,
    );
    let edit_t = anim::tween(
        window,
        cx,
        (key.clone(), "editing"),
        editing as u8 as f32,
        UI,
    );

    let name_block = if editing {
        let id_ev = entry.id.clone();
        let orig_ev = entry.name.clone();
        let input = controls::text_input(
            (
                key.clone(),
                SharedString::from(format!("rename-input-{generation}")),
            ),
            draft.clone(),
            TextInputOptions {
                caret_color: Some(p.accent.hsla()),
                ..Default::default()
            },
            window,
            cx,
            move |app, event, window, cx| {
                if !app
                    .sections
                    .devices
                    .rows
                    .get(&id_ev)
                    .is_some_and(|s| s.editing && s.generation == generation)
                {
                    return;
                }
                match event {
                    TextInputEvent::Change(value) => {
                        if let Some(s) = app.sections.devices.rows.get_mut(&id_ev) {
                            s.draft = value.to_string();
                        }
                        cx.notify();
                    }
                    TextInputEvent::Blur(value) if value.trim().is_empty() => {
                        if let Some(s) = app.sections.devices.rows.get_mut(&id_ev) {
                            s.draft = orig_ev.clone();
                        }
                        let id = id_ev.clone();
                        let old = orig_ev.clone();
                        cx.defer_in(window, move |app, _, cx| {
                            if let Some(s) = app.sections.devices.rows.get(&id)
                                && s.editing
                                && s.generation == generation
                                && let Some(input) = &s.input
                            {
                                input.update(cx, |input, cx| input.set_value(old, cx));
                            }
                        });
                        cx.notify();
                    }
                    TextInputEvent::Enter(_) => {
                        finish_rename(app, &id_ev, &orig_ev, true, window, cx)
                    }
                    TextInputEvent::Escape => {
                        finish_rename(app, &id_ev, &orig_ev, false, window, cx)
                    }
                    _ => {}
                }
            },
        );
        app.sections.devices.rows.get_mut(&entry.id).unwrap().input = Some(input.clone());
        if focus_pending {
            let id = entry.id.clone();
            let focus_input = input.clone();
            cx.defer_in(window, move |app, window, cx| {
                if app
                    .sections
                    .devices
                    .rows
                    .get(&id)
                    .is_some_and(|s| s.editing && s.generation == generation)
                {
                    focus_input.update(cx, |input, cx| input.focus_at_end(window, cx));
                }
            });
        }
        let commit_id = entry.id.clone();
        let commit_orig = entry.name.clone();
        let cancel_id = entry.id.clone();
        let cancel_orig = entry.name.clone();
        let commit = action(
            (
                key.clone(),
                SharedString::from(format!("commit-{generation}")),
            ),
            "check-mark",
            "Confirm rename",
            ActionKind::Commit,
            false,
            window,
            cx,
            move |app, window, cx| finish_rename(app, &commit_id, &commit_orig, true, window, cx),
        );
        let cancel = action(
            (
                key.clone(),
                SharedString::from(format!("cancel-{generation}")),
            ),
            "codicon_close",
            "Cancel rename",
            ActionKind::Cancel,
            false,
            window,
            cx,
            move |app, window, cx| finish_rename(app, &cancel_id, &cancel_orig, false, window, cx),
        );
        div()
            .relative()
            .flex()
            .items_center()
            .min_w_0()
            .min_h(px(18.))
            .pr(px(52.))
            .text_size(px(14.))
            .font_weight(FontWeight(620.))
            .line_height(px(18.))
            .child(
                controls::text_input_container((key.clone(), "rename-shell"), &input).flex_1(),
            )
            .child(
                div()
                    .absolute()
                    .top(relative(0.5))
                    .mt(px(-9.))
                    .right(px(24.))
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(commit),
            )
            .child(
                div()
                    .absolute()
                    .top(relative(0.5))
                    .mt(px(-9.))
                    .right_0()
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(cancel),
            )
    } else {
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(px(2.))
            .child(
                div()
                    .min_w_0()
                    .text_size(px(14.))
                    .font_weight(FontWeight(620.))
                    .line_height(px(18.))
                    .text_color(if is_active {
                        p.text.hsla()
                    } else {
                        p.muted.hsla()
                    })
                    .text_ellipsis()
                    .child(display_name),
            )
            .when_some(subtitle, |this, sub| {
                this.child(
                    div()
                        .min_w_0()
                        .text_size(px(12.))
                        .line_height(px(15.))
                        .text_color(p.muted.opacity(0.85).hsla())
                        .text_ellipsis()
                        .child(sub),
                )
            })
    };

    // Right-side controls depend on device state.
    let trailing = if is_active {
        let meter = compact_peak_meter(peak, p);

        let default_id = ElementId::from((key.clone(), "default"));
        let (default_ptr, default_hover, _) = controls::hover_tween(default_id.clone(), window, cx);
        let default_dev_id = entry.id.clone();
        let default_pill = div()
            .flex()
            .justify_end()
            .flex_shrink_0()
            .child(if entry.is_default {
                div()
                    .h(px(26.))
                    .pl(px(9.))
                    .pr(px(11.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded_full()
                    .opacity(active_t)
                    .bg(p.accent_soft.hsla())
                    .text_size(px(12.5))
                    .font_weight(FontWeight(600.))
                    .text_color(p.accent_text.hsla())
                    .child(
                        svg()
                            .path(icon("check-mark"))
                            .size(px(10.))
                            .text_color(p.accent_text.hsla()),
                    )
                    .child("Default")
                    .into_any_element()
            } else {
                controls::track_pointer(
                    div()
                        .id(default_id)
                        .h(px(26.))
                        .px(px(11.))
                        .flex()
                        .items_center()
                        .rounded_full()
                        .opacity(1. - active_t)
                        .cursor_pointer()
                        .bg(lerp(
                            mix_pct(p.text, 6.5, TRANSPARENT),
                            p.surface_3,
                            default_hover,
                        )
                        .hsla())
                        .text_size(px(12.5))
                        .font_weight(FontWeight(600.))
                        .text_color(lerp(p.muted, p.text, default_hover).hsla())
                        .child("Set as default"),
                    default_ptr,
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    let _ = audio_devices::set_default_device(&default_dev_id, split_comm);
                    app.refresh_devices(cx);
                }))
                .into_any_element()
            });

        // Separate Default Communication role button (animates width when setting toggles)
        let comm_slot = anim::tween(
            window,
            cx,
            (key.clone(), "comm-slot"),
            split_comm as u8 as f32,
            UI,
        );
        let comm_dev_id = entry.id.clone();
        let is_comm = entry.is_default_comm;
        let comm_btn = (comm_slot > 0.01).then(|| {
            let btn = action(
                (key.clone(), "comm"),
                if is_comm { "phone-bold" } else { "phone-linear" },
                if is_comm {
                    "Default communication device"
                } else {
                    "Set as default communication device"
                },
                ActionKind::Comm(is_comm),
                false,
                window,
                cx,
                move |app, _, cx| {
                    let _ = audio_devices::set_default_comm_device(&comm_dev_id);
                    app.refresh_devices(cx);
                },
            );
            div()
                .w(px(40. * comm_slot))
                .pl(px(4. * comm_slot))
                .overflow_hidden()
                .opacity(comm_slot)
                .child(btn)
        });

        let favorite = (!entry.input).then(|| {
            let is_favorite = app
                .snapshot
                .config
                .volume
                .favorite_output_devices
                .iter()
                .any(|id| id == &entry.id);
            let fav_id = entry.id.clone();
            action(
                (key.clone(), "favorite"),
                if is_favorite {
                    "star-bold"
                } else {
                    "star-linear"
                },
                if is_favorite {
                    "Favorite: next/previous output hotkeys cycle through favorites"
                } else {
                    "Add to favorites for next/previous output hotkeys"
                },
                ActionKind::Favorite(is_favorite),
                false,
                window,
                cx,
                move |app, _, cx| {
                    let id = fav_id.clone();
                    app.update_config(cx, |config| {
                        let list = &mut config.volume.favorite_output_devices;
                        if list.iter().any(|existing| existing == &id) {
                            list.retain(|existing| existing != &id);
                        } else {
                            list.push(id);
                        }
                    });
                },
            )
        });

        let rename_slot = anim::tween(
            window,
            cx,
            (key.clone(), "rename-slot"),
            (rename_allowed && !editing) as u8 as f32,
            UI,
        );
        let rename_id = entry.id.clone();
        let rename_orig = entry.name.clone();
        let rename = (rename_allowed && !editing).then(|| {
            action(
                (key.clone(), "rename"),
                "pen-linear",
                "Rename",
                ActionKind::Normal,
                false,
                window,
                cx,
                move |app, _, cx| start_rename(app, &rename_id, &rename_orig, cx),
            )
        });

        let chevron = div()
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .child(
                svg()
                    .path(icon("alt-arrow-right-linear"))
                    .size(px(15.))
                    .text_color(lerp(p.muted.opacity(0.65), p.text, row_hover).hsla()),
            );

        div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(4.))
            .child(div().pr(px(8.)).child(meter))
            .child(default_pill)
            .children(comm_btn)
            .children(favorite)
            .when(rename_slot > 0., |this| {
                this.child(
                    div()
                        .w(px(38. * rename_slot))
                        .overflow_hidden()
                        .opacity(rename_slot)
                        .children(rename),
                )
            })
            .child(chevron)
    } else if is_disabled {
        let enable_id = ElementId::from((key.clone(), "enable"));
        let (en_ptr, en_hover, _) = controls::hover_tween(enable_id.clone(), window, cx);
        let enable_dev_id = entry.id.clone();
        let enable_btn = controls::track_pointer(
            div()
                .id(enable_id)
                .h(px(28.))
                .px(px(12.))
                .rounded_full()
                .flex()
                .items_center()
                .cursor_pointer()
                .bg(lerp(p.surface_3, p.accent_soft, en_hover).hsla())
                .text_size(px(12.5))
                .font_weight(FontWeight(620.))
                .text_color(lerp(p.text, p.accent_text, en_hover).hsla())
                .child("Enable"),
            en_ptr,
        )
        .on_click(cx.listener(move |app, _, _, cx| {
            cx.stop_propagation();
            let _ = audio_devices::set_device_enabled(&enable_dev_id, true);
            app.refresh_devices(cx);
        }));

        let chevron = div()
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .child(
                svg()
                    .path(icon("alt-arrow-right-linear"))
                    .size(px(15.))
                    .text_color(lerp(p.muted.opacity(0.5), p.text, row_hover).hsla()),
            );

        div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(4.))
            .child(enable_btn)
            .child(chevron)
    } else {
        // Disconnected
        div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .pr(px(8.))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight(600.))
                    .text_color(p.muted.opacity(0.75).hsla())
                    .child("Disconnected"),
            )
    };

    let open_id = entry.id.clone();
    let open_input = entry.input;
    let can_open_props = !editing && (is_active || is_disabled);
    let ctx_id = entry.id.clone();
    let ctx_input = entry.input;

    let row_bg = if editing {
        TRANSPARENT
    } else {
        lerp(TRANSPARENT, mix_pct(p.text, 4.0, TRANSPARENT), row_hover)
    };

    let icon_color = if entry.is_default {
        p.accent_text
    } else if is_active {
        p.muted
    } else {
        p.muted.opacity(0.55)
    };

    controls::track_pointer(
        div()
            .id(key)
            .flex()
            .items_center()
            .gap(px(12.))
            .min_h(px(super::form::ROW_HEIGHT))
            .pl(px(super::form::ROW_PAD))
            .pr(px(10.))
            .py(px(10.))
            .flex_shrink_0()
            .bg(row_bg.hsla())
            .when(can_open_props, |this| this.cursor_pointer())
            .when(is_first, |this| this.rounded_tl(px(14.)).rounded_tr(px(14.)))
            .when(is_last, |this| this.rounded_bl(px(14.)).rounded_br(px(14.)))
            .when(edit_t > 0., |this| {
                this.shadow(vec![
                    BoxShadow {
                        color: p.focus.opacity(edit_t).hsla(),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(0.),
                        spread_radius: px(1.),
                        inset: true,
                    },
                    BoxShadow {
                        color: mix_pct(p.focus, 18. * edit_t, TRANSPARENT).hsla(),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(0.),
                        spread_radius: px(2.),
                        inset: false,
                    },
                ])
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .flex_1()
                    .gap(px(12.))
                    .child(
                        svg()
                            .path(icon(entry.icon_name()))
                            .size(px(20.))
                            .flex_shrink_0()
                            .text_color(icon_color.hsla()),
                    )
                    .child(div().flex().flex_col().min_w_0().flex_1().child(name_block)),
            )
            .child(trailing),
        row_ptr,
    )
    .on_click(cx.listener(move |app, _, _, cx| {
        if can_open_props {
            app.sections
                .devices
                .open_device_properties(open_id.clone(), open_input);
            cx.notify();
        }
    }))
    .on_mouse_down(
        MouseButton::Right,
        cx.listener(move |app, event: &MouseDownEvent, _, cx| {
            app.sections.devices.context_menu = Some(ContextMenu {
                device_id: ctx_id.clone(),
                input: ctx_input,
                position: event.position,
            });
            cx.notify();
        }),
    )
}

/// Sleek 8-bar live peak level indicator for active device rows.
fn compact_peak_meter(peak: f32, p: Palette) -> Div {
    const BARS: usize = 8;
    let clamped = peak.clamp(0.0, 1.0);
    let mut row = div()
        .flex()
        .items_end()
        .gap(px(2.))
        .h(px(16.))
        .flex_shrink_0();
    for i in 0..BARS {
        let threshold = (i as f32 + 0.5) / BARS as f32;
        let lit = clamped >= threshold;
        let height = 5.0 + (i as f32 / (BARS - 1) as f32) * 11.0;
        let bar_color = if !lit {
            p.surface_3
        } else if i >= 7 {
            p.danger
        } else if i >= 5 {
            p.accent
        } else {
            p.success
        };
        row = row.child(
            div()
                .w(px(3.))
                .h(px(height))
                .rounded_full()
                .bg(bar_color.hsla()),
        );
    }
    row
}

fn render_context_menu(
    app: &mut SettingsApp,
    _mode: &str,
    rename_allowed: bool,
    split_comm: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Option<AnyElement> {
    let menu_state = app.sections.devices.context_menu.as_ref()?;
    let device_id = menu_state.device_id.clone();
    let input = menu_state.input;
    let pos = menu_state.position;

    let entry = if input {
        app.snapshot
            .all_input_devices
            .iter()
            .find(|d| d.id == device_id)
            .cloned()
    } else {
        app.snapshot
            .all_output_devices
            .iter()
            .find(|d| d.id == device_id)
            .cloned()
    }?;

    let p = app.palette;
    let mut items: Vec<AnyElement> = Vec::new();

    if entry.state.is_active() && !entry.is_default {
        let id = entry.id.clone();
        items.push(ctx_item(
            "ctx-default",
            "check-mark",
            "Set as default",
            false,
            window,
            cx,
            move |app, _, cx| {
                let _ = audio_devices::set_default_device(&id, split_comm);
                app.sections.devices.context_menu = None;
                app.refresh_devices(cx);
            },
        ));
    }

    if entry.state.is_active() && split_comm && !entry.is_default_comm {
        let id = entry.id.clone();
        items.push(ctx_item(
            "ctx-comm",
            "phone-linear",
            "Set as default communication",
            false,
            window,
            cx,
            move |app, _, cx| {
                let _ = audio_devices::set_default_comm_device(&id);
                app.sections.devices.context_menu = None;
                app.refresh_devices(cx);
            },
        ));
    }

    if entry.state.is_active() && !entry.input {
        let id = entry.id.clone();
        items.push(ctx_item(
            "ctx-test",
            "play-circle-linear",
            "Test",
            false,
            window,
            cx,
            move |app, _, cx| {
                let _ = audio_devices::play_test_tone(&id);
                app.sections.devices.context_menu = None;
                cx.notify();
            },
        ));
    }

    if rename_allowed {
        let id = entry.id.clone();
        let orig = entry.name.clone();
        items.push(ctx_item(
            "ctx-rename",
            "pen-linear",
            "Rename",
            false,
            window,
            cx,
            move |app, _, cx| {
                app.sections.devices.context_menu = None;
                start_rename(app, &id, &orig, cx);
            },
        ));
    }

    if entry.state.is_active() {
        let id = entry.id.clone();
        items.push(ctx_item(
            "ctx-disable",
            "forbidden-circle-linear",
            "Disable",
            true,
            window,
            cx,
            move |app, _, cx| {
                let _ = audio_devices::set_device_enabled(&id, false);
                app.sections.devices.context_menu = None;
                app.refresh_devices(cx);
            },
        ));
    } else if entry.state.is_disabled() {
        let id = entry.id.clone();
        items.push(ctx_item(
            "ctx-enable",
            "check-circle-bold",
            "Enable",
            false,
            window,
            cx,
            move |app, _, cx| {
                let _ = audio_devices::set_device_enabled(&id, true);
                app.sections.devices.context_menu = None;
                app.refresh_devices(cx);
            },
        ));
    }

    if entry.state.is_active() || entry.state.is_disabled() {
        let id = entry.id.clone();
        let inp = entry.input;
        items.push(ctx_item(
            "ctx-props",
            "tuning-2-linear",
            "Properties",
            false,
            window,
            cx,
            move |app, _, cx| {
                app.sections.devices.open_device_properties(id.clone(), inp);
                cx.notify();
            },
        ));
    }

    {
        let id = entry.id.clone();
        let inp = entry.input;
        items.push(ctx_item(
            "ctx-win-props",
            "square-arrow-right-up-linear",
            "Open Windows properties",
            false,
            window,
            cx,
            move |app, _, cx| {
                let _ = crate::open_audio_device_properties(&id, inp);
                app.sections.devices.context_menu = None;
                cx.notify();
            },
        ));
    }

    let menu_w = 224.0_f32;
    let menu_h = (items.len() as f32) * 34.0 + 12.0;
    let vp = window.viewport_size();
    let left = pos.x.as_f32().min((vp.width.as_f32() - menu_w - 12.0).max(8.0));
    let top = pos.y.as_f32().min((vp.height.as_f32() - menu_h - 12.0).max(8.0));

    let menu = div()
        .id("devices-context-menu")
        .w(px(menu_w))
        .p(px(6.))
        .rounded(px(12.))
        .bg(p.select_menu_bg.hsla())
        .border_1()
        .border_color(p.border_strong.hsla())
        .shadow(vec![BoxShadow {
            color: p.shadow_medium.hsla(),
            offset: point(px(0.), px(8.)),
            blur_radius: px(24.),
            spread_radius: px(0.),
            inset: false,
        }])
        .flex()
        .flex_col()
        .gap(px(2.))
        .on_mouse_down_out(cx.listener(|app, _, _, cx| {
            if app.sections.devices.context_menu.is_some() {
                app.sections.devices.context_menu = None;
                cx.notify();
            }
        }))
        .children(items);

    Some(
        deferred(anchored().position(point(px(left), px(top))).child(menu))
            .with_priority(100)
            .into_any_element(),
    )
}

fn ctx_item(
    id: &'static str,
    icon_name: &'static str,
    label: &'static str,
    danger: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_click: impl Fn(&mut SettingsApp, &mut Window, &mut Context<SettingsApp>) + 'static,
) -> AnyElement {
    let p = palette(cx);
    let el_id = ElementId::from(id);
    let (ptr, hover, _) = controls::hover_tween(el_id.clone(), window, cx);
    let fg = if danger {
        lerp(p.danger, p.danger_hover, hover)
    } else {
        p.text
    };
    let bg = if danger {
        lerp(TRANSPARENT, mix_pct(p.danger, 14., TRANSPARENT), hover)
    } else {
        lerp(TRANSPARENT, p.surface_3, hover)
    };
    controls::track_pointer(
        div()
            .id(el_id)
            .h(px(32.))
            .px(px(10.))
            .rounded(px(8.))
            .flex()
            .items_center()
            .gap(px(9.))
            .cursor_pointer()
            .bg(bg.hsla())
            .text_size(px(13.))
            .font_weight(FontWeight(560.))
            .text_color(fg.hsla())
            .child(
                svg()
                    .path(icon(icon_name))
                    .size(px(15.))
                    .text_color(if danger { fg } else { lerp(p.muted, p.text, hover) }.hsla()),
            )
            .child(label),
        ptr,
    )
    .on_click(cx.listener(move |app, _, window, cx| {
        cx.stop_propagation();
        on_click(app, window, cx);
    }))
    .into_any_element()
}

#[derive(Clone, Copy)]
enum ActionKind {
    Normal,
    /// Star toggle; `true` while the device is a favorite.
    Favorite(bool),
    /// Communication default toggle; `true` while this device is the default comm device.
    Comm(bool),
    Commit,
    Cancel,
}

struct ActionTooltip(&'static str);
impl Render for ActionTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        div()
            .px(px(8.))
            .py(px(5.))
            .rounded(px(6.))
            .bg(p.select_menu_bg.hsla())
            .text_color(p.text.hsla())
            .text_size(px(12.))
            .child(self.0)
    }
}

fn action(
    id: impl Into<ElementId>,
    icon_name: &'static str,
    title: &'static str,
    kind: ActionKind,
    disabled: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    callback: impl Fn(&mut SettingsApp, &mut Window, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    let id = id.into();
    let p = palette(cx);
    let callback = Rc::new(callback);
    let key_callback = callback.clone();
    let (pointer, hover, press) = controls::hover_tween(id.clone(), window, cx);
    let inline = matches!(kind, ActionKind::Commit | ActionKind::Cancel);
    let entrance = if inline {
        anim::tween_from(
            window,
            cx,
            (id.clone(), "entrance"),
            0.,
            1.,
            Transition::new(160, Easing::Bezier(0.22, 1., 0.36, 1.)),
        )
    } else {
        1.
    };
    let scale = 0.82 + 0.18 * entrance;
    let fg = match kind {
        ActionKind::Favorite(on) | ActionKind::Comm(on) => {
            if on {
                p.accent_text
            } else {
                lerp(p.muted, p.text, hover)
            }
        }
        ActionKind::Commit => p.success,
        ActionKind::Cancel => lerp(p.danger, p.danger_hover, hover),
        _ => lerp(p.muted, p.text, if disabled { 0. } else { hover }),
    };
    let bg = match kind {
        ActionKind::Comm(true) => lerp(p.accent_soft, mix_pct(p.accent, 22., TRANSPARENT), hover),
        _ => lerp(
            lerp(TRANSPARENT, p.surface_3, hover),
            p.surface_2,
            if disabled || inline { 0. } else { press },
        ),
    };
    let focus = window
        .use_keyed_state((id.clone(), "focus"), cx, |_, cx| cx.focus_handle())
        .read(cx)
        .clone();
    controls::track_pointer(
        div()
            .id(id)
            .size(px(if inline { 18. * scale } else { 34. }))
            .flex_shrink_0()
            .opacity(entrance)
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(if inline { 6. * scale } else { 8. }))
            .bg(bg.hsla())
            .when(!disabled, |this| {
                this.cursor_pointer()
                    .track_focus(&focus)
                    .focus_visible(move |style| controls::focus_outline(style, p.focus.hsla()))
                    .on_key_down(cx.listener(move |app, event: &KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            key_callback(app, window, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        callback(app, window, cx);
                    }))
            })
            .tooltip(move |_, cx| cx.new(|_| ActionTooltip(title)).into())
            .child(
                svg()
                    .path(icon(icon_name))
                    .size(px(if inline { 14. * scale } else { 17. }))
                    .text_color(fg.hsla()),
            ),
        pointer,
    )
}

/// Which microphone the tray icon and overlay reflect (`advanced.indicator_device`).
pub(crate) fn indicator_device_options(
    devices: &[crate::MicDevice],
    hotkeys: &[crate::HotkeyBinding],
    selected: &str,
    name_display: &str,
) -> Vec<SelectOption> {
    let device_name = |id: Option<&str>| {
        devices
            .iter()
            .find(|device| match id {
                Some(id) => device.id == id,
                None => device.is_default,
            })
            .map(|device| device.display_name(name_display))
    };
    let auto_id = crate::indicator_device_id(crate::INDICATOR_DEVICE_AUTO, hotkeys);
    let auto_detail = match device_name(auto_id.as_deref()) {
        Some(name) if auto_id.is_some() => format!("Follows your mute hotkeys: {name}"),
        _ => "Follows your mute hotkeys: Windows default".to_string(),
    };
    let default_detail = device_name(None).unwrap_or_else(|| "No active microphone".to_string());
    let mic = "microphone-3-linear";
    let mut options = vec![
        SelectOption::new(crate::INDICATOR_DEVICE_AUTO, "Auto")
            .detail(auto_detail)
            .icon(mic),
        SelectOption::new(crate::INDICATOR_DEVICE_DEFAULT, "Windows default")
            .detail(default_detail)
            .icon(mic),
    ];
    let selected_is_device = selected != crate::INDICATOR_DEVICE_AUTO
        && selected != crate::INDICATOR_DEVICE_DEFAULT
        && !selected.is_empty();
    if selected_is_device && !devices.iter().any(|device| device.id == selected) {
        options.push(
            SelectOption::new(selected.to_string(), "Disconnected microphone")
                .detail("Showing Windows default until it reconnects")
                .icon(mic),
        );
    }
    options.extend(devices.iter().map(|device| {
        SelectOption::new(device.id.clone(), device.display_name(name_display)).icon(mic)
    }));
    options
}
