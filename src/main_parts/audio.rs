fn current_mute_state() -> Result<bool> {
    let volume = capture_volume(None)?;
    let muted = unsafe { volume.GetMute()? };
    Ok(muted.as_bool())
}

fn current_mic_in_use() -> Result<bool> {
    let ignored_apps = STATE.lock().unwrap().tray_icon.mic_in_use_ignored_apps.clone();
    Ok(!mic_using_apps_filtered(&ignored_apps)?.is_empty())
}

fn active_capture_session_process_ids() -> Result<Vec<u32>> {
    unsafe {
        let enumerator = audio_device_enumerator()?;
        let collection = enumerator
            .EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)
            .context("enumerate capture endpoints for use state")?;
        let count = collection
            .GetCount()
            .context("count capture endpoints for use state")?;

        let mut pids = Vec::new();
        let mut seen = HashSet::new();
        for index in 0..count {
            let device = collection
                .Item(index)
                .context("get capture endpoint for use state")?;
            pids.extend(capture_device_active_session_process_ids(&device)?);
        }

        pids.retain(|pid| *pid != 0 && seen.insert(*pid));
        Ok(pids)
    }
}

unsafe fn capture_device_active_session_process_ids(device: &IMMDevice) -> Result<Vec<u32>> {
    let session_manager: IAudioSessionManager2 = unsafe {
        device
            .Activate(CLSCTX_ALL, None)
            .context("activate capture session manager")?
    };
    let session_enumerator = unsafe {
        session_manager
            .GetSessionEnumerator()
            .context("get capture session enumerator")?
    };
    let count = unsafe { session_enumerator.GetCount().context("count capture sessions")? };
    let mut pids = Vec::new();

    for index in 0..count {
        let session = unsafe {
            session_enumerator
                .GetSession(index)
                .context("get capture session")?
        };
        let control: IAudioSessionControl = session.cast().context("cast capture session control")?;
        let state = unsafe { control.GetState().context("get capture session state")? };
        if state == AudioSessionStateActive {
            let control_2: IAudioSessionControl2 = session
                .cast()
                .context("cast capture session process control")?;
            let pid = unsafe { control_2.GetProcessId().context("get capture session pid")? };
            pids.push(pid);
        }
    }

    Ok(pids)
}

fn mic_using_apps() -> Result<Vec<MicUsingApp>> {
    mic_using_apps_filtered(&[])
}

fn mic_using_apps_filtered(ignored_apps: &[String]) -> Result<Vec<MicUsingApp>> {
    let mut apps = active_capture_session_process_ids()?
        .into_iter()
        .filter_map(|pid| {
            let process_image = process_image(pid);
            let exe_name = process_image
                .exe_name
                .clone()
                .unwrap_or_else(|| unknown_process_name(pid));
            if is_process_image_ignored(&exe_name, ignored_apps) {
                return None;
            }
            Some(MicUsingApp {
                pid,
                name: process_display_name(pid, process_image.image_path.as_deref(), &exe_name),
                exe_name,
                image_path: process_image.image_path,
            })
        })
        .collect::<Vec<_>>();
    apps.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.pid.cmp(&right.pid))
    });
    Ok(apps)
}

