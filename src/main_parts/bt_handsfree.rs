// Keeps Bluetooth headphones out of Hands-Free (HFP) mode.
//
// Windows switches a Bluetooth headset from A2DP (stereo, full quality) to
// HFP (mono, telephone quality) as soon as an app opens its "Headset"
// microphone or speaker. Those "Headset" endpoints are created by the
// Bluetooth Hands-Free audio driver (BTHHFENUM); the A2DP "Headphones"
// endpoint comes from BTHENUM and is left alone.
//
// Disabling the HFP endpoints (IPolicyConfig::SetEndpointVisibility, the same
// call as "Disable" in the Sound control panel) leaves apps nothing to open, so
// the headset stays in A2DP. Every endpoint change re-checks, which covers a
// newly paired or reconnected headset. The endpoints silence! disabled are
// remembered on disk, so turning the setting off re-enables only those and
// never one the user disabled by hand.

const PKEY_ENDPOINT_DEVNODE_PATH: windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY =
    windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY {
        fmtid: GUID::from_u128(0xb3f8fa53_0004_438e_9003_51a46e139bfc),
        pid: 2,
    };
const PKEY_ENDPOINT_DRIVER_MATCHING_ID: windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY =
    windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY {
        fmtid: GUID::from_u128(0xa8b865dd_2e3d_4094_ad97_e593a70c75d6),
        pid: 8,
    };
const HANDSFREE_ENUMERATOR: &str = "BTHHFENUM\\";
const HANDSFREE_DISABLED_FILE: &str = "handsfree_disabled.json";

fn bluetooth_handsfree_block_enabled() -> bool {
    STATE.lock().unwrap().advanced.block_bluetooth_handsfree
}

/// Called on startup and on every endpoint change in the tray process.
fn enforce_bluetooth_handsfree_block() {
    if !bluetooth_handsfree_block_enabled() {
        return;
    }
    if let Err(err) = disable_bluetooth_handsfree_endpoints() {
        eprintln!("failed to disable Bluetooth hands-free endpoints: {err:?}");
    }
}

/// Called when the setting is toggled.
fn sync_bluetooth_handsfree_setting(enabled: bool) {
    let result = if enabled {
        disable_bluetooth_handsfree_endpoints()
    } else {
        restore_bluetooth_handsfree_endpoints()
    };
    if let Err(err) = result {
        eprintln!("failed to sync Bluetooth hands-free setting: {err:?}");
    }
}

fn disable_bluetooth_handsfree_endpoints() -> Result<()> {
    let mut disabled = load_handsfree_disabled_ids();
    let mut remembered = false;
    let mut first_error = None;
    for (id, state) in bluetooth_handsfree_endpoints()? {
        // Only live endpoints. A disabled one is done; touching a not-present
        // one fires another state change without a visible result.
        if state != DEVICE_STATE_ACTIVE
            && state != windows::Win32::Media::Audio::DEVICE_STATE_UNPLUGGED
        {
            continue;
        }
        match set_endpoint_visibility(&id, false) {
            Ok(()) => {
                if !disabled.contains(&id) {
                    disabled.push(id);
                    remembered = true;
                }
            }
            Err(err) => {
                first_error.get_or_insert(err);
            }
        }
    }
    if remembered {
        save_handsfree_disabled_ids(&disabled)?;
    }
    match first_error {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

fn restore_bluetooth_handsfree_endpoints() -> Result<()> {
    let disabled = load_handsfree_disabled_ids();
    if disabled.is_empty() {
        return Ok(());
    }
    let mut kept = Vec::new();
    for id in disabled {
        if let Err(err) = set_endpoint_visibility(&id, true) {
            // Keep it so the next "off" can retry, unless the endpoint is gone.
            if endpoint_exists(&id) {
                eprintln!("failed to re-enable hands-free endpoint {id}: {err:?}");
                kept.push(id);
            }
        }
    }
    save_handsfree_disabled_ids(&kept)
}

fn bluetooth_handsfree_endpoints() -> Result<Vec<(String, DEVICE_STATE)>> {
    let mut endpoints = Vec::new();
    unsafe {
        let enumerator = audio_device_enumerator()?;
        let collection = enumerator
            .EnumAudioEndpoints(
                windows::Win32::Media::Audio::eAll,
                DEVICE_STATE(windows::Win32::Media::Audio::DEVICE_STATEMASK_ALL),
            )
            .context("enumerate audio endpoints")?;
        for index in 0..collection.GetCount()? {
            let Ok(device) = collection.Item(index) else {
                continue;
            };
            if !is_bluetooth_handsfree_endpoint(&device) {
                continue;
            }
            let (Ok(id), Ok(state)) = (endpoint_device_id(&device), device.GetState()) else {
                continue;
            };
            endpoints.push((id, state));
        }
    }
    Ok(endpoints)
}

unsafe fn is_bluetooth_handsfree_endpoint(device: &IMMDevice) -> bool {
    let Ok(store) = (unsafe { device.OpenPropertyStore(STGM_READ) }) else {
        return false;
    };
    [&PKEY_ENDPOINT_DEVNODE_PATH, &PKEY_ENDPOINT_DRIVER_MATCHING_ID]
        .into_iter()
        .filter_map(|key| endpoint_property_string(&store, key))
        .any(|value| value.to_ascii_uppercase().contains(HANDSFREE_ENUMERATOR))
}

fn endpoint_exists(id: &str) -> bool {
    let id = wide(id);
    audio_device_enumerator()
        .and_then(|enumerator| unsafe { Ok(enumerator.GetDevice(PCWSTR(id.as_ptr()))?) })
        .is_ok()
}

fn set_endpoint_visibility(device_id: &str, visible: bool) -> Result<()> {
    let device_id = wide(device_id);
    unsafe {
        let policy: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
            .context("create policy config client")?;
        (Interface::vtable(&policy).SetEndpointVisibility)(
            Interface::as_raw(&policy),
            PCWSTR(device_id.as_ptr()),
            i32::from(visible),
        )
        .ok()
        .context("set audio endpoint visibility")?;
    }
    Ok(())
}

fn handsfree_disabled_path() -> Result<PathBuf> {
    Ok(app_config_dir()?.join(HANDSFREE_DISABLED_FILE))
}

fn load_handsfree_disabled_ids() -> Vec<String> {
    handsfree_disabled_path()
        .ok()
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_handsfree_disabled_ids(ids: &[String]) -> Result<()> {
    let path = handsfree_disabled_path()?;
    if ids.is_empty() {
        if path.exists() {
            fs::remove_file(&path).context("remove hands-free endpoint list")?;
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create config directory")?;
    }
    fs::write(&path, serde_json::to_vec_pretty(ids)?).context("write hands-free endpoint list")
}
