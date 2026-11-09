//! Output-volume foundation shared by the tray icon, OSD, wheel and media keys.
//!
//! COM callbacks only post to the hidden window. State changes land on the UI
//! thread through [`on_output_volume_changed`] and [`on_output_device_changed`].

/// Event-context GUID passed to every volume or mute write this process makes.
/// Notifications that carry it are [`VolumeChangeSource::Own`].
pub const OUTPUT_VOLUME_EVENT_CONTEXT: GUID =
    GUID::from_u128(0x4f8c_2a91_6d3e_4b7a_8c15_2e9f_0b6d_4a73);

const RENDER_GENERATION_MASK: u64 = 0x3FFF_FFFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeChangeSource {
    /// silence! wrote the change (`OUTPUT_VOLUME_EVENT_CONTEXT`).
    Own,
    /// Windows or another process wrote the change.
    External,
}

/// Default console render endpoint. `level` is the master scalar, `0.0..=1.0`.
/// `device_name` is `PKEY_Device_FriendlyName`.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputVolumeState {
    pub device_id: String,
    pub device_name: String,
    pub level: f32,
    pub muted: bool,
}

impl Default for OutputVolumeState {
    fn default() -> Self {
        Self {
            device_id: String::new(),
            device_name: String::new(),
            level: 0.0,
            muted: false,
        }
    }
}

enum RenderVolumeBind {
    /// Same device, callback already current. Not a device change.
    Unchanged,
    Bound(OutputVolumeState),
    /// No active console render endpoint.
    Missing,
    /// Registration is absent or the read failed for a reason other than a missing device.
    Unavailable,
}

#[windows_core::implement(IAudioEndpointVolumeCallback)]
struct OutputEndpointVolumeSink {
    hwnd: HWND,
    generation: u64,
}

impl windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolumeCallback_Impl
    for OutputEndpointVolumeSink_Impl
{
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        if pnotify.is_null() || self.hwnd.0.is_null() {
            return Ok(());
        }
        let notify = unsafe { &*pnotify };
        let own = notify.guidEventContext == OUTPUT_VOLUME_EVENT_CONTEXT;
        let mut flags = (self.generation & RENDER_GENERATION_MASK) << 2;
        if notify.bMuted.as_bool() {
            flags |= 1;
        }
        if own {
            flags |= 2;
        }
        unsafe {
            let _ = PostMessageW(
                self.hwnd,
                WM_OUTPUT_VOLUME_CHANGED,
                WPARAM(flags as usize),
                LPARAM(notify.fMasterVolume.to_bits() as i32 as isize),
            );
        }
        Ok(())
    }
}