pub fn current_mic_using_apps() -> Vec<MicUsingApp> {
    active_capture_session_process_ids()
        .map(|pids| {
            pids.into_iter()
                .map(|pid| {
                    let process_image = process_image(pid);
                    let exe_name = process_image
                        .exe_name
                        .clone()
                        .unwrap_or_else(|| unknown_process_name(pid));
                    MicUsingApp {
                        pid,
                        name: process_display_name(pid, process_image.image_path.as_deref(), &exe_name),
                        exe_name,
                        image_path: process_image.image_path,
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn is_process_image_ignored(exe_name: &str, ignored_apps: &[String]) -> bool {
    ignored_apps
        .iter()
        .any(|ignored| exe_name.eq_ignore_ascii_case(ignored))
}

#[derive(Default)]
struct ProcessImage {
    exe_name: Option<String>,
    image_path: Option<PathBuf>,
}

fn process_image(pid: u32) -> ProcessImage {
    if let Some(image_path) = process_image_path(pid) {
        return ProcessImage {
            exe_name: process_image_name(&image_path),
            image_path: Some(image_path),
        };
    }

    ProcessImage {
        exe_name: process_snapshot_image_name(pid),
        image_path: None,
    }
}

fn normalized_process_image_name(name: &str) -> Option<String> {
    let trimmed = name.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }
    let file_name = Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed)
        .trim();
    if file_name.is_empty() {
        return None;
    }
    if file_name.contains('.') {
        Some(file_name.to_string())
    } else {
        Some(format!("{file_name}.exe"))
    }
}

fn process_display_name(pid: u32, path: Option<&Path>, exe_name: &str) -> String {
    if let Some(name) = path.and_then(|path| {
        path.file_stem()
            .or_else(|| path.file_name())
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
    }) {
        return name;
    }

    Path::new(exe_name)
        .file_stem()
        .or_else(|| Path::new(exe_name).file_name())
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(|name| name.to_string())
        .unwrap_or_else(|| unknown_process_name(pid))
}

fn process_image_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
}

fn process_image_path(pid: u32) -> Option<PathBuf> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = vec![0u16; 32768];
        let mut len = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        result.ok()?;
        buffer.truncate(len as usize);
        Some(PathBuf::from(String::from_utf16_lossy(&buffer)))
    }
}

fn process_snapshot_image_name(pid: u32) -> Option<String> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = Process32FirstW(snapshot, &mut entry).is_ok();
        while found {
            if entry.th32ProcessID == pid {
                let _ = CloseHandle(snapshot);
                return process_entry_image_name(&entry);
            }
            found = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        None
    }
}

fn process_entry_image_name(entry: &PROCESSENTRY32W) -> Option<String> {
    let len = entry
        .szExeFile
        .iter()
        .position(|ch| *ch == 0)
        .unwrap_or(entry.szExeFile.len());
    if len == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&entry.szExeFile[..len]))
}

fn unknown_process_name(pid: u32) -> String {
    format!("Process {pid}")
}

pub fn mic_mute_state(device_id: Option<&str>) -> Result<bool> {
    let volume = capture_volume(device_id)?;
    let muted = unsafe { volume.GetMute()? };
    Ok(muted.as_bool())
}

fn target_mute_state(device_id: Option<&str>) -> Result<bool> {
    if is_all_microphones_target(device_id) {
        return current_mute_state();
    }
    mic_mute_state(device_id.filter(|id| !id.is_empty()))
}

fn set_mute_to_inverse(device_id: Option<&str>) -> Result<bool> {
    if is_all_microphones_target(device_id) {
        let next = !current_mute_state()?;
        set_all_capture_devices_mute(next)?;
        return Ok(next);
    }
    let (volume, id) = capture_volume_with_id(device_id.filter(|id| !id.is_empty()))?;
    let next = unsafe { !volume.GetMute()?.as_bool() };
    apply_capture_mute(&volume, &id, next)?;
    Ok(next)
}

fn set_mute(device_id: Option<&str>, muted: bool) -> Result<bool> {
    if is_all_microphones_target(device_id) {
        set_all_capture_devices_mute(muted)?;
        return Ok(muted);
    }
    let (volume, id) = capture_volume_with_id(device_id.filter(|id| !id.is_empty()))?;
    apply_capture_mute(&volume, &id, muted)?;
    Ok(muted)
}

fn apply_capture_mute(volume: &IAudioEndpointVolume, device_id: &str, muted: bool) -> Result<()> {
    unsafe {
        if muted {
            if !volume.GetMute()?.as_bool() {
                let scalar = volume.GetMasterVolumeLevelScalar().unwrap_or(1.0);
                store_premute_capture_volume(device_id, scalar);
            }
            volume.SetMute(true, null())?;
            volume.SetMasterVolumeLevelScalar(0.0, null())?;
        } else {
            let restore = take_premute_capture_volume(device_id).or_else(|| {
                match volume.GetMasterVolumeLevelScalar() {
                    Ok(current) if current <= f32::EPSILON => Some(1.0),
                    _ => None,
                }
            });
            volume.SetMute(false, null())?;
            if let Some(scalar) = restore {
                volume.SetMasterVolumeLevelScalar(scalar, null())?;
            }
        }
    }
    Ok(())
}

