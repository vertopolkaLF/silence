//! Per-app volume on the default output device, for the volume popup.
//!
//! Sessions are grouped by app (exe path; System Sounds on its own), the way
//! the Windows mixer and EarTrumpet show them. Calls are cheap enough for the
//! popup to poll while it is open, and safe from any thread: each call joins
//! the COM apartment the thread already has, or the MTA.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use once_cell::sync::Lazy;
use windows::{
    Win32::{
        Foundation::S_OK,
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
            CreateSolidBrush, DIB_RGB_COLORS, DeleteDC, DeleteObject, FillRect, SelectObject,
        },
        Media::Audio::{
            AudioSessionStateExpired, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
            ISimpleAudioVolume, MMDeviceEnumerator, eConsole, eRender,
        },
        Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW},
        System::Com::{
            CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
        },
        UI::{
            Shell::SHDefExtractIconW,
            WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx, HICON},
        },
    },
    core::{HSTRING, Interface, PCWSTR},
};

/// Key of the System Sounds row.
pub const SYSTEM_KEY: &str = "system";

#[derive(Clone, Debug, PartialEq)]
pub struct AppVolume {
    /// Stable per app: exe path (lowercase), `pid:<n>`, or [`SYSTEM_KEY`].
    pub key: String,
    pub name: String,
    /// Exe the icon comes from; `None` for System Sounds or unknown apps.
    pub icon_path: Option<PathBuf>,
    pub percent: u8,
    pub muted: bool,
    pub system: bool,
}

struct Session {
    key: String,
    pid: u32,
    path: Option<PathBuf>,
    display_name: String,
    system: bool,
    volume: ISimpleAudioVolume,
}

fn join_com() {
    // Fails harmlessly when the thread already has an apartment.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

fn sessions() -> Vec<Session> {
    join_com();
    let Ok(enumerator) = (unsafe {
        CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
    }) else {
        return Vec::new();
    };
    let Ok(device) = (unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }) else {
        return Vec::new();
    };
    let Ok(manager) = (unsafe { device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) }) else {
        return Vec::new();
    };
    let Ok(list) = (unsafe { manager.GetSessionEnumerator() }) else {
        return Vec::new();
    };
    let count = unsafe { list.GetCount() }.unwrap_or(0);
    let mut out = Vec::new();
    for index in 0..count {
        let Ok(control) = (unsafe { list.GetSession(index) }) else {
            continue;
        };
        if unsafe { control.GetState() }.is_ok_and(|state| state == AudioSessionStateExpired) {
            continue;
        }
        let Ok(control) = control.cast::<IAudioSessionControl2>() else {
            continue;
        };
        let Ok(volume) = control.cast::<ISimpleAudioVolume>() else {
            continue;
        };
        let system = unsafe { control.IsSystemSoundsSession() } == S_OK;
        let pid = unsafe { control.GetProcessId() }.unwrap_or(0);
        let display_name = unsafe { control.GetDisplayName() }
            .ok()
            .map(|name| unsafe {
                let text = name.to_string().unwrap_or_default();
                CoTaskMemFree(Some(name.0 as _));
                text
            })
            .filter(|name| !name.is_empty() && !name.starts_with('@'))
            .unwrap_or_default();
        let path = if system { None } else { crate::process_image_path(pid) };
        let key = if system {
            SYSTEM_KEY.to_string()
        } else if let Some(path) = &path {
            path.to_string_lossy().to_lowercase()
        } else {
            format!("pid:{pid}")
        };
        out.push(Session {
            key,
            pid,
            path,
            display_name,
            system,
            volume,
        });
    }
    out
}

/// Apps with a live audio session on the default output, System Sounds first.
pub fn app_volumes() -> Vec<AppVolume> {
    let mut apps: Vec<AppVolume> = Vec::new();
    for session in sessions() {
        if apps.iter().any(|app| app.key == session.key) {
            continue;
        }
        let level = unsafe { session.volume.GetMasterVolume() }.unwrap_or(1.0);
        let muted = unsafe { session.volume.GetMute() }.is_ok_and(|muted| muted.as_bool());
        let name = if session.system {
            "System sounds".to_string()
        } else if !session.display_name.is_empty() {
            session.display_name.clone()
        } else {
            app_name(session.pid, session.path.as_deref())
        };
        apps.push(AppVolume {
            key: session.key,
            name,
            icon_path: session.path,
            percent: (level.clamp(0.0, 1.0) * 100.0).round() as u8,
            muted,
            system: session.system,
        });
    }
    apps.sort_by(|a, b| {
        b.system
            .cmp(&a.system)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    apps
}

pub fn set_app_volume(key: &str, percent: u8) {
    let level = f32::from(percent.min(100)) / 100.0;
    for session in sessions().into_iter().filter(|session| session.key == key) {
        let _ = unsafe { session.volume.SetMasterVolume(level, std::ptr::null()) };
    }
}

pub fn set_app_mute(key: &str, muted: bool) {
    for session in sessions().into_iter().filter(|session| session.key == key) {
        let _ = unsafe { session.volume.SetMute(muted, std::ptr::null()) };
    }
}

static NAMES: Lazy<Mutex<HashMap<PathBuf, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// The exe's FileDescription ("Google Chrome"), else its file name.
fn app_name(pid: u32, path: Option<&Path>) -> String {
    let Some(path) = path else {
        return format!("Process {pid}");
    };
    if let Some(name) = NAMES.lock().unwrap().get(path) {
        return name.clone();
    }
    let name = file_description(path)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("Process {pid}"))
        });
    NAMES.lock().unwrap().insert(path.to_path_buf(), name.clone());
    name
}