impl AudioNotificationRegistration {
    fn rebind_render_volume(&mut self, force: bool) -> Result<RenderVolumeBind> {
        let device = match unsafe { self.enumerator.GetDefaultAudioEndpoint(eRender, eConsole) } {
            Ok(device) => device,
            Err(err) if err.code() == win32_not_found() => {
                self.clear_render_binding();
                return Ok(RenderVolumeBind::Missing);
            }
            Err(err) => return Err(err).context("get default render endpoint"),
        };
        let device_id = unsafe { endpoint_device_id(&device)? };
        if !force
            && self.render_volume.is_some()
            && self.render_device_id.as_deref() == Some(device_id.as_str())
        {
            return Ok(RenderVolumeBind::Unchanged);
        }

        let device_name = endpoint_friendly_name(&device);
        let volume: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_ALL, None) }
            .context("activate render endpoint volume")?;
        let generation = self.render_generation.wrapping_add(1).max(1);
        let callback: IAudioEndpointVolumeCallback =
            windows_core::ComObject::new(OutputEndpointVolumeSink {
                hwnd: self.hwnd,
                generation,
            })
            .into_interface();
        unsafe {
            volume
                .RegisterControlChangeNotify(&callback)
                .context("register render volume callback")?;
        }
        // Drop the previous callback only after the new one is registered, so a
        // change that lands during the swap still has a listener.
        self.unregister_render_volume_callback();

        let level = match unsafe { volume.GetMasterVolumeLevelScalar() } {
            Ok(level) => level,
            Err(err) => {
                unsafe {
                    let _ = volume.UnregisterControlChangeNotify(&callback);
                }
                return Err(err).context("read render volume");
            }
        };
        let muted = match unsafe { volume.GetMute() } {
            Ok(muted) => muted.as_bool(),
            Err(err) => {
                unsafe {
                    let _ = volume.UnregisterControlChangeNotify(&callback);
                }
                return Err(err).context("read render mute");
            }
        };

        self.render_volume = Some(volume);
        self.render_volume_callback = Some(callback);
        self.render_device_id = Some(device_id.clone());
        self.render_device_name = Some(device_name.clone());
        self.render_generation = generation;
        Ok(RenderVolumeBind::Bound(OutputVolumeState {
            device_id,
            device_name,
            level: level.clamp(0.0, 1.0),
            muted,
        }))
    }

    fn clear_render_binding(&mut self) {
        self.unregister_render_volume_callback();
        self.render_device_id = None;
        self.render_device_name = None;
        self.render_generation = self.render_generation.wrapping_add(1);
    }

    fn unregister_render_volume_callback(&mut self) {
        if let (Some(volume), Some(callback)) = (
            self.render_volume.take(),
            self.render_volume_callback.take(),
        ) {
            unsafe {
                if let Err(err) = volume.UnregisterControlChangeNotify(&callback) {
                    eprintln!("failed to unregister render volume callback: {err:?}");
                }
            }
        }
    }

    fn render_identity(&self) -> Option<(String, String, u64)> {
        Some((
            self.render_device_id.clone()?,
            self.render_device_name.clone().unwrap_or_default(),
            self.render_generation,
        ))
    }
}

pub fn output_volume_state() -> OutputVolumeState {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .output_volume
        .clone()
}

pub fn volume_module_enabled() -> bool {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .modules
        .volume
}

pub fn microphone_module_enabled() -> bool {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .modules
        .microphone
}

pub fn module_enabled(module: HotkeyModule) -> bool {
    match module {
        HotkeyModule::Microphone => microphone_module_enabled(),
        HotkeyModule::Volume => volume_module_enabled(),
        HotkeyModule::System => true,
    }
}

/// Move the default output by `steps` notches of `volume.wheel.step_percent`.
/// The result snaps onto that grid. A positive step unmutes.
///
/// Wheel and media-key stages call this. Nothing in this stage does, yet.
#[allow(dead_code)]
pub fn step_output_volume(steps: i32) -> Result<()> {
    if steps == 0 {
        return Ok(());
    }
    with_output_volume(|volume| step_output_volume_on(volume, steps))
}

/// `level` is a master scalar, clamped to `0.0..=1.0`.
pub fn set_output_volume(level: f32) -> Result<()> {
    let level = level.clamp(0.0, 1.0);
    with_output_volume(|volume| {
        unsafe {
            volume
                .SetMasterVolumeLevelScalar(level, &OUTPUT_VOLUME_EVENT_CONTEXT)
                .context("set output volume")?;
        }
        Ok(())
    })
}

pub fn toggle_output_mute() -> Result<()> {
    with_output_volume(|volume| {
        let muted = unsafe { volume.GetMute().context("read output mute")?.as_bool() };
        unsafe {
            volume
                .SetMute((!muted).into(), &OUTPUT_VOLUME_EVENT_CONTEXT)
                .context("toggle output mute")?;
        }
        Ok(())
    })
}