fn store_premute_capture_volume(device_id: &str, scalar: f32) {
    CAPTURE_PREMUTE_VOLUMES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(device_id.to_string(), scalar);
}

fn take_premute_capture_volume(device_id: &str) -> Option<f32> {
    CAPTURE_PREMUTE_VOLUMES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(device_id)
}

fn hotkey_volume_target_percent(target: Option<&str>, fallback: u8) -> u8 {
    target
        .and_then(|target| target.trim().parse::<u8>().ok())
        .unwrap_or(fallback)
        .min(100)
}

fn set_output_volume_from_hotkey(target: Option<&str>) -> Result<()> {
    set_output_volume_percent(hotkey_volume_target_percent(target, 100))
}

fn change_output_volume_from_hotkey(target: Option<&str>, direction: i32) -> Result<()> {
    let amount = i32::from(hotkey_volume_target_percent(target, 10));
    let volume = output_volume()?;
    let current = unsafe { volume.GetMasterVolumeLevelScalar()? };
    let current_percent = (current * 100.0).round() as i32;
    let next = (current_percent + amount * direction).clamp(0, 100) as u8;
    set_output_volume_percent(next)
}

fn set_output_volume_percent(percent: u8) -> Result<()> {
    let volume = output_volume()?;
    let scalar = f32::from(percent.min(100)) / 100.0;
    unsafe {
        volume.SetMasterVolumeLevelScalar(scalar, null())?;
    }
    Ok(())
}

fn play_mute_sound(muted: bool) {
    let settings = STATE.lock().unwrap().sound_settings.clone();
    if !settings.enabled {
        return;
    }
    if let Err(err) = play_configured_sound(&settings, muted, settings.volume) {
        eprintln!("failed to play mute sound: {err:?}");
    }
}

fn play_hold_to_mute_sound(muted: bool) {
    let (sound_settings, hold_settings) = {
        let state = STATE.lock().unwrap();
        (state.sound_settings.clone(), state.hold_to_mute.clone())
    };
    if !sound_settings.enabled || !hold_settings.play_sounds {
        return;
    }

    let volume = hold_settings
        .volume_override
        .unwrap_or(sound_settings.volume)
        .min(100);
    let result = if muted {
        if let Some(theme) = hold_settings.mute_theme_override.as_deref() {
            play_theme_sound(theme, muted, volume)
        } else {
            play_configured_sound(&sound_settings, muted, volume)
        }
    } else if let Some(theme) = hold_settings.unmute_theme_override.as_deref() {
        play_theme_sound(theme, muted, volume)
    } else {
        play_configured_sound(&sound_settings, muted, volume)
    };

    if let Err(err) = result {
        eprintln!("failed to play hold-to-mute sound: {err:?}");
    }
}

fn play_auto_mute_sound() {
    let (sound_settings, auto_mute) = {
        let state = STATE.lock().unwrap();
        (state.sound_settings.clone(), state.auto_mute.clone())
    };
    if !sound_settings.enabled || !auto_mute.play_sounds {
        return;
    }

    if let Err(err) = play_configured_sound(&sound_settings, true, sound_settings.volume) {
        eprintln!("failed to play auto-mute sound: {err:?}");
    }
}

pub fn preview_sound(selection: &str, muted: bool, volume: u8) -> Result<u64> {
    let settings = STATE.lock().unwrap().sound_settings.clone();
    play_preview_sound_selection(selection, &settings, muted, volume)
}

pub fn stop_preview_sound() {
    if let Ok(mut audio) = AUDIO_ENGINE.lock()
        && let Some(engine) = audio.as_mut()
    {
        engine.stop_preview_sound();
    }
}

pub fn choose_custom_sounds() -> Result<Vec<CustomSound>> {
    let Some(sources) = rfd::FileDialog::new()
        .set_title("Add custom sound")
        .add_filter("Audio", &["mp3", "wav", "ogg", "flac"])
        .pick_files()
    else {
        return Ok(Vec::new());
    };

    sources
        .iter()
        .map(|source| import_custom_sound(source))
        .collect()
}

