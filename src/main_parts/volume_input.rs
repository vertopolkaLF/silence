//! Tray-only wiring from the low-level hooks to output volume.
//!
//! The settings process installs `mouse_proc` on its own thread. It never sets
//! [`TRAY_VOLUME_HOOKS`], so it does not swallow wheels, post volume messages,
//! or reinstall hooks.

static TRAY_VOLUME_HOOKS: AtomicBool = AtomicBool::new(false);
static VOLUME_MODULE_ON: AtomicBool = AtomicBool::new(false);
static MEDIA_KEYS_INTERCEPT: AtomicBool = AtomicBool::new(false);
static WHEEL_ZONE: AtomicU8 = AtomicU8::new(0);
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
static LAST_HOOK_MILLIS: AtomicU64 = AtomicU64::new(0);
static HOOK_CLOCK: Lazy<Instant> = Lazy::new(Instant::now);
/// Latest `WheelTargets::icon_hidden_in_overflow` from the tray UI thread.
///
/// TODO: the settings process cannot see this yet. Share it over IPC when the
/// settings UI needs the "icon is in the overflow" hint.
static VOLUME_ICON_HIDDEN_IN_OVERFLOW: AtomicBool = AtomicBool::new(false);
static WHEEL_TARGETS: Lazy<RwLock<wheel_volume::WheelTargets>> =
    Lazy::new(|| RwLock::new(wheel_volume::WheelTargets::new()));
static PENDING_WHEELS: Lazy<Mutex<VecDeque<(i32, i32, i16)>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(32)));
static WHEEL_ACCUMULATOR: Lazy<Mutex<wheel_volume::WheelAccumulator>> =
    Lazy::new(|| Mutex::new(wheel_volume::WheelAccumulator::new()));

/// Called once the hidden window exists, before the low-level hooks are installed.
fn mark_tray_input_hooks_ready() {
    TRAY_VOLUME_HOOKS.store(true, Ordering::Relaxed);
    note_hook_event();
    sync_volume_input_runtime();
}

fn note_hook_event() {
    let millis = HOOK_CLOCK.elapsed().as_millis();
    LAST_HOOK_MILLIS.store(millis.min(u128::from(u64::MAX)) as u64, Ordering::Relaxed);
}

fn last_hook_event() -> Instant {
    *HOOK_CLOCK + Duration::from_millis(LAST_HOOK_MILLIS.load(Ordering::Relaxed))
}

fn tray_hwnd() -> HWND {
    HWND(TRAY_HWND.load(Ordering::Relaxed) as *mut c_void)
}

fn wheel_zone_code(zone: WheelZone) -> u8 {
    match zone {
        WheelZone::Off => 0,
        WheelZone::TrayIcon => 1,
        WheelZone::NotificationArea => 2,
        WheelZone::Taskbar => 3,
    }
}

fn hook_wheel_zone() -> wheel_volume::WheelZone {
    match WHEEL_ZONE.load(Ordering::Relaxed) {
        1 => wheel_volume::WheelZone::TrayIcon,
        2 => wheel_volume::WheelZone::NotificationArea,
        3 => wheel_volume::WheelZone::Taskbar,
        _ => wheel_volume::WheelZone::Off,
    }
}

fn wheel_targets_refresh_wanted() -> bool {
    TRAY_VOLUME_HOOKS.load(Ordering::Relaxed)
        && VOLUME_MODULE_ON.load(Ordering::Relaxed)
        && WHEEL_ZONE.load(Ordering::Relaxed) != 0
}

fn wheel_refresh_ms() -> u32 {
    wheel_volume::REFRESH_INTERVAL.as_millis().min(u128::from(u32::MAX)) as u32
}

/// Publish hook flags and start or stop the wheel-rect timer.
///
/// No-op in the settings process. Call on the tray UI thread (`apply_live_config`
/// and once at startup, because the first config load does not go through
/// `apply_live_config` after the hidden window exists).
fn sync_volume_input_runtime() {
    if !TRAY_VOLUME_HOOKS.load(Ordering::Relaxed) {
        return;
    }
    let (hwnd, module_on, zone, intercept, volume_sound) = {
        let state = STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            state.hwnd,
            state.modules.volume,
            state.volume.wheel.zone,
            state.volume.media_keys.intercept,
            state.volume.sound.clone(),
        )
    };
    let zone_code = wheel_zone_code(zone);
    let was_refreshing =
        VOLUME_MODULE_ON.load(Ordering::Relaxed) && WHEEL_ZONE.load(Ordering::Relaxed) != 0;
    VOLUME_MODULE_ON.store(module_on, Ordering::Relaxed);
    WHEEL_ZONE.store(zone_code, Ordering::Relaxed);
    MEDIA_KEYS_INTERCEPT.store(intercept, Ordering::Relaxed);
    TRAY_HWND.store(hwnd.0 as isize, Ordering::Relaxed);

    if module_on {
        prime_sound_assets(&volume_sound);
    } else {
        cancel_volume_change_sound();
    }
    if hwnd.0.is_null() {
        return;
    }

    // `SetTimer` restarts the countdown, so leave a running timer alone when an
    // unrelated setting is saved. Rects still refresh on the 1s tick.
    let refresh = module_on && zone_code != 0;
    unsafe {
        if refresh && !was_refreshing {
            let _ = SetTimer(hwnd, ID_WHEEL_REFRESH_TIMER, wheel_refresh_ms(), None);
        } else if !refresh && was_refreshing {
            let _ = KillTimer(hwnd, ID_WHEEL_REFRESH_TIMER);
            if let Ok(mut accumulator) = WHEEL_ACCUMULATOR.lock() {
                *accumulator = wheel_volume::WheelAccumulator::new();
            }
        }
    }
    if refresh && !was_refreshing {
        refresh_wheel_targets();
    }
}