fn file_description(path: &Path) -> Option<String> {
    let path = HSTRING::from(path.as_os_str());
    unsafe {
        let size = GetFileVersionInfoSizeW(&path, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&path, 0, size, data.as_mut_ptr().cast()).ok()?;
        let mut pointer = std::ptr::null_mut();
        let mut len = 0u32;
        let mut query = |sub: &str| -> Option<(*const u8, u32)> {
            let sub = HSTRING::from(sub);
            VerQueryValueW(data.as_ptr().cast(), &sub, &mut pointer, &mut len)
                .as_bool()
                .then_some((pointer as *const u8, len))
        };
        // First translation, then the usual English fallbacks.
        let mut languages = Vec::new();
        if let Some((pointer, len)) = query("\\VarFileInfo\\Translation")
            && len >= 4
        {
            let pairs = std::slice::from_raw_parts(pointer as *const u16, (len / 2) as usize);
            for pair in pairs.chunks_exact(2) {
                languages.push(format!("{:04x}{:04x}", pair[0], pair[1]));
            }
        }
        languages.extend(["040904b0".to_string(), "040904e4".to_string()]);
        for language in languages {
            if let Some((pointer, len)) = query(&format!("\\StringFileInfo\\{language}\\FileDescription"))
                && len > 1
            {
                let text = std::slice::from_raw_parts(pointer as *const u16, len as usize);
                let end = text.iter().position(|c| *c == 0).unwrap_or(text.len());
                let text = String::from_utf16_lossy(&text[..end]).trim().to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        None
    }
}

static ICONS: Lazy<Mutex<HashMap<(PathBuf, u32), Option<Arc<Vec<u8>>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// The exe's icon as PNG, `size` pixels square. Cached.
pub fn app_icon_png(path: &Path, size: u32) -> Option<Arc<Vec<u8>>> {
    let key = (path.to_path_buf(), size);
    if let Some(icon) = ICONS.lock().unwrap().get(&key) {
        return icon.clone();
    }
    let icon = extract_icon(path, size).map(Arc::new);
    ICONS.lock().unwrap().insert(key, icon.clone());
    icon
}

fn extract_icon(path: &Path, size: u32) -> Option<Vec<u8>> {
    let size = size.clamp(16, 256);
    let path_w = HSTRING::from(path.as_os_str());
    let mut icon = HICON::default();
    unsafe {
        SHDefExtractIconW(PCWSTR(path_w.as_ptr()), 0, 0, Some(&mut icon), None, size).ok().ok()?;
    }
    if icon.is_invalid() {
        return None;
    }
    // Draw on black and on white; the difference gives the alpha even for
    // old icons that only have a mask.
    let black = draw_icon(icon, size, 0x000000);
    let white = draw_icon(icon, size, 0xFFFFFF);
    unsafe {
        let _ = DestroyIcon(icon);
    }
    let (black, white) = (black?, white?);
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for i in 0..(size * size) as usize {
        let b = &black[i * 4..i * 4 + 4];
        let w = &white[i * 4..i * 4 + 4];
        let alpha = 255 - (i32::from(w[1]) - i32::from(b[1])).clamp(0, 255);
        let out = &mut rgba[i * 4..i * 4 + 4];
        if alpha > 0 {
            // On black, color = straight color * alpha.
            let unmul = |c: u8| ((u32::from(c) * 255 + alpha as u32 / 2) / alpha as u32).min(255) as u8;
            out[0] = unmul(b[2]);
            out[1] = unmul(b[1]);
            out[2] = unmul(b[0]);
        }
        out[3] = alpha as u8;
    }
    let image = image::RgbaImage::from_raw(size, size, rgba)?;
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(png)
}

/// BGRA pixels of `icon` drawn over a solid `background` (0xBBGGRR).
fn draw_icon(icon: HICON, size: u32, background: u32) -> Option<Vec<u8>> {
    let side = size as i32;
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: side,
            biHeight: -side,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let dc = CreateCompatibleDC(None);
        let old = SelectObject(dc, bitmap);
        let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(background));
        let rect = windows::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: side,
            bottom: side,
        };
        FillRect(dc, &rect, brush);
        let _ = DeleteObject(brush);
        let drawn = DrawIconEx(dc, 0, 0, icon, side, side, 0, None, DI_NORMAL).is_ok();
        let pixels = (drawn && !bits.is_null())
            .then(|| std::slice::from_raw_parts(bits as *const u8, (size * size * 4) as usize).to_vec());
        SelectObject(dc, old);
        let _ = DeleteDC(dc);
        let _ = DeleteObject(bitmap);
        pixels
    }
}