fn import_custom_sound(source: &Path) -> Result<CustomSound> {
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .filter(|extension| matches!(extension.as_str(), "mp3" | "wav" | "ogg" | "flac"))
        .context("selected file is not a supported audio format")?;
    let original_file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("custom sound")
        .to_string();
    let id = default_custom_sound_id();
    let destination_dir = custom_sounds_dir()?;
    fs::create_dir_all(&destination_dir)?;
    let destination = destination_dir.join(format!("{id}.{extension}"));
    fs::copy(source, &destination).with_context(|| {
        format!(
            "copy custom sound from {} to {}",
            source.display(),
            destination.display()
        )
    })?;

    Ok(CustomSound {
        id,
        path: destination,
        original_file_name,
    })
}

pub fn sound_themes() -> &'static [SoundTheme] {
    SOUND_THEMES
}

pub fn sound_theme_label(theme_id: &str) -> &'static str {
    SOUND_THEMES
        .iter()
        .find(|theme| theme.id == theme_id)
        .map(|theme| theme.label)
        .unwrap_or(SOUND_THEMES[0].label)
}

pub fn sound_selection_label<'a>(selection: &str, settings: &'a SoundSettings) -> &'a str {
    custom_sound_from_selection(selection, settings)
        .map(|sound| sound.original_file_name.as_str())
        .or_else(|| {
            SOUND_THEMES
                .iter()
                .find(|theme| theme.id == selection)
                .map(|theme| theme.label)
        })
        .unwrap_or(SOUND_THEMES[0].label)
}

fn prime_sound_assets(settings: &SoundSettings) {
    if !settings.enabled {
        return;
    }

    for muted in [true, false] {
        if let Err(err) = load_configured_sound(settings, muted) {
            eprintln!("failed to preload sound asset: {err:?}");
        }
    }
}

fn load_configured_sound(settings: &SoundSettings, muted: bool) -> Result<SamplesBuffer> {
    load_sound_selection(sound_selection_for(settings, muted), settings, muted)
}

fn play_configured_sound(settings: &SoundSettings, muted: bool, volume: u8) -> Result<()> {
    play_sound_selection(
        sound_selection_for(settings, muted),
        settings,
        muted,
        volume,
    )
}

fn sound_selection_for(settings: &SoundSettings, muted: bool) -> &str {
    if muted {
        settings.mute_theme.as_str()
    } else {
        settings.unmute_theme.as_str()
    }
}

fn play_sound_selection(
    selection: &str,
    settings: &SoundSettings,
    muted: bool,
    volume: u8,
) -> Result<()> {
    let volume = f32::from(volume.min(100)) / 100.0;
    let sound = load_sound_selection(selection, settings, muted)?;
    let mut audio = AUDIO_ENGINE.lock().unwrap();
    let engine = audio.as_mut().expect("audio engine initialized");
    engine.play_sound(sound, volume).map(|_| ())
}

fn play_preview_sound_selection(
    selection: &str,
    settings: &SoundSettings,
    muted: bool,
    volume: u8,
) -> Result<u64> {
    let volume = f32::from(volume.min(100)) / 100.0;
    let sound = load_sound_selection(selection, settings, muted)?;
    let mut audio = AUDIO_ENGINE.lock().unwrap();
    if audio.is_none() {
        *audio = Some(AudioEngine::new()?);
    }
    let engine = audio.as_mut().expect("audio engine initialized");
    let duration = engine.play_preview_sound(sound, volume)?;
    Ok(duration.as_millis().max(1) as u64)
}

fn load_sound_selection(
    selection: &str,
    settings: &SoundSettings,
    muted: bool,
) -> Result<SamplesBuffer> {
    if let Some(custom_sound) = custom_sound_from_selection(selection, settings) {
        match load_custom_sound(custom_sound) {
            Ok(sound) => return Ok(sound),
            Err(err) => {
                eprintln!(
                    "failed to load custom {} sound, falling back to theme: {err:?}",
                    if muted { "mute" } else { "unmute" }
                );
            }
        }
    }

    load_theme_sound(theme_from_selection(selection), muted)
}

fn custom_sound_from_selection<'a>(
    selection: &str,
    settings: &'a SoundSettings,
) -> Option<&'a CustomSound> {
    let id = custom_sound_id(selection)?;
    settings.custom_sounds.iter().find(|sound| sound.id == id)
}