/// Cycle the console, multimedia and communications defaults together.
/// Favorites are used when at least one of them is currently active; otherwise
/// every active render device is used. The list is ordered by friendly name.
pub fn cycle_output_device(forward: bool) -> Result<()> {
    let favorites = STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .volume
        .favorite_output_devices
        .clone();
    let mut devices = list_render_devices()?;
    if devices.is_empty() {
        return Ok(());
    }
    if !favorites.is_empty() {
        let favorite: Vec<_> = devices
            .iter()
            .filter(|device| favorites.iter().any(|id| id == &device.id))
            .cloned()
            .collect();
        if !favorite.is_empty() {
            devices = favorite;
        }
    }
    devices.sort_by(|left, right| {
        left.friendly_name
            .to_lowercase()
            .cmp(&right.friendly_name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });

    let len = devices.len();
    let next_index = match devices.iter().position(|device| device.is_default) {
        Some(index) if forward => (index + 1) % len,
        Some(index) => (index + len - 1) % len,
        None if forward => 0,
        None => len - 1,
    };
    if devices[next_index].is_default {
        return Ok(());
    }
    set_default_audio_device(&devices[next_index].id)
}

/// Single place later stages attach to a level or mute change.
pub fn on_output_volume_changed(state: &OutputVolumeState, source: VolumeChangeSource) {
    // Own writes only: wheel, media keys, and any other silence! call that
    // passes `OUTPUT_VOLUME_EVENT_CONTEXT`. External changes from other apps
    // do not play. Device switches go through `on_output_device_changed` and
    // stay silent. Playback itself is debounced and skipped while muted.
    let arm = if let Ok(mut app) = STATE.lock() {
        app.output_volume = state.clone();
        app.output_volume_seeded = true;
        source == VolumeChangeSource::Own
            && app.modules.volume
            && app.volume.sound.enabled
            && !state.muted
    } else {
        false
    };
    // stage 2 hooks: refresh the volume tray icon
    // stage 3 hooks: show the volume OSD
    // stage 4: `consider_volume_change_sound`
    consider_volume_change_sound(source, state.muted, arm);
}

/// Single place later stages attach to a default-output switch.
/// Fires once per device id change, whether silence! or Windows switched it.
pub fn on_output_device_changed(state: &OutputVolumeState) {
    if let Ok(mut app) = STATE.lock() {
        app.output_volume = state.clone();
        app.output_volume_seeded = true;
    }
    // stage 2 hooks: refresh the volume tray icon
    // stage 3 hooks: show the volume OSD when `osd.show_on_device_change` is set
    // stage 4: no sound. Volume ticks are own level/mute changes only.
    cancel_volume_change_sound();
}

pub(crate) fn handle_output_volume_message(wparam: WPARAM, lparam: LPARAM) {
    let generation = (wparam.0 as u64) >> 2;
    let muted = wparam.0 & 1 != 0;
    let own = wparam.0 & 2 != 0;
    let level = f32::from_bits(lparam.0 as u32);
    if !level.is_finite() {
        return;
    }
    let Some((device_id, device_name, bound_generation)) = render_identity() else {
        return;
    };
    if generation != (bound_generation & RENDER_GENERATION_MASK) {
        return;
    }
    observe_output_state(
        OutputVolumeState {
            device_id,
            device_name,
            level: level.clamp(0.0, 1.0),
            muted,
        },
        Some(if own {
            VolumeChangeSource::Own
        } else {
            VolumeChangeSource::External
        }),
    );
}

pub(crate) fn handle_output_device_changed() {
    let previous_id = output_volume_state().device_id;
    let outcome = rebind_output_volume_for_message();
    // A same-id rebind means the old interface was dead. Publish the level we
    // just read; a real device switch is reported by `observe_output_state`.
    let same_device = matches!(
        &outcome,
        RenderVolumeBind::Bound(state) if state.device_id == previous_id
    );
    publish_render_bind(outcome, same_device);
}

/// Rebind the console render callback. If the cached interface is dead but the
/// device id did not change, replace it. Does not publish hub events.
pub(crate) fn rebind_output_volume_for_message() -> RenderVolumeBind {
    let outcome = rebind_output_volume_callback(false);
    if matches!(outcome, RenderVolumeBind::Unchanged) && read_bound_render_state().is_err() {
        rebind_output_volume_callback(true)
    } else {
        outcome
    }
}

/// Bind the render callback without telling the volume hub.
/// The settings process uses this so it does not run tray/OSD hooks.
pub(crate) fn rebind_output_volume_callback(force: bool) -> RenderVolumeBind {
    AUDIO_NOTIFICATION_REGISTRATION.with(|registration| {
        let mut registration = registration.borrow_mut();
        let Some(registration) = registration.as_mut() else {
            return RenderVolumeBind::Unavailable;
        };
        let rebound = registration.rebind_render_volume(force);
        match rebound {
            Ok(outcome) => outcome,
            Err(err) if error_is_device_invalidated(&err) && !force => {
                match registration.rebind_render_volume(true) {
                    Ok(outcome) => outcome,
                    Err(err) => {
                        eprintln!("failed to rebind render volume after invalidation: {err:?}");
                        registration.clear_render_binding();
                        RenderVolumeBind::Missing
                    }
                }
            }
            Err(err) if error_is_missing_endpoint(&err) => {
                registration.clear_render_binding();
                RenderVolumeBind::Missing
            }
            Err(err) => {
                eprintln!("failed to rebind render volume: {err:?}");
                RenderVolumeBind::Unavailable
            }
        }
    })
}

fn publish_render_bind(outcome: RenderVolumeBind, level_refresh: bool) {
    match outcome {
        RenderVolumeBind::Unavailable => {}
        RenderVolumeBind::Unchanged => {
            if !output_volume_is_seeded()
                && let Ok(state) = read_bound_render_state()
            {
                observe_output_state(state, None);
            }
        }
        RenderVolumeBind::Missing => observe_output_state(OutputVolumeState::default(), None),
        RenderVolumeBind::Bound(state) => observe_output_state(
            state,
            level_refresh.then_some(VolumeChangeSource::External),
        ),
    }
}

fn observe_output_state(state: OutputVolumeState, volume_source: Option<VolumeChangeSource>) {
    let previous = output_volume_state();
    let seeded = output_volume_is_seeded();
    if !seeded {
        let mut app = STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        app.output_volume = state;
        app.output_volume_seeded = true;
        return;
    }
    if previous.device_id != state.device_id {
        on_output_device_changed(&state);
        return;
    }
    if let Some(source) = volume_source
        && output_snapshot_changed(&previous, &state)
    {
        on_output_volume_changed(&state, source);
    }
}

fn output_volume_is_seeded() -> bool {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .output_volume_seeded
}

fn output_snapshot_changed(previous: &OutputVolumeState, next: &OutputVolumeState) -> bool {
    previous.device_id != next.device_id
        || previous.device_name != next.device_name
        || previous.muted != next.muted
        || (previous.level - next.level).abs() > 0.0005
}

fn render_identity() -> Option<(String, String, u64)> {
    AUDIO_NOTIFICATION_REGISTRATION
        .with(|registration| registration.borrow().as_ref().and_then(|registration| registration.render_identity()))
}

fn read_bound_render_state() -> Result<OutputVolumeState> {
    AUDIO_NOTIFICATION_REGISTRATION.with(|registration| {
        let registration = registration.borrow();
        let registration = registration
            .as_ref()
            .context("audio notifications are not registered")?;
        let volume = registration
            .render_volume
            .as_ref()
            .context("render volume is not bound")?;
        let device_id = registration
            .render_device_id
            .clone()
            .context("render device is not bound")?;
        let device_name = registration.render_device_name.clone().unwrap_or_default();
        let level = unsafe { volume.GetMasterVolumeLevelScalar().context("read render volume")? };
        let muted = unsafe { volume.GetMute().context("read render mute")?.as_bool() };
        Ok(OutputVolumeState {
            device_id,
            device_name,
            level: level.clamp(0.0, 1.0),
            muted,
        })
    })
}

#[allow(dead_code)]
fn step_output_volume_on(volume: &IAudioEndpointVolume, steps: i32) -> Result<()> {
    let step_percent = STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .volume
        .wheel
        .step_percent
        .max(1);
    let current = unsafe { volume.GetMasterVolumeLevelScalar().context("read output volume")? };
    let muted = unsafe { volume.GetMute().context("read output mute")?.as_bool() };
    let next = stepped_output_percent(current, steps, step_percent);
    unsafe {
        volume
            .SetMasterVolumeLevelScalar(f32::from(next) / 100.0, &OUTPUT_VOLUME_EVENT_CONTEXT)
            .context("step output volume")?;
        if steps > 0 && muted {
            volume
                .SetMute(false.into(), &OUTPUT_VOLUME_EVENT_CONTEXT)
                .context("unmute output while increasing volume")?;
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn stepped_output_percent(current: f32, steps: i32, step_percent: u8) -> u8 {
    let step = i32::from(step_percent.max(1));
    let current_percent = (current.clamp(0.0, 1.0) * 100.0).round() as i32;
    if steps == 0 {
        return current_percent.clamp(0, 100) as u8;
    }
    let remainder = current_percent.rem_euclid(step);
    let (on_grid, consumed) = if remainder == 0 {
        (current_percent, 0)
    } else if steps > 0 {
        (current_percent + (step - remainder), 1)
    } else {
        (current_percent - remainder, 1)
    };
    let remaining_steps = steps - steps.signum() * consumed;
    (on_grid + remaining_steps * step).clamp(0, 100) as u8
}

fn with_output_volume<T>(operation: impl Fn(&IAudioEndpointVolume) -> Result<T>) -> Result<T> {
    match output_volume() {
        Ok(volume) => match operation(&volume) {
            Err(err) if error_is_device_invalidated(&err) => {
                recover_invalidated_output_volume();
                operation(&output_volume()?)
            }
            other => other,
        },
        Err(err) if error_is_device_invalidated(&err) => {
            recover_invalidated_output_volume();
            operation(&output_volume()?)
        }
        Err(err) => Err(err),
    }
}

fn recover_invalidated_output_volume() {
    let hwnd = STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .hwnd;
    if hwnd.0.is_null() {
        return;
    }
    let on_window_thread =
        unsafe { GetWindowThreadProcessId(hwnd, None) == GetCurrentThreadId() };
    if on_window_thread {
        publish_render_bind(rebind_output_volume_callback(true), true);
    } else {
        post_audio_window_message(hwnd, WM_OUTPUT_DEVICE_CHANGED);
    }
}

#[derive(Clone)]
struct RenderDeviceChoice {
    id: String,
    friendly_name: String,
    is_default: bool,
}

fn list_render_devices() -> Result<Vec<RenderDeviceChoice>> {
    unsafe {
        let enumerator = audio_device_enumerator()?;
        let default_id = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .ok()
            .and_then(|device| endpoint_device_id(&device).ok());
        let collection = enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .context("enumerate render endpoints")?;
        let count = collection.GetCount().context("count render endpoints")?;
        let mut devices = Vec::with_capacity(count as usize);
        for index in 0..count {
            let device = collection.Item(index).context("get render endpoint")?;
            let id = endpoint_device_id(&device)?;
            let friendly_name = endpoint_friendly_name(&device);
            let is_default = default_id.as_deref() == Some(id.as_str());
            devices.push(RenderDeviceChoice {
                id,
                friendly_name,
                is_default,
            });
        }
        Ok(devices)
    }
}

fn win32_not_found() -> HRESULT {
    HRESULT::from_win32(ERROR_NOT_FOUND.0)
}

fn error_is_device_invalidated(err: &anyhow::Error) -> bool {
    windows_error_code(err).is_some_and(|code| code == AUDCLNT_E_DEVICE_INVALIDATED)
}

fn error_is_missing_endpoint(err: &anyhow::Error) -> bool {
    windows_error_code(err)
        .is_some_and(|code| code == win32_not_found() || code == AUDCLNT_E_DEVICE_INVALIDATED)
}

fn windows_error_code(err: &anyhow::Error) -> Option<HRESULT> {
    err.chain()
        .find_map(|cause| cause.downcast_ref::<windows::core::Error>().map(|error| error.code()))
}
