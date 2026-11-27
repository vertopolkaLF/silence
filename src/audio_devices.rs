//! Rich Windows Core Audio device enumeration, peak metering, role switching,
//! test tone playback, and device property inspection/editing (replacement for
//! the classic Windows Sound Control Panel `mmsys.cpl`).

use anyhow::{Context, Result};
use std::{
    collections::{HashMap, HashSet},
    f32::consts::TAU,
    ffi::c_void,
    mem::size_of,
    ptr::null_mut,
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};
use windows::{
    Media::Audio::{
        SetDefaultSpatialAudioFormatStatus, SpatialAudioDeviceConfiguration,
        SpatialAudioFormatSubtype,
    },
    Win32::{
        Foundation::{E_ACCESSDENIED, S_OK},
        Media::Audio::{
            AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_SHAREMODE_SHARED, DEVICE_STATE,
            DEVICE_STATE_ACTIVE, DEVICE_STATE_DISABLED, DEVICE_STATE_NOTPRESENT,
            DEVICE_STATE_UNPLUGGED, EDataFlow, ERole,
            Endpoints::{IAudioEndpointVolume, IAudioMeterInformation},
            IAudioAutoGainControl, IAudioClient, IAudioRenderClient, IAudioVolumeLevel,
            IDeviceTopology, IMMDevice, IPart, PKEY_AudioEndpoint_FormFactor, WAVEFORMATEX,
            WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0, eCapture, eCommunications, eConsole,
            eMultimedia, eRender,
        },
        System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ, STGM_READWRITE},
        UI::Shell::PropertiesSystem::{IPropertyStore, PROPERTYKEY},
    },
    core::{GUID, HSTRING, Interface, PCWSTR, PROPVARIANT},
};

const WAVE_FORMAT_IEEE_FLOAT_TAG: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE_TAG: u16 = 0xFFFE;

const KSDATAFORMAT_SUBTYPE_PCM: GUID =
    GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: GUID =
    GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

// Exclusive mode settings on an endpoint's property store.
const PKEY_ALLOW_EXCLUSIVE: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xb3f8fa53_0004_438e_9003_51a46e139bfc),
    pid: 3,
};
const PKEY_EXCLUSIVE_PRIORITY: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xb3f8fa53_0004_438e_9003_51a46e139bfc),
    pid: 4,
};

// Native Windows "Listen to this device" properties on capture endpoints.
const PKEY_LISTEN_TARGET: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x24dbb0fc_9311_4b3d_9cf0_18ff155639d4),
    pid: 0,
};
const PKEY_LISTEN_ENABLED: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x24dbb0fc_9311_4b3d_9cf0_18ff155639d4),
    pid: 1,
};

// Audio Enhancements (SysFx): 0 = enabled, 1 = disabled (stored in FxProperties).
const PKEY_AUDIO_ENDPOINT_DISABLE_SYSFX: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x1da5d803_d492_4edd_8c23_e0c0ffee7f0e),
    pid: 5,
};

// APO registration property keys in FxProperties (`PKEY_FX_*` / `PKEY_CompositeFX_*`).
// A device only actually supports Audio Enhancements when at least one APO CLSID is registered.
const FX_APO_FMTID: GUID = GUID::from_u128(0xd04e05a6_594b_4fb6_a80d_01af5eed7d1d);
const FX_APO_PIDS: &[u32] = &[1, 2, 3, 5, 6, 7, 13, 14, 15];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceState {
    Active,
    Disabled,
    Unplugged,
    NotPresent,
}

impl DeviceState {
    pub fn from_raw(raw: DEVICE_STATE) -> Self {
        if raw == DEVICE_STATE_ACTIVE {
            Self::Active
        } else if raw == DEVICE_STATE_DISABLED {
            Self::Disabled
        } else if raw == DEVICE_STATE_UNPLUGGED {
            Self::Unplugged
        } else if raw == DEVICE_STATE_NOTPRESENT {
            Self::NotPresent
        } else {
            Self::NotPresent
        }
    }

    pub fn is_active(self) -> bool {
        self == Self::Active
    }

