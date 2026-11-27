// Work that used to block the tray's main thread.
//
// The main thread runs the volume wheel, volume keys and hotkey actions. Shell
// calls into explorer and audio device enumeration can each take tens to
// hundreds of milliseconds (explorer stalls while it updates the media
// flyout), and everything queued behind them waited. These two threads take
// that work; the main thread only gets the results.

/// Wakes the wheel-rect thread. `None` until it runs.
static WHEEL_RECTS_WAKE: Lazy<Mutex<Option<flume::Sender<()>>>> = Lazy::new(|| Mutex::new(None));
/// Jobs for the audio worker. `None` until it runs.
static AUDIO_JOBS: Lazy<Mutex<Option<flume::Sender<AudioJob>>>> = Lazy::new(|| Mutex::new(None));

enum AudioJob {
    /// Capture endpoints were added, removed or changed default.
    EndpointsChanged,
    HandsfreeChanged,
    /// Re-read "microphone in use" now instead of at the next poll.
    MicInUse,
}

/// Start both threads. Tray process only; later calls are free.
fn start_background_workers() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let (wake, woken) = flume::unbounded::<()>();
        *WHEEL_RECTS_WAKE.lock().unwrap() = Some(wake);
        let spawned = thread::Builder::new()
            .name("silence-wheel-rects".into())
            .spawn(move || {
                while woken.recv().is_ok() {
                    // A burst of wakes (timer, icon re-add, display change)
                    // needs one pass.
                    while woken.try_recv().is_ok() {}
                    if wheel_targets_refresh_wanted() {
                        refresh_wheel_targets_now();
                    }
                }
            });
        if let Err(err) = spawned {
            eprintln!("failed to start wheel rect thread: {err:?}");
            *WHEEL_RECTS_WAKE.lock().unwrap() = None;
        }

        let (jobs, received) = flume::unbounded::<AudioJob>();
        *AUDIO_JOBS.lock().unwrap() = Some(jobs);
        let spawned = thread::Builder::new()
            .name("silence-audio-worker".into())
            .spawn(move || run_audio_worker(received));
        if let Err(err) = spawned {
            eprintln!("failed to start audio worker: {err:?}");
            *AUDIO_JOBS.lock().unwrap() = None;
        }
    });
}

/// Rebuild the cached wheel hit rects off the main thread. Runs them here when
/// the worker is not there (settings process, startup failure).
pub(crate) fn refresh_wheel_targets() {
    let sent = WHEEL_RECTS_WAKE
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|wake| wake.send(()).is_ok());
    if !sent {
        refresh_wheel_targets_now();
    }
}

/// Endpoint changes: Bluetooth hands-free blocking and the microphone switch
/// OSD go to the audio worker. Runs them here without one.
fn audio_endpoints_changed_in_background() {
    if !send_audio_job(AudioJob::EndpointsChanged) {
        enforce_bluetooth_handsfree_block();
        on_capture_endpoints_changed();
    }
}

/// Ask for a fresh "microphone in use" reading. The result arrives as
/// `WM_MIC_IN_USE`. Without the worker it is read here.
fn refresh_mic_in_use_state() {
    if !send_audio_job(AudioJob::MicInUse) {
        refresh_mic_in_use_state_now();
    }
}

fn send_audio_job(job: AudioJob) -> bool {
    AUDIO_JOBS
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|jobs| jobs.send(job).is_ok())
}

fn run_audio_worker(jobs: flume::Receiver<AudioJob>) {
    // Core Audio is free-threaded; this thread needs COM, not an STA.
    if let Err(err) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
        eprintln!("audio worker: COM init failed: {err:?}");
        return;
    }
    let interval = Duration::from_millis(MIC_IN_USE_REFRESH_MS);
    let mut next_poll = Instant::now();
    // Unknown until the first reading, so that one is always posted.
    let mut last_in_use: Option<bool> = None;
    loop {
        let wait = next_poll.saturating_duration_since(Instant::now());
        let mut poll_now = false;
        match jobs.recv_timeout(wait) {
            Ok(job) => {
                for job in std::iter::once(job).chain(jobs.try_iter()) {
                    match job {
                        AudioJob::EndpointsChanged => {
                            enforce_bluetooth_handsfree_block();
                            on_capture_endpoints_changed();
                            // A device that came or went may change use state.
                            poll_now = true;
                        }
                        AudioJob::HandsfreeChanged => enforce_bluetooth_handsfree_block(),
                        AudioJob::MicInUse => {
                            poll_now = true;
                            last_in_use = None;
                        }
                    }
                }
            }
            Err(flume::RecvTimeoutError::Timeout) => poll_now = true,
            Err(flume::RecvTimeoutError::Disconnected) => break,
        }
        if !poll_now {
            continue;
        }
        next_poll = Instant::now() + interval;
        if !microphone_module_enabled() {
            last_in_use = None;
            continue;
        }
        let in_use = match current_mic_in_use() {
            Ok(in_use) => in_use,
            Err(err) => {
                eprintln!("failed to refresh microphone use state: {err:?}");
                continue;
            }
        };
        if last_in_use == Some(in_use) {
            continue;
        }
        last_in_use = Some(in_use);
        let hwnd = STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).hwnd;
        if !hwnd.0.is_null() {
            unsafe {
                let _ = PostMessageW(hwnd, WM_MIC_IN_USE, WPARAM(in_use as usize), LPARAM(0));
            }
        }
    }
}