fn custom_sound_id(selection: &str) -> Option<&str> {
    selection.strip_prefix("custom:")
}

fn custom_sound_value(id: &str) -> String {
    format!("custom:{id}")
}

fn theme_from_selection(selection: &str) -> &str {
    if custom_sound_id(selection).is_some() {
        SOUND_THEMES[0].id
    } else {
        selection
    }
}

fn load_theme_sound(theme: &str, muted: bool) -> Result<SamplesBuffer> {
    let file = sound_file_name(theme, muted);
    if let Some(bytes) = sound_asset_bytes(&file) {
        return load_decoded_sound_bytes(file, bytes);
    }

    let path = sound_asset_path(&file).with_context(|| format!("find sound asset {file}"))?;
    load_decoded_sound(file, &path)
}

fn load_custom_sound(custom_sound: &CustomSound) -> Result<SamplesBuffer> {
    let cache_key = custom_sound_cache_key(&custom_sound.path)?;
    load_decoded_sound(cache_key, &custom_sound.path)
}

fn load_decoded_sound(cache_key: String, path: &Path) -> Result<SamplesBuffer> {
    let mut audio = AUDIO_ENGINE.lock().unwrap();
    if audio.is_none() {
        *audio = Some(AudioEngine::new()?);
    }

    let engine = audio.as_mut().expect("audio engine initialized");
    engine.decoded_sound(&cache_key, path)
}

fn load_decoded_sound_bytes(cache_key: String, bytes: &'static [u8]) -> Result<SamplesBuffer> {
    let mut audio = AUDIO_ENGINE.lock().unwrap();
    if audio.is_none() {
        *audio = Some(AudioEngine::new()?);
    }

    let engine = audio.as_mut().expect("audio engine initialized");
    engine.decoded_sound_bytes(&cache_key, bytes.to_vec())
}

fn play_theme_sound(theme: &str, muted: bool, volume: u8) -> Result<()> {
    let volume = f32::from(volume.min(100)) / 100.0;
    let sound = load_theme_sound(theme, muted)?;
    let mut audio = AUDIO_ENGINE.lock().unwrap();
    let engine = audio.as_mut().expect("audio engine initialized");
    engine.play_sound(sound, volume).map(|_| ())
}

fn sound_file_name(theme: &str, muted: bool) -> String {
    let theme = if SOUND_THEMES.iter().any(|known| known.id == theme) {
        theme
    } else {
        SOUND_THEMES[0].id
    };
    let action = if muted { "mute" } else { "unmute" };
    format!("{theme}_{action}.mp3")
}

fn sound_asset_path(file: &str) -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        roots.extend(parent.ancestors().map(PathBuf::from));
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));

    roots
        .into_iter()
        .map(|root| root.join("assets").join("sounds").join(file))
        .find(|path| path.exists())
}

fn sound_asset_bytes(file: &str) -> Option<&'static [u8]> {
    Some(match file {
        "8bit_mute.mp3" => include_bytes!("../../assets/sounds/8bit_mute.mp3"),
        "8bit_unmute.mp3" => include_bytes!("../../assets/sounds/8bit_unmute.mp3"),
        "blob_mute.mp3" => include_bytes!("../../assets/sounds/blob_mute.mp3"),
        "blob_unmute.mp3" => include_bytes!("../../assets/sounds/blob_unmute.mp3"),
        "digital_mute.mp3" => include_bytes!("../../assets/sounds/digital_mute.mp3"),
        "digital_unmute.mp3" => include_bytes!("../../assets/sounds/digital_unmute.mp3"),
        "discord_mute.mp3" => include_bytes!("../../assets/sounds/discord_mute.mp3"),
        "discord_unmute.mp3" => include_bytes!("../../assets/sounds/discord_unmute.mp3"),
        "pop_mute.mp3" => include_bytes!("../../assets/sounds/pop_mute.mp3"),
        "pop_unmute.mp3" => include_bytes!("../../assets/sounds/pop_unmute.mp3"),
        "punchy_mute.mp3" => include_bytes!("../../assets/sounds/punchy_mute.mp3"),
        "punchy_unmute.mp3" => include_bytes!("../../assets/sounds/punchy_unmute.mp3"),
        "scifi_mute.mp3" => include_bytes!("../../assets/sounds/scifi_mute.mp3"),
        "scifi_unmute.mp3" => include_bytes!("../../assets/sounds/scifi_unmute.mp3"),
        "vibrant_mute.mp3" => include_bytes!("../../assets/sounds/vibrant_mute.mp3"),
        "vibrant_unmute.mp3" => include_bytes!("../../assets/sounds/vibrant_unmute.mp3"),
        _ => return None,
    })
}