/// Rebuild cached wheel hit rects. Tray UI thread.
///
/// Safe to call when the volume icon does not exist yet: the icon rect stays
/// empty and taskbar / notification rects still update. The tray-icon code
/// should call this again after `NIM_ADD` / `NIM_MODIFY` on [`ID_VOLUME_TRAY`].
pub(crate) fn refresh_wheel_targets() {
    if !TRAY_VOLUME_HOOKS.load(Ordering::Relaxed) {
        return;
    }
    let mut next = wheel_volume::WheelTargets::new();
    next.refresh(tray_hwnd(), ID_VOLUME_TRAY);
    // TODO: settings cannot read `VOLUME_ICON_HIDDEN_IN_OVERFLOW` (no IPC).
    let hidden = next.icon_hidden_in_overflow();
    {
        let mut targets = WHEEL_TARGETS
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *targets = next;
    }
    VOLUME_ICON_HIDDEN_IN_OVERFLOW.store(hidden, Ordering::Relaxed);
}

#[allow(dead_code)] // later settings bridge; this stage only stores the flag
pub(crate) fn volume_icon_hidden_in_overflow() -> bool {
    VOLUME_ICON_HIDDEN_IN_OVERFLOW.load(Ordering::Relaxed)
}

/// `true` when this wheel event was taken by the tray. Never blocks.
fn volume_wheel_should_swallow(message: u32, lparam: LPARAM) -> bool {
    if !TRAY_VOLUME_HOOKS.load(Ordering::Relaxed) || !VOLUME_MODULE_ON.load(Ordering::Relaxed) {
        return false;
    }
    let zone = hook_wheel_zone();
    if zone == wheel_volume::WheelZone::Off {
        return false;
    }
    let Some((pt, delta, injected)) = wheel_volume::decode_wheel(message, lparam) else {
        return false;
    };
    if injected {
        return false;
    }
    let Ok(targets) = WHEEL_TARGETS.try_read() else {
        return false;
    };
    if !targets.hit(pt, zone) {
        return false;
    }
    drop(targets);
    enqueue_wheel(pt.x, pt.y, delta)
}

fn enqueue_wheel(x: i32, y: i32, delta: i16) -> bool {
    let Ok(mut pending) = PENDING_WHEELS.try_lock() else {
        return false;
    };
    pending.push_back((x, y, delta));
    drop(pending);

    let hwnd = tray_hwnd();
    if hwnd.0.is_null() {
        pop_pending_wheel();
        return false;
    }
    let posted = unsafe { PostMessageW(hwnd, WM_VOLUME_WHEEL, WPARAM(0), LPARAM(0)).is_ok() };
    if !posted {
        pop_pending_wheel();
    }
    posted
}

fn pop_pending_wheel() {
    if let Ok(mut pending) = PENDING_WHEELS.try_lock() {
        pending.pop_back();
    }
}

fn drain_pending_wheels() -> Vec<(i32, i32, i16)> {
    match PENDING_WHEELS.lock() {
        Ok(mut pending) => pending.drain(..).collect(),
        Err(poisoned) => poisoned.into_inner().drain(..).collect(),
    }
}

fn handle_volume_wheel_message() {
    let pending = drain_pending_wheels();
    if !volume_module_enabled() {
        return;
    }
    // Do not hold `WHEEL_ACCUMULATOR` across the fullscreen query. That call
    // can pump, and this handler would deadlock itself on the same thread.
    let mut steps = 0i32;
    for (x, y, delta) in pending {
        if wheel_volume::fullscreen_blocks_wheel(POINT { x, y }) {
            continue;
        }
        let mut accumulator = WHEEL_ACCUMULATOR
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        steps = steps.saturating_add(accumulator.feed(delta));
    }
    if steps != 0
        && let Err(err) = step_output_volume(steps)
    {
        eprintln!("failed to step output volume from the wheel: {err:?}");
    }
}