    pub fn is_disabled(self) -> bool {
        self == Self::Disabled
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Disabled => "Disabled",
            Self::Unplugged => "Disconnected",
            Self::NotPresent => "Not present",
        }
    }

    fn sort_rank(self) -> u8 {
        match self {
            Self::Active => 0,
            Self::Disabled => 1,
            Self::Unplugged => 2,
            Self::NotPresent => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointFormFactorKind {
    Headphones,
    Headset,
    Speakers,
    Monitor,
    Microphone,
    LineLevel,
    Digital,
    Other,
}

impl EndpointFormFactorKind {
    fn from_raw(value: u32) -> Self {
        match value {
            1 => Self::Speakers,
            2 => Self::LineLevel,
            3 => Self::Headphones,
            4 => Self::Microphone,
            5 | 6 => Self::Headset,
            7 | 8 => Self::Digital,
            9 => Self::Monitor,
            _ => Self::Other,
        }
    }

    pub fn icon_name(self, input: bool) -> &'static str {
        match self {
            Self::Headphones | Self::Headset => "headphones-round-sound-linear",
            Self::Speakers => "speaker-minimalistic-linear",
            Self::Monitor => "monitor-linear",
            Self::Microphone => "microphone-3-linear",
            Self::LineLevel | Self::Digital => "plug-circle-linear",
            Self::Other => {
                if input {
                    "microphone-3-linear"
                } else {
                    "volume-loud-linear"
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceEntry {
    pub id: String,
    pub name: String,
    pub system_name: String,
    pub input: bool,
    pub state: DeviceState,
    pub is_default: bool,
    pub is_default_comm: bool,
    pub form_factor: EndpointFormFactorKind,
}

impl DeviceEntry {
    pub fn display_name(&self, mode: &str) -> String {
        crate::display_audio_device_name(&self.name, &self.system_name, mode)
    }

    /// Secondary caption under the device title: adapter/driver name (unless
    /// already shown by the display mode) and non-active status.
    pub fn subtitle(&self, mode: &str) -> Option<String> {
        let mut parts = Vec::new();
        let show_adapter = mode != crate::AUDIO_DEVICE_NAME_BOTH
            && mode != crate::AUDIO_DEVICE_NAME_SYSTEM
            && !self.system_name.is_empty()
            && !self.system_name.eq_ignore_ascii_case(&self.name);
        if show_adapter {
            parts.push(self.system_name.as_str());
        }
        if !self.state.is_active() {
            parts.push(self.state.label());
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" · "))
        }
    }

    pub fn icon_name(&self) -> &'static str {
        self.form_factor.icon_name(self.input)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioFormatOption {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub valid_bits: u16,
    pub channel_mask: u32,
    pub is_float: bool,
}

impl AudioFormatOption {
    pub fn id(&self) -> String {
        format!(
            "{}ch-{}b-{}hz{}",
            self.channels,
            self.valid_bits,
            self.sample_rate,
            if self.is_float { "-f" } else { "" }
        )
    }

    pub fn label(&self) -> String {
        let quality = match (self.valid_bits, self.sample_rate) {
            (16, 44100) => "CD Quality",
            (16, 48000) => "DVD Quality",
            (24, 48000) | (24, 96000) | (24, 192000) | (32, 48000) | (32, 96000)
            | (32, 192000) => "Studio Quality",
            _ => "",
        };
        if quality.is_empty() {
            format!("{} bit, {} Hz", self.valid_bits, self.sample_rate)
        } else {
            format!(
                "{} bit, {} Hz ({quality})",
                self.valid_bits, self.sample_rate
            )
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MicBoostInfo {
    pub min_db: f32,
    pub max_db: f32,
    pub step_db: f32,
    pub current_db: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialAudioOption {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceProperties {
    pub id: String,
    pub input: bool,
    pub active: bool,
    pub volume: f32,
    pub muted: bool,
    pub channel_count: u32,
    pub channel_volumes: Vec<f32>,
    pub supports_balance: bool,
    /// Stereo balance in `-1.0..=1.0` (`-1.0` = full left, `0.0` = center, `1.0` = full right).
    pub balance: f32,
    pub current_format: Option<AudioFormatOption>,
    pub supported_formats: Vec<AudioFormatOption>,
    pub allow_exclusive: Option<bool>,
    pub exclusive_priority: Option<bool>,
    pub enhancements_enabled: Option<bool>,
    pub spatial_supported: bool,
    pub spatial_current: String,
    pub spatial_options: Vec<SpatialAudioOption>,
    pub listen_enabled: Option<bool>,
    pub listen_target_id: String,
    pub mic_boost: Option<MicBoostInfo>,
    pub mic_agc: Option<bool>,
}

#[derive(Debug)]
pub enum ListenError {
    AccessDenied,
    Other(anyhow::Error),
}

pub fn list_devices(
    input: bool,
    include_disabled: bool,
    include_disconnected: bool,
) -> Result<Vec<DeviceEntry>> {
    let flow: EDataFlow = if input { eCapture } else { eRender };
    let fallback_name = if input { "Microphone" } else { "Speaker" };
    let mut mask = DEVICE_STATE_ACTIVE.0;
    if include_disabled {
        mask |= DEVICE_STATE_DISABLED.0;
    }
    if include_disconnected {
        mask |= DEVICE_STATE_UNPLUGGED.0;
    }

    unsafe {
        let enumerator = crate::audio_device_enumerator()?;
        let default_id = enumerator
            .GetDefaultAudioEndpoint(flow, eConsole)
            .ok()
            .and_then(|d| crate::endpoint_device_id(&d).ok());
        let default_comm_id = enumerator
            .GetDefaultAudioEndpoint(flow, eCommunications)
            .ok()
            .and_then(|d| crate::endpoint_device_id(&d).ok());

        let collection = enumerator
            .EnumAudioEndpoints(flow, DEVICE_STATE(mask))
            .context("enumerate audio endpoints")?;
        let count = collection.GetCount().context("count audio endpoints")?;
        let mut devices = Vec::with_capacity(count as usize);

        for index in 0..count {
            let Ok(device) = collection.Item(index) else {
                continue;
            };
            let Ok(id) = crate::endpoint_device_id(&device) else {
                continue;
            };
            let state = device
                .GetState()
                .map(DeviceState::from_raw)
                .unwrap_or(DeviceState::NotPresent);
            let (name, system_name) =
                crate::endpoint_device_display_names(&device, &id, fallback_name);
            let form_factor = read_form_factor(&device);
            let is_default = state.is_active() && default_id.as_deref() == Some(id.as_str());
            let is_default_comm =
                state.is_active() && default_comm_id.as_deref() == Some(id.as_str());
            devices.push(DeviceEntry {
                id,
                name,
                system_name,
                input,
                state,
                is_default,
                is_default_comm,
                form_factor,
            });
        }

        devices.sort_by(|a, b| {
            a.state
                .sort_rank()
                .cmp(&b.state.sort_rank())
                .then_with(|| b.is_default.cmp(&a.is_default))
                .then_with(|| b.is_default_comm.cmp(&a.is_default_comm))
                .then_with(|| {
                    a.name
                        .to_ascii_lowercase()
                        .cmp(&b.name.to_ascii_lowercase())
                })
                .then_with(|| a.id.cmp(&b.id))
        });

        Ok(devices)
    }
}

pub fn set_default_device(device_id: &str, split_comm: bool) -> Result<()> {
    if split_comm {
        set_default_roles(device_id, &[eConsole, eMultimedia])
    } else {
        set_default_roles(device_id, &[eConsole, eMultimedia, eCommunications])
    }
}

pub fn set_default_comm_device(device_id: &str) -> Result<()> {
    set_default_roles(device_id, &[eCommunications])
}

fn set_default_roles(device_id: &str, roles: &[ERole]) -> Result<()> {
    let device_id = crate::wide(device_id);
    unsafe {
        let policy: crate::IPolicyConfig =
            CoCreateInstance(&crate::CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
                .context("create policy config client")?;
        for &role in roles {
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

pub fn set_device_enabled(device_id: &str, enabled: bool) -> Result<()> {
    crate::set_endpoint_visibility(device_id, enabled)
}

/// Keeps lightweight silent WASAPI capture streams open for visible input
/// devices so `IAudioMeterInformation::GetPeakValue()` reports live levels
/// even when no other app is recording from the microphone.
/// Pass an empty slice to stop and release all capture streams immediately.
pub fn sync_capture_meter_streams(input_ids: &[String]) {
    static TX: OnceLock<flume::Sender<Vec<String>>> = OnceLock::new();
    static LAST: Mutex<Vec<String>> = Mutex::new(Vec::new());

    {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if *last == input_ids {
            return;
        }
        *last = input_ids.to_vec();
    }

    let tx = TX.get_or_init(|| {
        let (tx, rx) = flume::unbounded::<Vec<String>>();
        thread::spawn(move || capture_meter_worker(rx));
        tx
    });
    let _ = tx.send(input_ids.to_vec());
}

fn capture_meter_worker(rx: flume::Receiver<Vec<String>>) {
    let Ok(_com) = crate::ComApartment::join_mta() else {
        return;
    };
    let mut active: HashMap<String, IAudioClient> = HashMap::new();
    while let Ok(mut desired) = rx.recv() {
        // Coalesce rapid updates to the latest desired set.
        while let Ok(newer) = rx.try_recv() {
            desired = newer;
        }
        let wanted: HashSet<&str> = desired.iter().map(String::as_str).collect();
        active.retain(|id, client| {
            if wanted.contains(id.as_str()) {
                true
            } else {
                unsafe {
                    let _ = client.Stop();
                }
                false
            }
        });
        for id in desired {
            if active.contains_key(&id) {
                continue;
            }
            if let Ok(client) = start_silent_capture_stream(&id) {
                active.insert(id, client);
            }
        }
    }
    for (_, client) in active {
        unsafe {
            let _ = client.Stop();
        }
    }
}

fn start_silent_capture_stream(device_id: &str) -> Result<IAudioClient> {
    let id_w = crate::wide(device_id);
    unsafe {
        let enumerator = crate::audio_device_enumerator()?;
        let device = enumerator
            .GetDevice(PCWSTR(id_w.as_ptr()))
            .context("get capture device for meter")?;
        if device.GetState().ok() != Some(DEVICE_STATE_ACTIVE) {
            anyhow::bail!("capture device not active");
        }
        let client: IAudioClient = device
            .Activate(CLSCTX_ALL, None)
            .context("activate capture audio client")?;
        let mix_ptr = client.GetMixFormat().context("get capture mix format")?;
        let init_res = client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            0,
            1_000_000, // 100ms buffer
            0,
            mix_ptr,
            None,
        );
        CoTaskMemFree(Some(mix_ptr as *const c_void));
        init_res.context("initialize capture meter stream")?;
        client.Start().context("start capture meter stream")?;
        Ok(client)
    }
}

/// Samples current peak audio level (`0.0..=1.0`) for each requested active
/// device ID. Safe to call from a background polling task.
pub fn sample_peak_levels(ids: &[String]) -> HashMap<String, f32> {
    let mut peaks = HashMap::with_capacity(ids.len());
    if ids.is_empty() {
        return peaks;
    }
    let Ok(_com) = crate::ComApartment::join_mta() else {
        return peaks;
    };
    unsafe {
        let Ok(enumerator) = crate::audio_device_enumerator() else {
            return peaks;
        };
        for id in ids {
            let id_w = crate::wide(id);
            let Ok(device) = enumerator.GetDevice(PCWSTR(id_w.as_ptr())) else {
                continue;
            };
            if device.GetState().ok() != Some(DEVICE_STATE_ACTIVE) {
                continue;
            }
            let Ok(meter) = device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None) else {
                continue;
            };
            if let Ok(peak) = meter.GetPeakValue() {
                peaks.insert(id.clone(), peak.clamp(0.0, 1.0));
            }
        }
    }
    peaks
}

pub fn read_device_properties(device_id: &str, input: bool) -> Result<DeviceProperties> {
    let id_w = crate::wide(device_id);
    unsafe {
        let enumerator = crate::audio_device_enumerator()?;
        let device = enumerator
            .GetDevice(PCWSTR(id_w.as_ptr()))
            .context("get audio device")?;
        let active = device.GetState().ok() == Some(DEVICE_STATE_ACTIVE);

        let (volume, muted, channel_count, channel_volumes, supports_balance, balance) = if active {
            match device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
                Ok(ep_vol) => {
                    let master = ep_vol.GetMasterVolumeLevelScalar().unwrap_or(1.0);
                    let muted = ep_vol.GetMute().map(|b| b.as_bool()).unwrap_or(false);
                    let count = ep_vol.GetChannelCount().unwrap_or(0);
                    let mut channels = Vec::with_capacity(count as usize);
                    for ch in 0..count {
                        channels.push(ep_vol.GetChannelVolumeLevelScalar(ch).unwrap_or(master));
                    }
                    let supports_bal = !input && endpoint_supports_balance(&device, &ep_vol, count);
                    let balance = compute_stereo_balance(&channels);
                    (master, muted, count, channels, supports_bal, balance)
                }
                Err(_) => (1.0, false, 0, Vec::new(), false, 0.0),
            }
        } else {
            (1.0, false, 0, Vec::new(), false, 0.0)
        };

        let current_format = read_device_format(device_id);
        let supported_formats = if active {
            probe_supported_formats(&device, current_format.as_ref())
        } else {
            current_format.clone().into_iter().collect()
        };

        let store = device.OpenPropertyStore(STGM_READ).ok();
        let allow_exclusive = store
            .as_ref()
            .and_then(|s| read_prop_u32(s, &PKEY_ALLOW_EXCLUSIVE))
            .map(|v| v != 0)
            .or(Some(true));
        let exclusive_priority = store
            .as_ref()
            .and_then(|s| read_prop_u32(s, &PKEY_EXCLUSIVE_PRIORITY))
            .map(|v| v != 0)
            .or(Some(true));

        let (listen_enabled, listen_target_id) = if input {
            let enabled = store
                .as_ref()
                .and_then(|s| read_prop_bool(s, &PKEY_LISTEN_ENABLED))
                .or(Some(false));
            let target = store
                .as_ref()
                .and_then(|s| crate::endpoint_property_string(s, &PKEY_LISTEN_TARGET))
                .unwrap_or_default();
            (enabled, target)
        } else {
            (None, String::new())
        };

        let (mic_boost, mic_agc) = if input && active {
            read_mic_topology_controls(&device)
        } else {
            (None, None)
        };

        let enhancements_enabled = if active {
            read_device_enhancements(device_id)
        } else {
            None
        };

        let (spatial_supported, spatial_current, spatial_options) = if !input && active {
            read_device_spatial_audio(device_id)
        } else {
            (false, String::new(), Vec::new())
        };

        Ok(DeviceProperties {
            id: device_id.to_string(),
            input,
            active,
            volume,
            muted,
            channel_count,
            channel_volumes,
            supports_balance,
            balance,
            current_format,
            supported_formats,
            allow_exclusive,
            exclusive_priority,
            enhancements_enabled,
            spatial_supported,
            spatial_current,
            spatial_options,
            listen_enabled,
            listen_target_id,
            mic_boost,
            mic_agc,
        })
    }
}

pub fn set_device_volume(device_id: &str, scalar: f32) -> Result<()> {
    let ep_vol = activate_endpoint_volume(device_id)?;
    let clamped = scalar.clamp(0.0, 1.0);
    unsafe {
        ep_vol
            .SetMasterVolumeLevelScalar(clamped, null_mut())
            .context("set device volume")?;
    }
    Ok(())
}

pub fn set_device_mute(device_id: &str, muted: bool) -> Result<()> {
    let ep_vol = activate_endpoint_volume(device_id)?;
    unsafe {
        ep_vol
            .SetMute(muted, null_mut())
            .context("set device mute")?;
    }
    Ok(())
}

pub fn set_device_balance(device_id: &str, balance: f32) -> Result<()> {
    let ep_vol = activate_endpoint_volume(device_id)?;
    let balance = balance.clamp(-1.0, 1.0);
    unsafe {
        let count = ep_vol.GetChannelCount().context("get channel count")?;
        if count != 2 {
            return Ok(());
        }
        let l_cur = ep_vol.GetChannelVolumeLevelScalar(0).unwrap_or(1.0);
        let r_cur = ep_vol.GetChannelVolumeLevelScalar(1).unwrap_or(1.0);
        let peak = l_cur.max(r_cur).max(0.01);
        let (left, right) = if balance < 0.0 {
            (peak, peak * (1.0 + balance))
        } else {
            (peak * (1.0 - balance), peak)
        };
        ep_vol
            .SetChannelVolumeLevelScalar(0, left.clamp(0.0, 1.0), null_mut())
            .context("set left channel volume")?;
        ep_vol
            .SetChannelVolumeLevelScalar(1, right.clamp(0.0, 1.0), null_mut())
            .context("set right channel volume")?;
    }
    Ok(())
}

pub fn set_device_format(device_id: &str, format: &AudioFormatOption) -> Result<()> {
    let wf = build_wave_format_extensible(format);
    let id_w = crate::wide(device_id);
    unsafe {
        let policy: crate::IPolicyConfig =
            CoCreateInstance(&crate::CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
                .context("create policy config client")?;
        let ptr = &wf as *const WAVEFORMATEXTENSIBLE as *const c_void;
        (Interface::vtable(&policy).SetDeviceFormat)(
            Interface::as_raw(&policy),
            PCWSTR(id_w.as_ptr()),
            ptr,
            ptr,
        )
        .ok()
        .context("set device audio format")?;
    }
    Ok(())
}

pub fn set_device_exclusive(device_id: &str, allow: bool, priority: bool) -> Result<()> {
    let id_w = crate::wide(device_id);
    unsafe {
        let enumerator = crate::audio_device_enumerator()?;
        let device = enumerator
            .GetDevice(PCWSTR(id_w.as_ptr()))
            .context("get audio device")?;
        let store = device
            .OpenPropertyStore(STGM_READWRITE)
            .context("open property store for exclusive mode")?;
        let allow_pv = PROPVARIANT::from(u32::from(allow));
        let prio_pv = PROPVARIANT::from(u32::from(priority));
        store
            .SetValue(&PKEY_ALLOW_EXCLUSIVE, &allow_pv)
            .context("set allow exclusive mode")?;
        store
            .SetValue(&PKEY_EXCLUSIVE_PRIORITY, &prio_pv)
            .context("set exclusive priority")?;
        store.Commit().context("commit exclusive mode")?;
    }
    Ok(())
}

pub fn set_device_listen(
    device_id: &str,
    enabled: bool,
    target_id: &str,
) -> std::result::Result<(), ListenError> {
    let id_w = crate::wide(device_id);
    unsafe {
        let enumerator = crate::audio_device_enumerator().map_err(ListenError::Other)?;
        let device = enumerator
            .GetDevice(PCWSTR(id_w.as_ptr()))
            .map_err(|e| ListenError::Other(e.into()))?;
        let store = match device.OpenPropertyStore(STGM_READWRITE) {
            Ok(s) => s,
            Err(e) if e.code() == E_ACCESSDENIED => return Err(ListenError::AccessDenied),
            Err(e) => return Err(ListenError::Other(e.into())),
        };
        let enabled_pv = PROPVARIANT::from(enabled);
        if let Err(e) = store.SetValue(&PKEY_LISTEN_ENABLED, &enabled_pv) {
            if e.code() == E_ACCESSDENIED {
                return Err(ListenError::AccessDenied);
            }
            return Err(ListenError::Other(e.into()));
        }
        let target_pv = if target_id.is_empty() {
            PROPVARIANT::new()
        } else {
            crate::audio_device_name_propvariant(target_id).map_err(ListenError::Other)?
        };
        if let Err(e) = store.SetValue(&PKEY_LISTEN_TARGET, &target_pv) {
            if e.code() == E_ACCESSDENIED {
                return Err(ListenError::AccessDenied);
            }
            return Err(ListenError::Other(e.into()));
        }
        if let Err(e) = store.Commit() {
            if e.code() == E_ACCESSDENIED {
                return Err(ListenError::AccessDenied);
            }
            return Err(ListenError::Other(e.into()));
        }
    }
    Ok(())
}

pub fn set_device_mic_boost(device_id: &str, db: f32) -> Result<()> {
    let id_w = crate::wide(device_id);
    unsafe {
        let enumerator = crate::audio_device_enumerator()?;
        let device = enumerator
            .GetDevice(PCWSTR(id_w.as_ptr()))
            .context("get audio device")?;
        let Some(boost) = find_mic_boost_control(&device) else {
            return Ok(());
        };
        let count = boost.GetChannelCount().unwrap_or(1);
        for ch in 0..count {
            let _ = boost.SetLevel(ch, db, None);
        }
    }
    Ok(())
}

pub fn set_device_mic_agc(device_id: &str, enabled: bool) -> Result<()> {
    let id_w = crate::wide(device_id);
    unsafe {
        let enumerator = crate::audio_device_enumerator()?;
        let device = enumerator
            .GetDevice(PCWSTR(id_w.as_ptr()))
            .context("get audio device")?;
        let Some(agc) = find_mic_agc_control(&device) else {
            return Ok(());
        };
        agc.SetEnabled(enabled, None).context("set mic AGC")?;
    }
    Ok(())
}

fn read_device_enhancements(device_id: &str) -> Option<bool> {
    let id_w = crate::wide(device_id);
    unsafe {
        let policy: crate::IPolicyConfig =
            CoCreateInstance(&crate::CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL).ok()?;
        let has_apo = FX_APO_PIDS.iter().any(|&pid| {
            let key = PROPERTYKEY {
                fmtid: FX_APO_FMTID,
                pid,
            };
            let mut pv = PROPVARIANT::new();
            let hr = (Interface::vtable(&policy).GetPropertyValue)(
                Interface::as_raw(&policy),
                PCWSTR(id_w.as_ptr()),
                1, // bFxStore = TRUE
                &key as *const PROPERTYKEY as *const c_void,
                &mut pv,
            );
            hr.is_ok() && !pv.is_empty()
        });
        if !has_apo {
            return None;
        }
        let mut pv = PROPVARIANT::new();
        let hr = (Interface::vtable(&policy).GetPropertyValue)(
            Interface::as_raw(&policy),
            PCWSTR(id_w.as_ptr()),
            1, // bFxStore = TRUE
            &PKEY_AUDIO_ENDPOINT_DISABLE_SYSFX as *const PROPERTYKEY as *const c_void,
            &mut pv,
        );
        if hr.is_err() {
            return None;
        }
        if pv.is_empty() {
            return Some(true);
        }
        let disabled = u32::try_from(&pv).unwrap_or(0) != 0;
        Some(!disabled)
    }
}

pub fn set_device_enhancements(device_id: &str, enabled: bool) -> Result<()> {
    let id_w = crate::wide(device_id);
    let pv = PROPVARIANT::from(u32::from(!enabled));
    unsafe {
        let policy: crate::IPolicyConfig =
            CoCreateInstance(&crate::CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
                .context("create policy config client")?;
        (Interface::vtable(&policy).SetPropertyValue)(
            Interface::as_raw(&policy),
            PCWSTR(id_w.as_ptr()),
            1, // bFxStore = TRUE
            &PKEY_AUDIO_ENDPOINT_DISABLE_SYSFX as *const PROPERTYKEY as *const c_void,
            &pv,
        )
        .ok()
        .context("set audio enhancements")?;
    }
    Ok(())
}

fn winrt_render_device_id(mmdevice_id: &str) -> HSTRING {
    HSTRING::from(format!(
        r"\\?\SWD#MMDEVAPI#{mmdevice_id}#{{e6327cad-dcec-4949-ae8a-991e976a79d2}}"
    ))
}

fn known_spatial_subtypes() -> Vec<(String, &'static str)> {
    let candidates: [(windows::core::Result<HSTRING>, &'static str); 7] = [
        (
            SpatialAudioFormatSubtype::WindowsSonic(),
            "Windows Sonic for Headphones",
        ),
        (
            SpatialAudioFormatSubtype::DolbyAtmosForHeadphones(),
            "Dolby Atmos for Headphones",
        ),
        (
            SpatialAudioFormatSubtype::DolbyAtmosForHomeTheater(),
            "Dolby Atmos for Home Theater",
        ),
        (
            SpatialAudioFormatSubtype::DolbyAtmosForSpeakers(),
            "Dolby Atmos for built-in speakers",
        ),
        (
            SpatialAudioFormatSubtype::DTSHeadphoneX(),
            "DTS Headphone:X",
        ),
        (SpatialAudioFormatSubtype::DTSXUltra(), "DTS:X Ultra"),
        (
            SpatialAudioFormatSubtype::DTSXForHomeTheater(),
            "DTS:X for Home Theater",
        ),
    ];
    candidates
        .into_iter()
        .filter_map(|(res, label)| {
            let id = res.ok()?.to_string();
            if id.is_empty() { None } else { Some((id, label)) }
        })
        .collect()
}

const SPATIAL_OFF_GUID: &str = "{00000000-0000-0000-0000-000000000000}";

fn is_spatial_off(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case(SPATIAL_OFF_GUID)
        || trimmed.eq_ignore_ascii_case("00000000-0000-0000-0000-000000000000")
}

fn read_device_spatial_audio(device_id: &str) -> (bool, String, Vec<SpatialAudioOption>) {
    let winrt_id = winrt_render_device_id(device_id);
    let Ok(cfg) = SpatialAudioDeviceConfiguration::GetForDeviceId(&winrt_id) else {
        return (false, String::new(), Vec::new());
    };
    let supported = cfg.IsSpatialAudioSupported().unwrap_or(false);
    if !supported {
        return (false, String::new(), Vec::new());
    }
    let active_raw = cfg
        .ActiveSpatialAudioFormat()
        .ok()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let active = if is_spatial_off(&active_raw) {
        String::new()
    } else {
        active_raw
    };

    let sonic_id = SpatialAudioFormatSubtype::WindowsSonic()
        .ok()
        .map(|s| s.to_string())
        .unwrap_or_default();

    let mut options = vec![SpatialAudioOption {
        id: String::new(),
        label: "Off".to_string(),
    }];
    for (subtype_id, label) in known_spatial_subtypes() {
        let is_active = !active.is_empty() && active.eq_ignore_ascii_case(&subtype_id);
        let is_sonic = !sonic_id.is_empty() && subtype_id.eq_ignore_ascii_case(&sonic_id);
        if is_active || (is_sonic && cfg.IsSpatialAudioFormatSupported(&HSTRING::from(&subtype_id)).unwrap_or(false)) {
            options.push(SpatialAudioOption {
                id: subtype_id,
                label: label.to_string(),
            });
        }
    }
    if !active.is_empty()
        && !options
            .iter()
            .any(|o| o.id.eq_ignore_ascii_case(&active))
    {
        options.push(SpatialAudioOption {
            id: active.clone(),
            label: "Active".to_string(),
        });
    }
    let current = options
        .iter()
        .find(|o| o.id.eq_ignore_ascii_case(&active))
        .map(|o| o.id.clone())
        .unwrap_or_default();
    (true, current, options)
}

pub fn set_device_spatial_audio(
    device_id: &str,
    subtype: &str,
) -> std::result::Result<(), String> {
    let winrt_id = winrt_render_device_id(device_id);
    let cfg = SpatialAudioDeviceConfiguration::GetForDeviceId(&winrt_id)
        .map_err(|_| "Spatial audio is unavailable for this device.".to_string())?;
    let wire_subtype = if is_spatial_off(subtype) {
        SPATIAL_OFF_GUID
    } else {
        subtype
    };
    let op = cfg
        .SetDefaultSpatialAudioFormatAsync(&HSTRING::from(wire_subtype))
        .map_err(|_| "Windows rejected the spatial audio change.".to_string())?;
    let result = op
        .get()
        .map_err(|_| "Windows rejected the spatial audio change.".to_string())?;
    match result.Status().unwrap_or(SetDefaultSpatialAudioFormatStatus::UnknownError) {
        SetDefaultSpatialAudioFormatStatus::Succeeded => Ok(()),
        SetDefaultSpatialAudioFormatStatus::AccessDenied => Err(
            "Windows denied permission to change Spatial sound from this app. Use Windows properties."
                .to_string(),
        ),
        SetDefaultSpatialAudioFormatStatus::LicenseExpired => {
            Err("The license for this spatial audio format has expired.".to_string())
        }
        SetDefaultSpatialAudioFormatStatus::LicenseNotValidForAudioEndpoint => {
            Err("This spatial audio license is not valid for this device.".to_string())
        }
        SetDefaultSpatialAudioFormatStatus::NotSupportedOnAudioEndpoint => {
            Err("This spatial audio format is not supported on this device.".to_string())
        }
        _ => Err("Could not switch spatial audio format on this device.".to_string()),
    }
}

static TEST_TONE_LOCK: Mutex<()> = Mutex::new(());

/// Plays a gentle stereo L -> R test chime directly to `device_id` via WASAPI
/// shared mode (works even when the device is not the Windows default).
pub fn play_test_tone(device_id: &str) -> Result<()> {
    let id = device_id.to_string();
    thread::spawn(move || {
        let Ok(_guard) = TEST_TONE_LOCK.try_lock() else {
            return;
        };
        let _ = play_test_tone_blocking(&id);
    });
    Ok(())
}

fn play_test_tone_blocking(device_id: &str) -> Result<()> {
    let _com = crate::ComApartment::join_mta()?;
    unsafe {
        let enumerator = crate::audio_device_enumerator()?;
        let id_w = crate::wide(device_id);
        let device = enumerator
            .GetDevice(PCWSTR(id_w.as_ptr()))
            .context("get test output device")?;
        let client: IAudioClient = device
            .Activate(CLSCTX_ALL, None)
            .context("activate audio client for test tone")?;
        let mix_ptr = client.GetMixFormat().context("get mix format")?;
        let mix = *mix_ptr;
        let sample_rate = mix.nSamplesPerSec.max(8000);
        let channels = mix.nChannels.max(1) as usize;
        let bits = mix.wBitsPerSample;
        let is_float = is_wave_format_float(mix_ptr);

        // 200ms buffer
        let hns_duration: i64 = 2_000_000;
        let init_res = client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            0,
            hns_duration,
            0,
            mix_ptr,
            None,
        );
        CoTaskMemFree(Some(mix_ptr as *const c_void));
        init_res.context("initialize audio client for test tone")?;

        let buffer_frames = client.GetBufferSize().context("get buffer size")?;
        let render: IAudioRenderClient = client
            .GetService()
            .context("get render client for test tone")?;

        // Two gentle bell chimes: 0.00..0.34s left (523.25 Hz C5), 0.42..0.76s right (659.25 Hz E5)
        let total_secs = 0.85_f32;
        let total_frames = (sample_rate as f32 * total_secs) as u64;
        let mut frame_cursor: u64 = 0;

        client.Start().context("start test tone stream")?;

        while frame_cursor < total_frames {
            let padding = client.GetCurrentPadding().unwrap_or(0);
            let available = buffer_frames.saturating_sub(padding);
            if available == 0 {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            let remaining = (total_frames - frame_cursor) as u32;
            let to_write = available.min(remaining);
            let Ok(data_ptr) = render.GetBuffer(to_write) else {
                break;
            };

            for i in 0..to_write {
                let t = (frame_cursor + u64::from(i)) as f32 / sample_rate as f32;
                let (left, right) = chime_sample(t);
                write_frame(data_ptr, i as usize, channels, bits, is_float, left, right);
            }

            let _ = render.ReleaseBuffer(to_write, 0);
            frame_cursor += u64::from(to_write);
        }

        // Let the buffer drain before stopping.
        thread::sleep(Duration::from_millis(220));
        let _ = client.Stop();
    }
    Ok(())
}

fn chime_sample(t: f32) -> (f32, f32) {
    let tone = |local_t: f32, freq: f32| -> f32 {
        if local_t < 0.0 || local_t > 0.34 {
            return 0.0;
        }
        let attack = (local_t / 0.012).min(1.0);
        let decay = (-local_t * 9.5).exp();
        let env = attack * decay * 0.24;
        let fundamental = (TAU * freq * local_t).sin();
        let overtone = 0.22 * (TAU * freq * 2.0 * local_t).sin();
        env * (fundamental + overtone)
    };
    (tone(t, 523.25), tone(t - 0.38, 659.25))
}

unsafe fn write_frame(
    buf: *mut u8,
    frame_idx: usize,
    channels: usize,
    bits: u16,
    is_float: bool,
    left: f32,
    right: f32,
) {
    let mono = (left + right) * 0.5;
    for ch in 0..channels {
        let sample = if channels == 1 {
            mono
        } else if ch == 0 {
            left
        } else if ch == 1 {
            right
        } else {
            0.0
        };
        let sample = sample.clamp(-1.0, 1.0);
        if is_float && bits == 32 {
            let ptr = unsafe { (buf as *mut f32).add(frame_idx * channels + ch) };
            unsafe { *ptr = sample };
        } else if bits == 16 {
            let ptr = unsafe { (buf as *mut i16).add(frame_idx * channels + ch) };
            unsafe { *ptr = (sample * i16::MAX as f32) as i16 };
        } else if bits == 32 {
            let ptr = unsafe { (buf as *mut i32).add(frame_idx * channels + ch) };
            unsafe { *ptr = (sample * i32::MAX as f32) as i32 };
        }
    }
}

fn compute_stereo_balance(channels: &[f32]) -> f32 {
    if channels.len() != 2 {
        return 0.0;
    }
    let l = channels[0].clamp(0.0, 1.0);
    let r = channels[1].clamp(0.0, 1.0);
    let peak = l.max(r);
    if peak < 0.001 {
        return 0.0;
    }
    if (l - r).abs() < 0.005 {
        return 0.0;
    }
    if l > r {
        -(1.0 - r / l).clamp(0.0, 1.0)
    } else {
        (1.0 - l / r).clamp(0.0, 1.0)
    }
}

fn activate_endpoint_volume(device_id: &str) -> Result<IAudioEndpointVolume> {
    let id_w = crate::wide(device_id);
    unsafe {
        let enumerator = crate::audio_device_enumerator()?;
        let device = enumerator
            .GetDevice(PCWSTR(id_w.as_ptr()))
            .context("get audio device")?;
        device
            .Activate(CLSCTX_ALL, None)
            .context("activate endpoint volume")
    }
}

unsafe fn read_form_factor(device: &IMMDevice) -> EndpointFormFactorKind {
    let Ok(store) = (unsafe { device.OpenPropertyStore(STGM_READ) }) else {
        return EndpointFormFactorKind::Other;
    };
    read_prop_u32(&store, &PKEY_AudioEndpoint_FormFactor)
        .map(EndpointFormFactorKind::from_raw)
        .unwrap_or(EndpointFormFactorKind::Other)
}

fn read_prop_u32(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<u32> {
    let pv = unsafe { store.GetValue(key).ok()? };
    if pv.is_empty() {
        return None;
    }
    u32::try_from(&pv).ok()
}

fn read_prop_bool(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<bool> {
    let pv = unsafe { store.GetValue(key).ok()? };
    if pv.is_empty() {
        return None;
    }
    bool::try_from(&pv).ok()
}

unsafe fn is_wave_format_float(ptr: *const WAVEFORMATEX) -> bool {
    let wf = unsafe { *ptr };
    if wf.wFormatTag == WAVE_FORMAT_IEEE_FLOAT_TAG {
        return true;
    }
    if wf.wFormatTag == WAVE_FORMAT_EXTENSIBLE_TAG && wf.cbSize >= 22 {
        let ext = unsafe { *(ptr as *const WAVEFORMATEXTENSIBLE) };
        let sub = ext.SubFormat;
        return sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
    }
    false
}

fn read_device_format(device_id: &str) -> Option<AudioFormatOption> {
    let id_w = crate::wide(device_id);
    unsafe {
        let policy: crate::IPolicyConfig =
            CoCreateInstance(&crate::CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL).ok()?;
        let mut raw_ptr: *mut c_void = null_mut();
        let hr = (Interface::vtable(&policy).GetDeviceFormat)(
            Interface::as_raw(&policy),
            PCWSTR(id_w.as_ptr()),
            0,
            &mut raw_ptr,
        );
        if hr.is_err() || raw_ptr.is_null() {
            return None;
        }
        let parsed = parse_wave_format(raw_ptr as *const WAVEFORMATEX);
        CoTaskMemFree(Some(raw_ptr));
        Some(parsed)
    }
}

unsafe fn parse_wave_format(ptr: *const WAVEFORMATEX) -> AudioFormatOption {
    let wf = unsafe { *ptr };
    let channels = wf.nChannels;
    let sample_rate = wf.nSamplesPerSec;
    let bits_per_sample = wf.wBitsPerSample;
    if wf.wFormatTag == WAVE_FORMAT_EXTENSIBLE_TAG && wf.cbSize >= 22 {
        let ext = unsafe { *(ptr as *const WAVEFORMATEXTENSIBLE) };
        let valid_bits = unsafe { ext.Samples.wValidBitsPerSample };
        let valid_bits = if valid_bits == 0 {
            bits_per_sample
        } else {
            valid_bits
        };
        let channel_mask = ext.dwChannelMask;
        let sub = ext.SubFormat;
        let is_float = sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
        AudioFormatOption {
            channels,
            sample_rate,
            bits_per_sample,
            valid_bits,
            channel_mask,
            is_float,
        }
    } else {
        AudioFormatOption {
            channels,
            sample_rate,
            bits_per_sample,
            valid_bits: bits_per_sample,
            channel_mask: default_channel_mask(channels),
            is_float: wf.wFormatTag == WAVE_FORMAT_IEEE_FLOAT_TAG,
        }
    }
}

fn default_channel_mask(channels: u16) -> u32 {
    match channels {
        1 => 0x4,   // SPEAKER_FRONT_CENTER
        2 => 0x3,   // SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT
        6 => 0x3F,  // 5.1
        8 => 0x63F, // 7.1
        _ => 0,
    }
}

fn build_wave_format_extensible(opt: &AudioFormatOption) -> WAVEFORMATEXTENSIBLE {
    let container_bits = if opt.bits_per_sample >= opt.valid_bits && opt.bits_per_sample != 0 {
        opt.bits_per_sample
    } else if opt.valid_bits == 24 {
        24
    } else {
        opt.valid_bits
    };
    let block_align = opt.channels * (container_bits / 8);
    let avg_bytes = opt.sample_rate * u32::from(block_align);
    WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_EXTENSIBLE_TAG,
            nChannels: opt.channels,
            nSamplesPerSec: opt.sample_rate,
            nAvgBytesPerSec: avg_bytes,
            nBlockAlign: block_align,
            wBitsPerSample: container_bits,
            cbSize: (size_of::<WAVEFORMATEXTENSIBLE>() - size_of::<WAVEFORMATEX>()) as u16,
        },
        Samples: WAVEFORMATEXTENSIBLE_0 {
            wValidBitsPerSample: opt.valid_bits,
        },
        dwChannelMask: if opt.channel_mask != 0 {
            opt.channel_mask
        } else {
            default_channel_mask(opt.channels)
        },
        SubFormat: if opt.is_float {
            KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
        } else {
            KSDATAFORMAT_SUBTYPE_PCM
        },
    }
}

unsafe fn probe_supported_formats(
    device: &IMMDevice,
    current: Option<&AudioFormatOption>,
) -> Vec<AudioFormatOption> {
    let Ok(client) = (unsafe { device.Activate::<IAudioClient>(CLSCTX_ALL, None) }) else {
        return current.cloned().into_iter().collect();
    };
    let (channels, channel_mask) = current
        .map(|f| (f.channels, f.channel_mask))
        .unwrap_or((2, 0x3));

    const BIT_DEPTHS: &[(u16, u16)] = &[(16, 16), (24, 24), (32, 24), (32, 32)];
    const SAMPLE_RATES: &[u32] = &[44100, 48000, 88200, 96000, 176400, 192000];

    let mut supported = Vec::new();
    for &(container_bits, valid_bits) in BIT_DEPTHS {
        for &rate in SAMPLE_RATES {
            // Avoid duplicate (24-in-24 vs 24-in-32) for the same valid_bits + rate.
            if supported
                .iter()
                .any(|f: &AudioFormatOption| f.valid_bits == valid_bits && f.sample_rate == rate)
            {
                continue;
            }
            let candidate = AudioFormatOption {
                channels,
                sample_rate: rate,
                bits_per_sample: container_bits,
                valid_bits,
                channel_mask,
                is_float: false,
            };
            let wf = build_wave_format_extensible(&candidate);
            let hr = unsafe {
                client.IsFormatSupported(
                    AUDCLNT_SHAREMODE_EXCLUSIVE,
                    &wf.Format as *const WAVEFORMATEX,
                    None,
                )
            };
            if hr == S_OK {
                supported.push(candidate);
            }
        }
    }

    if let Some(cur) = current {
        if !supported.iter().any(|f| f.id() == cur.id()) {
            supported.push(cur.clone());
        }
    }
    supported.sort_by_key(|f| (f.valid_bits, f.sample_rate));
    supported
}

unsafe fn read_mic_topology_controls(device: &IMMDevice) -> (Option<MicBoostInfo>, Option<bool>) {
    let boost = unsafe { find_mic_boost_control(device) }.and_then(|vol| unsafe {
        let mut min_db = 0.0_f32;
        let mut max_db = 0.0_f32;
        let mut step_db = 1.0_f32;
        vol.GetLevelRange(0, &mut min_db, &mut max_db, &mut step_db)
            .ok()?;
        if (max_db - min_db).abs() < 0.5 {
            return None;
        }
        let current_db = vol.GetLevel(0).ok()?;
        Some(MicBoostInfo {
            min_db,
            max_db,
            step_db: if step_db > 0.0 { step_db } else { 1.0 },
            current_db,
        })
    });
    let agc = unsafe { find_mic_agc_control(device) }
        .and_then(|ctrl| unsafe { ctrl.GetEnabled().ok().map(|b| b.as_bool()) });
    (boost, agc)
}

unsafe fn activate_part<T: Interface>(part: &IPart) -> Option<T> {
    let mut raw: *mut c_void = null_mut();
    unsafe { part.Activate(CLSCTX_ALL.0, &T::IID, Some(&mut raw)) }.ok()?;
    if raw.is_null() {
        return None;
    }
    Some(unsafe { T::from_raw(raw) })
}

unsafe fn find_mic_boost_control(device: &IMMDevice) -> Option<IAudioVolumeLevel> {
    let parts = unsafe { walk_incoming_parts(device) };
    // Look for a boost/volume node that isn't the primary negative-dB capture fader
    // (microphone boost ranges typically start at 0 dB and go positive, e.g. 0..+30 dB).
    for part in parts {
        if let Some(vol) = unsafe { activate_part::<IAudioVolumeLevel>(&part) } {
            let mut min_db = 0.0_f32;
            let mut max_db = 0.0_f32;
            let mut step_db = 0.0_f32;
            if unsafe { vol.GetLevelRange(0, &mut min_db, &mut max_db, &mut step_db) }.is_ok()
                && min_db >= -0.1
                && max_db > 0.5
            {
                return Some(vol);
            }
        }
    }
    None
}

unsafe fn find_mic_agc_control(device: &IMMDevice) -> Option<IAudioAutoGainControl> {
    let parts = unsafe { walk_incoming_parts(device) };
    for part in parts {
        if let Some(agc) = unsafe { activate_part::<IAudioAutoGainControl>(&part) } {
            return Some(agc);
        }
    }
    None
}

const ENDPOINT_HARDWARE_SUPPORT_VOLUME: u32 = 0x0000_0001;

/// True when a stereo render endpoint actually supports independent L/R channel
/// attenuation (software-mixed endpoints always do; hardware-volume endpoints
/// like Bluetooth AVRCP only do when their topology `IAudioVolumeLevel` node
/// exposes >= 2 channels).
unsafe fn endpoint_supports_balance(
    device: &IMMDevice,
    ep_vol: &IAudioEndpointVolume,
    ep_channels: u32,
) -> bool {
    if ep_channels != 2 {
        return false;
    }
    let hw_support = unsafe { ep_vol.QueryHardwareSupport().unwrap_or(0) };
    if (hw_support & ENDPOINT_HARDWARE_SUPPORT_VOLUME) == 0 {
        return true;
    }
    let parts = unsafe { walk_outgoing_parts(device) };
    for part in parts {
        if let Some(vol) = unsafe { activate_part::<IAudioVolumeLevel>(&part) } {
            if let Ok(ch) = unsafe { vol.GetChannelCount() } {
                return ch >= 2;
            }
        }
    }
    false
}

unsafe fn walk_outgoing_parts(device: &IMMDevice) -> Vec<IPart> {
    let Ok(topology) = (unsafe { device.Activate::<IDeviceTopology>(CLSCTX_ALL, None) }) else {
        return Vec::new();
    };
    let Ok(connector) = (unsafe { topology.GetConnector(0) }) else {
        return Vec::new();
    };
    let Ok(connected) = (unsafe { connector.GetConnectedTo() }) else {
        return Vec::new();
    };
    let Ok(root_part) = connected.cast::<IPart>() else {
        return Vec::new();
    };

    let mut result = Vec::new();
    let mut stack = vec![root_part];
    let mut visited = 0usize;
    while let Some(part) = stack.pop() {
        if visited > 64 {
            break;
        }
        visited += 1;
        if let Ok(outgoing) = unsafe { part.EnumPartsOutgoing() } {
            if let Ok(count) = unsafe { outgoing.GetCount() } {
                for i in 0..count {
                    if let Ok(next) = unsafe { outgoing.GetPart(i) } {
                        stack.push(next);
                    }
                }
            }
        }
        result.push(part);
    }
    result
}

unsafe fn walk_incoming_parts(device: &IMMDevice) -> Vec<IPart> {
    let Ok(topology) = (unsafe { device.Activate::<IDeviceTopology>(CLSCTX_ALL, None) }) else {
        return Vec::new();
    };
    let Ok(connector) = (unsafe { topology.GetConnector(0) }) else {
        return Vec::new();
    };
    let Ok(connected) = (unsafe { connector.GetConnectedTo() }) else {
        return Vec::new();
    };
    let Ok(root_part) = connected.cast::<IPart>() else {
        return Vec::new();
    };

    let mut result = Vec::new();
    let mut stack = vec![root_part];
    let mut visited = 0usize;
    while let Some(part) = stack.pop() {
        if visited > 64 {
            break;
        }
        visited += 1;
        if let Ok(incoming) = unsafe { part.EnumPartsIncoming() } {
            if let Ok(count) = unsafe { incoming.GetCount() } {
                for i in 0..count {
                    if let Ok(next) = unsafe { incoming.GetPart(i) } {
                        stack.push(next);
                    }
                }
            }
        }
        result.push(part);
    }
    result
}