fn custom_sound_cache_key(path: &Path) -> Result<String> {
    let metadata = path
        .metadata()
        .with_context(|| format!("read custom sound metadata {}", path.display()))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Ok(format!(
        "custom:{}:{}:{modified}",
        path.display(),
        metadata.len()
    ))
}

fn custom_sounds_dir() -> Result<PathBuf> {
    Ok(app_config_dir()?.join("custom_sounds"))
}

fn is_all_microphones_target(device_id: Option<&str>) -> bool {
    matches!(device_id, Some(id) if id == HOTKEY_TARGET_ALL_MICROPHONES)
}

fn capture_volume_with_id(device_id: Option<&str>) -> Result<(IAudioEndpointVolume, String)> {
    unsafe {
        let enumerator = audio_device_enumerator()?;
        let device = capture_device(&enumerator, device_id)?;
        let id = endpoint_device_id(&device)?;
        let volume = device
            .Activate(CLSCTX_ALL, None)
            .context("activate endpoint volume")?;
        Ok((volume, id))
    }
}

fn audio_device_enumerator() -> Result<IMMDeviceEnumerator> {
    unsafe {
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .context("create audio device enumerator")
    }
}

fn capture_volume(device_id: Option<&str>) -> Result<IAudioEndpointVolume> {
    unsafe {
        let enumerator = audio_device_enumerator()?;
        let device = capture_device(&enumerator, device_id)?;
        device
            .Activate(CLSCTX_ALL, None)
            .context("activate endpoint volume")
    }
}

fn output_volume() -> Result<IAudioEndpointVolume> {
    unsafe {
        let enumerator = audio_device_enumerator()?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .context("get default output endpoint")?;
        device
            .Activate(CLSCTX_ALL, None)
            .context("activate endpoint volume")
    }
}

unsafe fn capture_device(
    enumerator: &IMMDeviceEnumerator,
    device_id: Option<&str>,
) -> Result<IMMDevice> {
    if let Some(device_id) = device_id.filter(|id| !id.is_empty()) {
        let id = wide(device_id);
        if let Ok(device) = unsafe { enumerator.GetDevice(PCWSTR(id.as_ptr())) } {
            if unsafe { device.GetState()? } == DEVICE_STATE_ACTIVE {
                return Ok(device);
            }
        }
    }

    unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) }
        .context("get default capture endpoint")
}

fn active_capture_device_volumes() -> Result<Vec<(String, IAudioEndpointVolume)>> {
    unsafe {
        let enumerator = audio_device_enumerator()?;
        let collection = enumerator
            .EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)
            .context("enumerate capture endpoints")?;
        let count = collection.GetCount().context("count capture endpoints")?;
        let mut volumes = Vec::with_capacity(count as usize);

        for index in 0..count {
            let device = collection.Item(index).context("get capture endpoint")?;
            let id = endpoint_device_id(&device)?;
            let volume = device
                .Activate(CLSCTX_ALL, None)
                .context("activate endpoint volume")?;
            volumes.push((id, volume));
        }

        Ok(volumes)
    }
}

fn set_all_capture_devices_mute(muted: bool) -> Result<()> {
    for (id, volume) in active_capture_device_volumes()? {
        apply_capture_mute(&volume, &id, muted)?;
    }
    Ok(())
}

pub fn capture_devices() -> Result<Vec<MicDevice>> {
    endpoint_devices(eCapture, "Microphone").map(|devices| {
        devices
            .into_iter()
            .map(|device| MicDevice {
                id: device.id,
                name: device.name,
                system_name: device.system_name,
                is_default: device.is_default,
            })
            .collect()
    })
}

