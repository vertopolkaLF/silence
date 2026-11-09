//! Fake Mica for the volume OSD: the desktop wallpaper, heavily blurred, cut
//! to the part the card sits over. Real Mica/Acrylic is not available to a
//! transparent DirectComposition window, so this approximates it.
//!
//! The wallpaper is decoded and blurred once per monitor size at low
//! resolution (blur hides the missing detail) on a background thread. The
//! renderer only crops a tiny region and lets the GPU scale it up.

use gpui::{Image, ImageFormat};
use image::{RgbaImage, imageops};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::SystemTime,
};
use windows::Win32::{
    Foundation::RECT,
    UI::WindowsAndMessaging::{
        SPI_GETDESKWALLPAPER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    },
};

/// Width of the blurred monitor image. Mica's blur is so wide that more
/// pixels would not show.
const LOW_RES_WIDTH: u32 = 256;
const BLUR_SIGMA: f32 = 4.8;

struct Blurred {
    wallpaper: PathBuf,
    modified: Option<SystemTime>,
    monitor_w: i32,
    monitor_h: i32,
    image: RgbaImage,
}

struct Crop {
    key: (RECT, RECT, Option<SystemTime>),
    image: Arc<Image>,
}

static BLURRED: Mutex<Option<Blurred>> = Mutex::new(None);
static CROP: Mutex<Option<Crop>> = Mutex::new(None);
static BUSY: Mutex<bool> = Mutex::new(false);

fn wallpaper_path() -> Option<(PathBuf, Option<SystemTime>)> {
    let mut buffer = [0u16; 520];
    unsafe {
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            buffer.len() as u32,
            Some(buffer.as_mut_ptr().cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .ok()?;
    }
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    if len == 0 {
        return None;
    }
    let path = PathBuf::from(String::from_utf16_lossy(&buffer[..len]));
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    Some((path, modified))
}

/// Decode, cover-fit to the monitor (Windows' default "Fill"), downscale and blur.
fn build(path: &PathBuf, monitor_w: i32, monitor_h: i32) -> Option<RgbaImage> {
    // Shrink huge wallpapers first; the blur throws the detail away anyway.
    let source = image::open(path).ok()?.thumbnail(1280, 1280).to_rgba8();
    let (sw, sh) = (source.width() as f32, source.height() as f32);
    let (mw, mh) = (monitor_w.max(1) as f32, monitor_h.max(1) as f32);
    let low_w = LOW_RES_WIDTH;
    let low_h = ((LOW_RES_WIDTH as f32) * mh / mw).round().max(1.0) as u32;
    // Cover: the source region that fills the monitor aspect, centered.
    let scale = (mw / sw).max(mh / sh);
    let crop_w = (mw / scale).min(sw);
    let crop_h = (mh / scale).min(sh);
    let x = ((sw - crop_w) / 2.0).max(0.0) as u32;
    let y = ((sh - crop_h) / 2.0).max(0.0) as u32;
    let region = imageops::crop_imm(&source, x, y, crop_w as u32, crop_h as u32).to_image();
    let small = imageops::resize(&region, low_w, low_h, imageops::FilterType::Triangle);
    Some(imageops::blur(&small, BLUR_SIGMA))
}

/// Start (or refresh) the blurred wallpaper for this monitor size in the
/// background. Cheap to call often: it returns at once when current.
pub(crate) fn prepare(monitor_w: i32, monitor_h: i32) {
    let Some((path, modified)) = wallpaper_path() else {
        *BLURRED.lock().unwrap() = None;
        return;
    };
    {
        let blurred = BLURRED.lock().unwrap();
        if let Some(current) = blurred.as_ref()
            && current.wallpaper == path
            && current.modified == modified
            && current.monitor_w == monitor_w
            && current.monitor_h == monitor_h
        {
            return;
        }
    }
    {
        let mut busy = BUSY.lock().unwrap();
        if *busy {
            return;
        }
        *busy = true;
    }
    std::thread::spawn(move || {
        let image = build(&path, monitor_w, monitor_h);
        *BLURRED.lock().unwrap() = image.map(|image| Blurred {
            wallpaper: path,
            modified,
            monitor_w,
            monitor_h,
            image,
        });
        *BUSY.lock().unwrap() = false;
    });
}

/// Warm the cache for the primary monitor so the first OSD already has it.
pub(crate) fn prewarm() {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let (width, height) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    if width > 0 && height > 0 {
        prepare(width, height);
    }
}

/// True while the wallpaper is being blurred in the background.
pub(crate) fn pending() -> bool {
    *BUSY.lock().unwrap()
}

/// The blurred wallpaper under `card` (physical pixels, screen coordinates)
/// on `monitor`, or `None` while it is still being prepared.
pub(crate) fn crop(monitor: RECT, card: RECT) -> Option<Arc<Image>> {
    let monitor_w = monitor.right - monitor.left;
    let monitor_h = monitor.bottom - monitor.top;
    prepare(monitor_w, monitor_h);
    let blurred = BLURRED.lock().unwrap();
    let blurred = blurred.as_ref()?;
    if blurred.monitor_w != monitor_w || blurred.monitor_h != monitor_h {
        return None;
    }
    let key = (monitor, card, blurred.modified);
    {
        let cached = CROP.lock().unwrap();
        if let Some(cached) = cached.as_ref()
            && cached.key == key
        {
            return Some(cached.image.clone());
        }
    }
    let scale = blurred.image.width() as f32 / monitor_w.max(1) as f32;
    let to_low = |value: i32, origin: i32| ((value - origin) as f32 * scale).round() as i64;
    let left = to_low(card.left, monitor.left).clamp(0, blurred.image.width() as i64 - 1) as u32;
    let top = to_low(card.top, monitor.top).clamp(0, blurred.image.height() as i64 - 1) as u32;
    let right = to_low(card.right, monitor.left).clamp(left as i64 + 1, blurred.image.width() as i64) as u32;
    let bottom =
        to_low(card.bottom, monitor.top).clamp(top as i64 + 1, blurred.image.height() as i64) as u32;
    let region = imageops::crop_imm(&blurred.image, left, top, right - left, bottom - top).to_image();
    // Scale up to the card's own pixels here: a few-texel image stretched on
    // the GPU fades out over its outer half-texel and shows a dark rim.
    let card_w = (card.right - card.left).clamp(1, 1600) as u32;
    let card_h = (card.bottom - card.top).clamp(1, 1600) as u32;
    let region = imageops::resize(&region, card_w, card_h, imageops::FilterType::Triangle);
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(region)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    let image = Arc::new(Image::from_bytes(ImageFormat::Png, png));
    *CROP.lock().unwrap() = Some(Crop {
        key,
        image: image.clone(),
    });
    Some(image)
}