/// Hotkey matching has already run. A binding whose `vk` is this volume key
/// wins: the key is not swallowed, so existing hotkey behavior stays intact.
fn swallow_volume_media_key(
    vk: u32,
    flags: u32,
    is_down: bool,
    is_up: bool,
    hotkey_claimed: bool,
) -> bool {
    if hotkey_claimed || (!is_down && !is_up) {
        return false;
    }
    if !TRAY_VOLUME_HOOKS.load(Ordering::Relaxed)
        || !VOLUME_MODULE_ON.load(Ordering::Relaxed)
        || !MEDIA_KEYS_INTERCEPT.load(Ordering::Relaxed)
        || !is_volume_media_vk(vk)
    {
        return false;
    }
    // `LLKHF_INJECTED` and lower-IL injected keys are ignored so another tool's
    // synthesized volume key cannot loop with this hook.
    if flags & (LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED) != 0 {
        return false;
    }
    if is_down {
        let hwnd = tray_hwnd();
        if !hwnd.0.is_null() {
            unsafe {
                let _ = PostMessageW(hwnd, WM_VOLUME_MEDIA_KEY, WPARAM(vk as usize), LPARAM(0));
            }
        }
    }
    true
}

fn is_volume_media_vk(vk: u32) -> bool {
    matches!(vk, VK_VOLUME_MUTE | VK_VOLUME_DOWN | VK_VOLUME_UP)
}

fn handle_volume_media_key(vk: usize) {
    if !volume_module_enabled() {
        return;
    }
    let intercept = STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .volume
        .media_keys
        .intercept;
    if !intercept {
        return;
    }
    let result = match vk as u32 {
        VK_VOLUME_UP => step_output_volume(1),
        VK_VOLUME_DOWN => step_output_volume(-1),
        VK_VOLUME_MUTE => toggle_output_mute(),
        _ => return,
    };
    if let Err(err) = result {
        eprintln!("volume media key failed: {err:?}");
    }
}

fn maintain_low_level_hooks() {
    // Settings never starts this timer and never sets the flag. Reinstalling
    // here would unhook that process's mouse-recording thread.
    if !TRAY_VOLUME_HOOKS.load(Ordering::Relaxed) {
        return;
    }
    if wheel_volume::should_reinstall_hook(last_hook_event()) {
        reinstall_low_level_hooks();
    }
}

fn reinstall_low_level_hooks() {
    let (keyboard, mouse) = {
        let state = STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        (state.hook, state.mouse_hook)
    };
    unsafe {
        if !keyboard.0.is_null() {
            let _ = UnhookWindowsHookEx(keyboard);
        }
        if !mouse.0.is_null() {
            let _ = UnhookWindowsHookEx(mouse);
        }
    }
    let instance = match unsafe { GetModuleHandleW(None) } {
        Ok(instance) => instance,
        Err(err) => {
            eprintln!("failed to reinstall low-level hooks: {err:?}");
            note_hook_event();
            return;
        }
    };
    if let Err(err) = install_keyboard_hook(instance.into()) {
        eprintln!("failed to reinstall keyboard hook: {err:?}");
    }
    if let Err(err) = install_mouse_hook(instance.into()) {
        eprintln!("failed to reinstall mouse hook: {err:?}");
    }
    // A failed install must not retry on every 1s tick while the user is active.
    note_hook_event();
}

/// `arm` is already "own change, module on, sound enabled, not muted".
fn consider_volume_change_sound(source: VolumeChangeSource, muted: bool, arm: bool) {
    if arm && !muted {
        schedule_volume_change_sound();
        return;
    }
    // External changes leave a pending own tick queued. Mute, or an own change
    // that is no longer eligible, cancels it.
    if muted || source == VolumeChangeSource::Own {
        cancel_volume_change_sound();
    }
}

fn schedule_volume_change_sound() {
    let hwnd = tray_hwnd();
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let _ = KillTimer(hwnd, ID_VOLUME_SOUND_TIMER);
        let _ = SetTimer(hwnd, ID_VOLUME_SOUND_TIMER, VOLUME_SOUND_DEBOUNCE_MS, None);
    }
}

fn cancel_volume_change_sound() {
    let hwnd = tray_hwnd();
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let _ = KillTimer(hwnd, ID_VOLUME_SOUND_TIMER);
    }
}

fn play_debounced_volume_change_sound() {
    let (module_on, muted, settings) = {
        let state = STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            state.modules.volume,
            state.output_volume.muted,
            state.volume.sound.clone(),
        )
    };
    // Scheduled only for silence!'s own writes (wheel, media keys, other
    // `OUTPUT_VOLUME_EVENT_CONTEXT` calls). External apps do not arm the timer.
    // The tick uses the unmute clip. `mute_theme` is unused: playback is
    // skipped entirely while the output is muted. Device changes never get here.
    if !module_on || muted || !settings.enabled {
        return;
    }
    if let Err(err) = play_configured_sound(&settings, false, settings.volume) {
        eprintln!("failed to play volume change sound: {err:?}");
    }
}