pub fn render_devices() -> Result<Vec<AudioDevice>> {
    endpoint_devices(eRender, "Speaker")
}

fn endpoint_devices(flow: EDataFlow, fallback_name: &str) -> Result<Vec<AudioDevice>> {
    unsafe {
        let enumerator = audio_device_enumerator()?;
        let default_id = endpoint_device_id(
            &enumerator
                .GetDefaultAudioEndpoint(flow, eConsole)
                .context("get default audio endpoint")?,
        )
        .ok();
        let collection = enumerator
            .EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)
            .context("enumerate audio endpoints")?;
        let count = collection.GetCount().context("count audio endpoints")?;
        let mut devices = Vec::with_capacity(count as usize);

        for index in 0..count {
            let device = collection.Item(index).context("get audio endpoint")?;
            let id = endpoint_device_id(&device)?;
            let (name, system_name) = endpoint_device_display_names(&device, &id, fallback_name);
            let is_default = default_id.as_deref() == Some(id.as_str());
            devices.push(AudioDevice {
                id,
                name,
                system_name,
                is_default,
            });
        }

        Ok(devices)
    }
}

pub fn set_default_capture_device(device_id: &str) -> Result<()> {
    set_default_audio_device(device_id)
}

pub fn set_default_render_device(device_id: &str) -> Result<()> {
    set_default_audio_device(device_id)
}

pub fn open_audio_device_properties(_device_id: &str, input: bool) -> Result<()> {
    if !_device_id.is_empty() {
        let target = format!("ms-mmsys:,{_device_id},general");
        if Command::new("rundll32.exe")
            .args(["shell32.dll,Control_RunDLL", "mmsys.cpl", target.as_str()])
            .spawn()
            .is_ok()
        {
            return Ok(());
        }
    }

    open_audio_device_control_panel(input)
}

pub fn rename_audio_device(device_id: &str, name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let device_id = wide(device_id);
    unsafe {
        let enumerator = audio_device_enumerator()?;
        let device = enumerator
            .GetDevice(PCWSTR(device_id.as_ptr()))
            .context("get audio endpoint")?;
        let store = device
            .OpenPropertyStore(STGM_READWRITE)
            .context("open audio endpoint property store")?;
        let value = audio_device_name_propvariant(trimmed)?;
        store
            .SetValue(&PKEY_Device_DeviceDesc, &value)
            .context("rename audio endpoint")?;
        store.Commit().context("commit audio endpoint rename")?;
    }
    Ok(())
}

fn audio_device_name_propvariant(name: &str) -> Result<PROPVARIANT> {
    let source_value = PROPVARIANT::from(name);
    let mut value = PROPVARIANT::default();
    unsafe {
        PropVariantChangeType(&mut value, &source_value, Default::default(), VT_LPWSTR)
            .context("convert audio endpoint name to LPWSTR")?;
    }
    Ok(value)
}

fn open_audio_device_control_panel(input: bool) -> Result<()> {
    let tab = if input { "1" } else { "0" };
    Command::new("rundll32.exe")
        .args(["shell32.dll,Control_RunDLL", &format!("mmsys.cpl,,{tab}")])
        .spawn()
        .context("open audio device properties")?;
    Ok(())
}

fn set_default_audio_device(device_id: &str) -> Result<()> {
    let device_id = wide(device_id);
    unsafe {
        let policy: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
            .context("create policy config client")?;
        for role in [eConsole, eMultimedia, eCommunications] {
            (Interface::vtable(&policy).SetDefaultEndpoint)(
                Interface::as_raw(&policy),
                PCWSTR(device_id.as_ptr()),
                role,
            )
            .ok()
            .context("set default audio endpoint")?;
        }
    }
    Ok(())
}

fn toggle_default_audio_device(
    flow: EDataFlow,
    target_1: Option<String>,
    target_2: Option<String>,
) -> Result<()> {
    let Some(target_1) = target_1.filter(|target| !target.is_empty()) else {
        return Ok(());
    };
    let Some(target_2) = target_2.filter(|target| !target.is_empty()) else {
        return Ok(());
    };

    let devices = endpoint_devices(flow, "Device")?;
    let next = if devices
        .iter()
        .any(|device| device.is_default && device.id == target_1)
    {
        target_2
    } else {
        target_1
    };

    set_default_audio_device(&next)
}

