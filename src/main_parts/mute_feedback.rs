const MUTE_FAILURE_DURATION_MS: u32 = 5_000;
const MUTE_FAILURE_ICON_PAIR: &str = "__mute_failure__";
static MUTE_OPERATION_SEQUENCE: AtomicIsize = AtomicIsize::new(0);

#[derive(Clone, Copy)]
struct MuteFailureNotice {
    label: &'static str,
    expires_at: Instant,
}

fn report_mute_failure(requested: Option<bool>, err: &anyhow::Error) {
    eprintln!("microphone mute operation failed: {err:#}");
    let hwnd = STATE.lock().unwrap().hwnd;
    if hwnd.0.is_null() {
        return;
    }
    let action = match requested {
        Some(true) => 1,
        Some(false) => 0,
        None => 2,
    };
    let sequence = MUTE_OPERATION_SEQUENCE
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1);
    // Gamepad actions run on worker threads. Render and play feedback on the window thread.
    unsafe {
        let _ = PostMessageW(hwnd, WM_MUTE_FAILED, WPARAM(action), LPARAM(sequence));
    }
}

fn show_mute_failure_notice(action: usize, sequence: isize) {
    if MUTE_OPERATION_SEQUENCE.load(Ordering::Relaxed) != sequence {
        return;
    }
    let label = match action {
        1 => "Couldn't mute microphone",
        0 => "Couldn't unmute microphone",
        _ => "Couldn't change microphone mute",
    };
    let now = Instant::now();
    let (hwnd, play_sound) = {
        let mut state = STATE.lock().unwrap();
        let play_sound = !state
            .mute_failure
            .is_some_and(|notice| notice.label == label && notice.expires_at > now);
        state.mute_failure = Some(MuteFailureNotice {
            label,
            expires_at: now + Duration::from_millis(u64::from(MUTE_FAILURE_DURATION_MS)),
        });
        (state.hwnd, play_sound)
    };
    apply_overlay_visibility();
    unsafe {
        let _ = SetTimer(hwnd, ID_MUTE_FAILURE_TIMER, MUTE_FAILURE_DURATION_MS, None);
    }
    if play_sound {
        play_mute_failure_sound();
    }
}

fn clear_mute_failure_notice() {
    let (hwnd, had_failure) = {
        let mut state = STATE.lock().unwrap();
        (state.hwnd, state.mute_failure.take().is_some())
    };
    if had_failure {
        unsafe {
            let _ = KillTimer(hwnd, ID_MUTE_FAILURE_TIMER);
        }
        apply_overlay_visibility();
    }
}

fn expire_mute_failure_notice() {
    let expired = STATE
        .lock()
        .unwrap()
        .mute_failure
        .is_some_and(|notice| notice.expires_at <= Instant::now());
    if expired {
        clear_mute_failure_notice();
    }
}

fn mute_failure_overlay(settings: &OverlayConfig, label: &str) -> OverlayConfig {
    // Keep placement, but never let icon-only, invisible, or click bindings hide the warning.
    OverlayConfig {
        theme: settings.theme.clone(),
        corner_radii: settings.corner_radii.clone(),
        enabled: true,
        visibility: "Always".to_string(),
        display: settings.display.clone(),
        position_x: settings.position_x,
        position_y: settings.position_y,
        scale: settings.scale.clamp(100, 150),
        show_text: true,
        muted_label: label.to_string(),
        unmuted_label: label.to_string(),
        text_font: gpui_overlay::fonts::DEFAULT.to_string(),
        text_font_weight: 600,
        variant: "MicIcon".to_string(),
        icon_pair: MUTE_FAILURE_ICON_PAIR.to_string(),
        icon_style: "Colored".to_string(),
        background_style: "Dark".to_string(),
        background_opacity: 100,
        content_opacity: 100,
        behaviour: "PassThrough".to_string(),
        ..OverlayConfig::default()
    }
}

fn play_mute_failure_sound() {
    let result = (|| -> Result<()> {
        let volume = STATE.lock().unwrap().sound_settings.volume;
        let sound = load_decoded_sound_bytes(
            "mute_failure.wav".to_string(),
            include_bytes!("../../assets/sounds/mute_failure.wav"),
        )?;
        let mut audio = AUDIO_ENGINE.lock().unwrap();
        let engine = audio
            .as_mut()
            .context("audio engine unavailable for mute warning")?;
        // Error feedback is independent of normal mute/unmute sounds and their themes.
        engine.play_sound(sound, f32::from(volume.min(100)) / 100.0)?;
        Ok(())
    })();
    if let Err(err) = result {
        eprintln!("failed to play microphone mute failure sound: {err:#}");
    }
}