pub fn default_mic_label(devices: &[MicDevice]) -> String {
    devices
        .iter()
        .find(|device| device.is_default)
        .map(|device| device.display_name(AUDIO_DEVICE_NAME_PRETTY))
        .unwrap_or_else(|| "Default microphone".to_string())
}

unsafe fn endpoint_device_id(device: &IMMDevice) -> Result<String> {
    let id = unsafe { device.GetId()? };
    let text = unsafe { pwstr_to_string(id) };
    unsafe { CoTaskMemFree(Some(id.0 as *const c_void)) };
    Ok(text)
}

fn endpoint_device_display_names(
    device: &IMMDevice,
    device_id: &str,
    fallback_name: &str,
) -> (String, String) {
    unsafe {
        let Some(store) = device.OpenPropertyStore(STGM_READ).ok() else {
            return (fallback_name.to_string(), String::new());
        };
        let raw_friendly =
            endpoint_property_string(&store, &PKEY_Device_FriendlyName).unwrap_or_default();
        let raw_desc =
            endpoint_property_string(&store, &PKEY_Device_DeviceDesc).unwrap_or_default();
        let (friendly_name, friendly_system_name) = split_audio_device_name(&raw_friendly);
        let raw_desc = raw_desc.trim().to_string();
        if !raw_desc.is_empty() {
            normalize_audio_device_name_variant(device, device_id, &raw_desc);
        }

        let name = if raw_desc.is_empty() {
            if friendly_name.trim().is_empty() {
                fallback_name.to_string()
            } else {
                friendly_name
            }
        } else {
            raw_desc
        };

        (name, friendly_system_name)
    }
}

fn normalize_audio_device_name_variant(device: &IMMDevice, device_id: &str, name: &str) {
    let normalized_key = (device_id.to_string(), name.to_string());
    {
        let mut normalized = NORMALIZED_AUDIO_DEVICE_NAMES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if normalized.contains(&normalized_key) {
            return;
        }
        normalized.insert(normalized_key);
    }

    let Ok(value) = audio_device_name_propvariant(name) else {
        return;
    };

    unsafe {
        if let Ok(store) = device.OpenPropertyStore(STGM_READWRITE) {
            let _ = store.SetValue(&PKEY_Device_DeviceDesc, &value);
            let _ = store.Commit();
        }
    }
}

fn endpoint_property_string(
    store: &windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore,
    key: &windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY,
) -> Option<String> {
    let value = unsafe { store.GetValue(key).ok()? };
    let name = value.to_string();
    if name.is_empty() { None } else { Some(name) }
}

fn split_audio_device_name(name: &str) -> (String, String) {
    let trimmed = name.trim();
    if trimmed.ends_with(')') {
        if let Some(open_index) = trimmed.rfind(" (") {
            let pretty_name = trimmed[..open_index].trim();
            let system_name = trimmed[open_index + 2..trimmed.len() - 1].trim();
            if !pretty_name.is_empty() && !system_name.is_empty() {
                return (pretty_name.to_string(), system_name.to_string());
            }
        }
    }

    (trimmed.to_string(), trimmed.to_string())
}

fn display_audio_device_name(pretty_name: &str, system_name: &str, mode: &str) -> String {
    match normalize_audio_device_name_display(mode) {
        AUDIO_DEVICE_NAME_SYSTEM => system_name.to_string(),
        AUDIO_DEVICE_NAME_BOTH => {
            if pretty_name == system_name || system_name.is_empty() {
                pretty_name.to_string()
            } else {
                format!("{pretty_name} ({system_name})")
            }
        }
        _ => pretty_name.to_string(),
    }
}

fn normalize_audio_device_name_display(mode: &str) -> &'static str {
    match mode {
        AUDIO_DEVICE_NAME_SYSTEM => AUDIO_DEVICE_NAME_SYSTEM,
        AUDIO_DEVICE_NAME_BOTH => AUDIO_DEVICE_NAME_BOTH,
        _ => AUDIO_DEVICE_NAME_PRETTY,
    }
}
