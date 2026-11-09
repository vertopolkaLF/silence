// Second tray icon: output volume, drawn as straight-alpha RGBA and wrapped in an HICON.
// Not wired into the tray yet. create_argb_icon owns the Win32 bitmap layout; accent and
// light-theme detection reuse the helpers already in the process.

#[cfg(not(feature = "volume_icon_raster_only"))]
use std::collections::HashMap;

#[cfg(not(feature = "volume_icon_raster_only"))]
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
#[cfg(not(feature = "volume_icon_raster_only"))]
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, FindWindowW, HICON, SM_CXSMICON};
#[cfg(not(feature = "volume_icon_raster_only"))]
use windows::core::{PCWSTR, w};


/// Which volume mark the second tray icon draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VolumeTraySkin {
    Rings,
    Digits,
    Bar,
    Speaker,
}

/// Colors and shape for one volume icon. `accent` is straight RGB.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VolumeIconStyle {
    pub skin: VolumeTraySkin,
    pub accent: [u8; 3],
    pub light_taskbar: bool,
    /// Taskbar ink only: no accent, no red.
    pub monochrome: bool,
    /// Rings: the arc starts at 6 o'clock instead of 12.
    pub ring_from_bottom: bool,
    /// Rings: the arc grows clockwise.
    pub ring_clockwise: bool,
}

impl VolumeIconStyle {
    pub fn new(skin: VolumeTraySkin, accent: [u8; 3], light_taskbar: bool) -> Self {
        Self {
            skin,
            accent,
            light_taskbar,
            monochrome: false,
            ring_from_bottom: false,
            ring_clockwise: true,
        }
    }
}

/// Straight-alpha RGBA, `size_px` by `size_px`, row 0 at the top.
///
/// `level_percent` above 100 is drawn as 100. `size_px == 0` returns an empty buffer.
pub fn render_volume_icon_rgba(
    style: &VolumeIconStyle,
    level_percent: u8,
    muted: bool,
    size_px: u32,
) -> Vec<u8> {
    if size_px == 0 {
        return Vec::new();
    }
    let level = level_percent.min(100);
    let ink = Ink::new(style, muted);
    let body = match style.skin {
        VolumeTraySkin::Rings => rings_svg(&ink, level, style),
        VolumeTraySkin::Digits => digits_svg(&ink, level),
        VolumeTraySkin::Bar => bars_svg(&ink, level),
        VolumeTraySkin::Speaker => speaker_svg(&ink, level),
    };
    raster_svg(
        &format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">{body}</svg>"#
        ),
        size_px,
    )
}

/// HICON for the notification area. The pixel layout matches [`create_argb_icon`](super::create_argb_icon):
/// top-down BGRA, straight alpha.
///
/// Sizes above 256 are rejected. The caller does not own the icon when it comes from
/// [`VolumeIconCache`]; this function's icon is owned by the caller.
#[cfg(not(feature = "volume_icon_raster_only"))]
pub fn create_volume_hicon(
    style: &VolumeIconStyle,
    level_percent: u8,
    muted: bool,
    size_px: u32,
) -> windows::core::Result<HICON> {
    if size_px == 0 || size_px > 256 {
        return Err(icon_error());
    }
    let rgba = render_volume_icon_rgba(style, level_percent, muted, size_px);
    if rgba.len() != (size_px as usize) * (size_px as usize) * 4 {
        return Err(icon_error());
    }
    let bgra = rgba_to_bgra(&rgba);
    super::create_argb_icon(size_px as i32, size_px as i32, &bgra).ok_or_else(icon_error)
}

/// Small icon size for the shell tray at the taskbar's own DPI.
///
/// Uses `Shell_TrayWnd` + `GetDpiForWindow` + `GetSystemMetricsForDpi(SM_CXSMICON)`.
/// If the tray window or the DPI call is missing, the size is 16.
#[cfg(not(feature = "volume_icon_raster_only"))]
pub fn tray_icon_size_px() -> u32 {
    unsafe {
        let dpi = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null())
            .ok()
            .filter(|hwnd| !hwnd.0.is_null())
            .map(|hwnd| GetDpiForWindow(hwnd))
            .filter(|dpi| *dpi > 0)
            .unwrap_or(96);
        let size = GetSystemMetricsForDpi(SM_CXSMICON, dpi);
        if size > 0 { size as u32 } else { 16 }
    }
}

/// WinUI accent fill for the taskbar theme (Light2 on dark, Dark1 on light).
#[cfg(not(feature = "volume_icon_raster_only"))]
pub fn windows_accent_rgb() -> [u8; 3] {
    let (r, g, b) = super::WindowsAccent::load().fill(taskbar_uses_light_theme());
    [r, g, b]
}

/// `HKCU\...\Themes\Personalize\SystemUsesLightTheme`.
///
/// Delegates to the existing helper. A missing value is treated as dark (`0`),
/// which is what the rest of the app already does.
#[cfg(not(feature = "volume_icon_raster_only"))]
pub fn taskbar_uses_light_theme() -> bool {
    super::windows_uses_light_system_theme()
}

/// Owns volume HICONs so a volume notification does not rasterize again.
///
/// The handle returned by [`VolumeIconCache::hicon`] stays valid until
/// [`VolumeIconCache::invalidate`] or drop. Do not call `DestroyIcon` on it.
#[cfg(not(feature = "volume_icon_raster_only"))]
#[derive(Debug, Default)]
pub struct VolumeIconCache {
    icons: HashMap<VolumeIconKey, HICON>,
}

#[cfg(not(feature = "volume_icon_raster_only"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct VolumeIconKey {
    skin: VolumeTraySkin,
    accent: [u8; 3],
    light_taskbar: bool,
    level: u8,
    muted: bool,
    size_px: u32,
}

#[cfg(not(feature = "volume_icon_raster_only"))]
impl VolumeIconCache {
    pub fn new() -> Self {
        Self {
            icons: HashMap::new(),
        }
    }

    /// Destroys every cached icon. Handles handed out earlier are no longer valid.
    pub fn invalidate(&mut self) {
        for (_, icon) in self.icons.drain() {
            unsafe {
                let _ = DestroyIcon(icon);
            }
        }
    }

    pub fn hicon(
        &mut self,
        style: &VolumeIconStyle,
        level_percent: u8,
        muted: bool,
        size_px: u32,
    ) -> windows::core::Result<HICON> {
        let key = VolumeIconKey {
            skin: style.skin,
            accent: style.accent,
            light_taskbar: style.light_taskbar,
            level: level_percent.min(100),
            muted,
            size_px,
        };
        if let Some(icon) = self.icons.get(&key) {
            return Ok(*icon);
        }
        let icon = create_volume_hicon(style, level_percent, muted, size_px)?;
        self.icons.insert(key, icon);
        Ok(icon)
    }
}

#[cfg(not(feature = "volume_icon_raster_only"))]
impl Drop for VolumeIconCache {
    fn drop(&mut self) {
        self.invalidate();
    }
}

#[cfg(not(feature = "volume_icon_raster_only"))]
fn icon_error() -> windows::core::Error {
    windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL)
}

#[cfg(not(feature = "volume_icon_raster_only"))]
fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut bgra = vec![0u8; rgba.len()];
    for (src, dst) in rgba.chunks_exact(4).zip(bgra.chunks_exact_mut(4)) {
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = src[3];
    }
    bgra
}

// All marks are drawn on a 16x16 grid, the tray's logical icon size, so
// strokes land on the same pixels Windows' own tray glyphs use.

/// Colors for one icon. Windows 11 tray glyphs are solid white on a dark
/// taskbar and solid black on a light one; the level uses the accent.
struct Ink {
    fg: &'static str,
    /// Unfilled part of a meter.
    track: &'static str,
    /// Filled part of a meter: accent, or red while muted.
    level: String,
    muted: bool,
}

impl Ink {
    fn new(style: &VolumeIconStyle, muted: bool) -> Self {
        let (fg, track, red) = if style.light_taskbar {
            ("#000000", "rgba(0,0,0,0.26)", "#C42B1C")
        } else {
            ("#FFFFFF", "rgba(255,255,255,0.30)", "#FF6B6B")
        };
        let level = if style.monochrome {
            if muted {
                if style.light_taskbar {
                    "rgba(0,0,0,0.45)".to_string()
                } else {
                    "rgba(255,255,255,0.45)".to_string()
                }
            } else {
                fg.to_string()
            }
        } else if muted {
            red.to_string()
        } else {
            let [r, g, b] = style.accent;
            format!("rgb({r},{g},{b})")
        };
        Self {
            fg,
            track,
            level,
            muted,
        }
    }
}

/// Ring with a round-capped arc from 12 or 6 o'clock, in either direction.
fn rings_svg(ink: &Ink, level: u8, style: &VolumeIconStyle) -> String {
    let (cx, cy, r, width) = (8.0_f32, 8.0_f32, 5.6_f32, 2.2_f32);
    // Solid black under the arc on a dark taskbar; a light taskbar keeps the
    // faint track so the ring stays visible.
    let track = if style.light_taskbar { ink.track } else { "#000000" };
    let mut out = format!(
        r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{track}" stroke-width="{width}"/>"#
    );
    if level >= 100 {
        out.push_str(&format!(
            r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{}" stroke-width="{width}"/>"#,
            ink.level
        ));
    } else if level > 0 {
        let turns = f32::from(level) / 100.0;
        let angle = turns * std::f32::consts::TAU;
        let dir = if style.ring_clockwise { 1.0 } else { -1.0 };
        let up = if style.ring_from_bottom { -1.0 } else { 1.0 };
        let (sx, sy) = (cx, cy - up * r);
        let (ex, ey) = (cx + up * dir * r * angle.sin(), cy - up * r * angle.cos());
        let large = if turns > 0.5 { 1 } else { 0 };
        let sweep = if style.ring_clockwise { 1 } else { 0 };
        out.push_str(&format!(
            r#"<path d="M{sx} {sy} A{r} {r} 0 {large} {sweep} {ex:.3} {ey:.3}" fill="none" stroke="{}" stroke-width="{width}" stroke-linecap="round"/>"#,
            ink.level
        ));
    }
    out
}

/// The number itself in Segoe UI Semibold, sized to the digit count.
fn digits_svg(ink: &Ink, level: u8) -> String {
    let text = level.to_string();
    let (size, spacing) = match text.len() {
        1 => (13.0_f32, 0.0_f32),
        2 => (11.5, -0.4),
        _ => (8.6, -0.5),
    };
    // Segoe UI digits are ~0.7em tall: center them on the grid.
    let baseline = 8.0 + size * 0.35;
    let color = if ink.muted { ink.level.as_str() } else { ink.fg };
    format!(
        r#"<text x="8" y="{baseline:.2}" text-anchor="middle" font-family="{DIGIT_FONT}" font-weight="600" font-size="{size}" letter-spacing="{spacing}" fill="{color}">{text}</text>"#
    )
}

/// Four rising bars, like a signal meter, filled up to the level.
fn bars_svg(ink: &Ink, level: u8) -> String {
    let filled = (u32::from(level) + 24) / 25;
    let width = 2.4_f32;
    let gap = 1.3_f32;
    let mut out = String::new();
    for i in 0..4u32 {
        let x = 1.6 + i as f32 * (width + gap);
        let height = 4.5 + i as f32 * 3.0;
        let y = 14.0 - height;
        let color = if i < filled {
            ink.level.as_str()
        } else {
            ink.track
        };
        out.push_str(&format!(
            r#"<rect x="{x:.2}" y="{y:.2}" width="{width}" height="{height:.2}" rx="1.1" fill="{color}"/>"#
        ));
    }
    out
}

/// Windows' own volume glyphs from Segoe Fluent Icons (MDL2 on Windows 10),
/// so this skin looks exactly like the system volume icon.
fn speaker_svg(ink: &Ink, level: u8) -> String {
    let glyph: u32 = if ink.muted {
        0xE74F
    } else {
        match level {
            0 => 0xE992,
            1..=33 => 0xE993,
            34..=66 => 0xE994,
            _ => 0xE995,
        }
    };
    format!(
        r#"<text x="0.5" y="15.5" font-family="{}" font-size="15" fill="{}">&#x{glyph:X};</text>"#,
        icon_font_family(),
        ink.fg,
    )
}

const DIGIT_FONT: &str = "Segoe UI";

fn icon_font_family() -> &'static str {
    if FONTS.1 {
        "Segoe Fluent Icons"
    } else {
        "Segoe MDL2 Assets"
    }
}

/// Font database with just the system fonts the skins use. Loaded once.
/// `.1` is true when Segoe Fluent Icons (Windows 11) is present.
static FONTS: once_cell::sync::Lazy<(std::sync::Arc<resvg::usvg::fontdb::Database>, bool)> =
    once_cell::sync::Lazy::new(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        let dir = std::env::var_os("WINDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("C:\\Windows"))
            .join("Fonts");
        let has_fluent = dir.join("SegoeIcons.ttf").exists();
        for file in ["SegoeIcons.ttf", "segmdl2.ttf", "seguisb.ttf"] {
            let _ = db.load_font_file(dir.join(file));
        }
        (std::sync::Arc::new(db), has_fluent)
    });

fn raster_svg(svg: &str, size: u32) -> Vec<u8> {
    let empty = vec![0u8; (size as usize) * (size as usize) * 4];
    let factor = if size <= 48 { 4 } else { 2 };
    let big = size.saturating_mul(factor);
    let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(big, big) else {
        return empty;
    };
    let options = resvg::usvg::Options {
        fontdb: FONTS.0.clone(),
        ..Default::default()
    };
    let Ok(tree) = resvg::usvg::Tree::from_str(svg, &options) else {
        return empty;
    };
    let svg_w = tree.size().width();
    if svg_w <= 0.0 {
        return empty;
    }
    let scale = big as f32 / svg_w;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    downsample_straight(pixmap.data(), big, size)
}

/// Box-filter premultiplied RGBA, then un-premultiply into straight alpha.
fn downsample_straight(premul: &[u8], src_size: u32, dst_size: u32) -> Vec<u8> {
    let src_px = (src_size as usize) * (src_size as usize);
    if premul.len() < src_px * 4 || dst_size == 0 || src_size < dst_size {
        return vec![0u8; (dst_size as usize) * (dst_size as usize) * 4];
    }
    let factor = (src_size / dst_size) as usize;
    if factor <= 1 {
        return demultiply_buffer(&premul[..src_px * 4]);
    }
    let src_size = src_size as usize;
    let dst_size = dst_size as usize;
    let area = (factor * factor) as u32;
    let mut out = vec![0u8; dst_size * dst_size * 4];
    for y in 0..dst_size {
        for x in 0..dst_size {
            let mut sum = [0u32; 4];
            for ky in 0..factor {
                let row = (y * factor + ky) * src_size;
                for kx in 0..factor {
                    let i = (row + x * factor + kx) * 4;
                    sum[0] += u32::from(premul[i]);
                    sum[1] += u32::from(premul[i + 1]);
                    sum[2] += u32::from(premul[i + 2]);
                    sum[3] += u32::from(premul[i + 3]);
                }
            }
            let alpha = (sum[3] + area / 2) / area;
            let o = (y * dst_size + x) * 4;
            if alpha == 0 {
                continue;
            }
            for channel in 0..3 {
                let averaged = (sum[channel] + area / 2) / area;
                out[o + channel] = ((averaged * 255 + alpha / 2) / alpha).min(255) as u8;
            }
            out[o + 3] = alpha as u8;
        }
    }
    out
}

fn demultiply_buffer(premul: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(premul.len());
    for px in premul.chunks_exact(4) {
        let alpha = u32::from(px[3]);
        if alpha == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        for channel in 0..3 {
            let straight = (u32::from(px[channel]) * 255 + alpha / 2) / alpha;
            out.push(straight.min(255) as u8);
        }
        out.push(px[3]);
    }
    out
}

/// Contact sheet for the dev preview. Bands are 16, 20, 24, 32.
/// Inside a band: dark rings/digits/bar/speaker, then the same on a light taskbar.
/// Columns: 0, 10, 25, 50, 75, 100, muted 40. Icons are nearest-neighbor ×8.
#[cfg(any(test, feature = "volume_icon_raster_only"))]
pub fn write_volume_icon_preview(path: &std::path::Path) -> std::io::Result<()> {
    let (width, height, rgba) = render_volume_icon_sheet();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, encode_png(width, height, &rgba))
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn render_volume_icon_sheet() -> (u32, u32, Vec<u8>) {
    const SCALE: u32 = 8;
    const GAP: u32 = 10;
    const THEME_GAP: u32 = 22;
    const BAND_GAP: u32 = 36;
    const MARGIN: u32 = 16;
    const SIZES: [u32; 4] = [16, 20, 24, 32];
    const SKINS: [VolumeTraySkin; 4] = [
        VolumeTraySkin::Rings,
        VolumeTraySkin::Digits,
        VolumeTraySkin::Bar,
        VolumeTraySkin::Speaker,
    ];
    const LEVELS: [(u8, bool); 7] = [
        (0, false),
        (10, false),
        (25, false),
        (50, false),
        (75, false),
        (100, false),
        (40, true),
    ];
    let accent = [0, 120, 215];
    let mut inner_w = 0u32;
    for size in SIZES {
        let cell = size * SCALE;
        inner_w = inner_w.max(7 * cell + 6 * GAP);
    }
    let width = inner_w + MARGIN * 2;
    let mut height = MARGIN;
    for (band_index, size) in SIZES.iter().enumerate() {
        let cell = size * SCALE;
        for theme_index in 0..2 {
            for skin_index in 0..SKINS.len() {
                height += cell;
                let last_skin = skin_index + 1 == SKINS.len();
                let last_theme = theme_index + 1 == 2;
                if !last_skin {
                    height += GAP;
                } else if !last_theme {
                    height += THEME_GAP;
                }
            }
        }
        if band_index + 1 != SIZES.len() {
            height += BAND_GAP;
        }
    }
    height += MARGIN;

    let mut sheet = vec![0x3Cu8; (width as usize) * (height as usize) * 4];
    for pixel in sheet.chunks_exact_mut(4) {
        pixel[3] = 255;
    }

    let mut y = MARGIN;
    for (band_index, size) in SIZES.iter().copied().enumerate() {
        let cell = size * SCALE;
        let row_w = 7 * cell + 6 * GAP;
        let x0 = MARGIN + (inner_w - row_w) / 2;
        for (theme_index, light) in [false, true].into_iter().enumerate() {
            let bg = if light {
                [0xF3, 0xF3, 0xF3]
            } else {
                [0x1C, 0x1C, 0x1C]
            };
            for (skin_index, skin) in SKINS.iter().copied().enumerate() {
                let style = VolumeIconStyle::new(skin, accent, light);
                let mut x = x0;
                for (level, muted) in LEVELS {
                    let icon = render_volume_icon_rgba(&style, level, muted, size);
                    let composed = composite_on(&icon, bg);
                    let up = upscale_nearest(&composed, size, size, SCALE);
                    blit(&mut sheet, width, x, y, &up, cell, cell);
                    x += cell + GAP;
                }
                y += cell;
                let last_skin = skin_index + 1 == SKINS.len();
                let last_theme = theme_index + 1 == 2;
                if !last_skin {
                    y += GAP;
                } else if !last_theme {
                    y += THEME_GAP;
                }
            }
        }
        if band_index + 1 != SIZES.len() {
            y += BAND_GAP;
        }
    }
    (width, height, sheet)
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn composite_on(icon: &[u8], bg: [u8; 3]) -> Vec<u8> {
    let mut out = Vec::with_capacity(icon.len());
    for px in icon.chunks_exact(4) {
        let alpha = u32::from(px[3]);
        let inv = 255 - alpha;
        out.push(((u32::from(px[0]) * alpha + u32::from(bg[0]) * inv + 127) / 255) as u8);
        out.push(((u32::from(px[1]) * alpha + u32::from(bg[1]) * inv + 127) / 255) as u8);
        out.push(((u32::from(px[2]) * alpha + u32::from(bg[2]) * inv + 127) / 255) as u8);
        out.push(255);
    }
    out
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn upscale_nearest(src: &[u8], width: u32, height: u32, scale: u32) -> Vec<u8> {
    let dst_w = width * scale;
    let dst_h = height * scale;
    let mut dst = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];
    for y in 0..height {
        for x in 0..width {
            let si = ((y * width + x) * 4) as usize;
            let pixel = [src[si], src[si + 1], src[si + 2], src[si + 3]];
            let x0 = x * scale;
            let y0 = y * scale;
            for dy in 0..scale {
                for dx in 0..scale {
                    let di = (((y0 + dy) * dst_w + (x0 + dx)) * 4) as usize;
                    dst[di..di + 4].copy_from_slice(&pixel);
                }
            }
        }
    }
    dst
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn blit(dst: &mut [u8], dst_w: u32, x: u32, y: u32, src: &[u8], src_w: u32, src_h: u32) {
    let dst_h = dst.len() / (dst_w as usize * 4);
    for row in 0..src_h {
        let dy = y + row;
        if dy as usize >= dst_h || x.saturating_add(src_w) > dst_w {
            continue;
        }
        let si = (row * src_w * 4) as usize;
        let di = ((dy * dst_w + x) * 4) as usize;
        let n = (src_w * 4) as usize;
        dst[di..di + n].copy_from_slice(&src[si..si + n]);
    }
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let filtered = filter_none_rows(rgba, width, height);
    let stride = 1 + (width as usize) * 4;
    let zlib = zlib_store(&filtered, stride);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    png.extend(png_chunk(b"IHDR", &ihdr));
    png.extend(png_chunk(b"IDAT", &zlib));
    png.extend(png_chunk(b"IEND", &[]));
    png
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn filter_none_rows(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let row = (width as usize) * 4;
    let mut out = Vec::with_capacity((height as usize) * (1 + row));
    for y in 0..height as usize {
        out.push(0);
        let start = y * row;
        out.extend_from_slice(&rgba[start..start + row]);
    }
    out
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn zlib_store(data: &[u8], stride: usize) -> Vec<u8> {
    let mut zlib = Vec::new();
    zlib.push(0x78);
    zlib.push(0x01);
    zlib.extend(deflate_fixed(data, stride));
    zlib.extend_from_slice(&adler32(data).to_be_bytes());
    zlib
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn adler32(data: &[u8]) -> u32 {
    let mut a = 1u64;
    let mut b = 0u64;
    for (index, &byte) in data.iter().enumerate() {
        a += u64::from(byte);
        b += a;
        if index % 5550 == 5499 {
            a %= 65521;
            b %= 65521;
        }
    }
    a %= 65521;
    b %= 65521;
    ((b as u32) << 16) | (a as u32)
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn deflate_fixed(data: &[u8], stride: usize) -> Vec<u8> {
    let mut bits = BitWriter::new();
    // BFINAL=1, BTYPE=01 (fixed Huffman), LSB first.
    bits.write_bits(0b011, 3);
    let mut index = 0;
    while index < data.len() {
        let (len, dist) = longest_match(data, index, stride);
        if len >= 3 {
            emit_length(&mut bits, len as u16);
            emit_distance(&mut bits, dist as u16);
            index += len;
        } else {
            emit_symbol(&mut bits, u16::from(data[index]));
            index += 1;
        }
    }
    emit_symbol(&mut bits, 256);
    bits.finish()
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn longest_match(data: &[u8], index: usize, stride: usize) -> (usize, usize) {
    let max_len = (data.len() - index).min(258);
    if max_len < 3 {
        return (0, 0);
    }
    let mut best_len = 0usize;
    let mut best_dist = 0usize;
    let candidates = [
        1usize,
        4,
        stride,
        stride.saturating_mul(2),
        stride.saturating_mul(8),
    ];
    for dist in candidates {
        if dist == 0 || dist > index || dist > 32768 {
            continue;
        }
        let mut len = 0;
        while len < max_len && data[index + len] == data[index + len - dist] {
            len += 1;
        }
        if len > best_len || (len == best_len && len >= 3 && dist < best_dist) {
            best_len = len;
            best_dist = dist;
        }
    }
    if best_len >= 3 {
        (best_len, best_dist)
    } else {
        (0, 0)
    }
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn emit_symbol(bits: &mut BitWriter, symbol: u16) {
    let (code, len) = fixed_lit_code(symbol);
    bits.write_bits(reverse_bits(code, len), len);
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn emit_length(bits: &mut BitWriter, len: u16) {
    let (symbol, extra_bits, extra) = length_code(len);
    emit_symbol(bits, symbol);
    if extra_bits > 0 {
        bits.write_bits(u32::from(extra), extra_bits);
    }
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn emit_distance(bits: &mut BitWriter, dist: u16) {
    let (symbol, extra_bits, extra) = distance_code(dist);
    bits.write_bits(reverse_bits(u32::from(symbol), 5), 5);
    if extra_bits > 0 {
        bits.write_bits(u32::from(extra), extra_bits);
    }
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn fixed_lit_code(symbol: u16) -> (u32, u8) {
    if symbol <= 143 {
        (u32::from(symbol) + 48, 8)
    } else if symbol <= 255 {
        (u32::from(symbol) - 144 + 400, 9)
    } else if symbol <= 279 {
        (u32::from(symbol) - 256, 7)
    } else {
        (u32::from(symbol) - 280 + 192, 8)
    }
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn length_code(len: u16) -> (u16, u8, u16) {
    if len < 11 {
        return (257 + (len - 3), 0, 0);
    }
    if len == 258 {
        return (285, 0, 0);
    }
    let table = [(11u16, 265u16, 1u8), (19, 269, 2), (35, 273, 3), (67, 277, 4), (131, 281, 5)];
    for (index, &(base, symbol0, extra)) in table.iter().enumerate() {
        let next = if index + 1 < table.len() {
            table[index + 1].0
        } else {
            258
        };
        if len >= base && len < next {
            let span = 1u16 << extra;
            let offset = len - base;
            return (symbol0 + offset / span, extra, offset % span);
        }
    }
    (285, 0, 0)
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn distance_code(dist: u16) -> (u16, u8, u16) {
    if dist <= 4 {
        return (dist - 1, 0, 0);
    }
    let mut base = 5u16;
    let mut symbol = 4u16;
    let mut extra = 1u8;
    loop {
        for _ in 0..2 {
            let span = 1u16 << extra;
            if dist < base.saturating_add(span) {
                return (symbol, extra, dist - base);
            }
            base = base.saturating_add(span);
            symbol += 1;
            if symbol > 29 {
                return (29, 13, dist.saturating_sub(24577));
            }
        }
        extra += 1;
    }
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn reverse_bits(mut value: u32, n: u8) -> u32 {
    let mut out = 0u32;
    for _ in 0..n {
        out = (out << 1) | (value & 1);
        value >>= 1;
    }
    out
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
struct BitWriter {
    out: Vec<u8>,
    acc: u32,
    n: u8,
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
impl BitWriter {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            acc: 0,
            n: 0,
        }
    }

    fn write_bits(&mut self, mut value: u32, mut nbits: u8) {
        while nbits > 0 {
            let space = 32 - self.n;
            let take = nbits.min(space);
            let mask = if take == 32 {
                u32::MAX
            } else {
                (1u32 << take) - 1
            };
            self.acc |= (value & mask) << self.n;
            self.n += take;
            value = if take == 32 { 0 } else { value >> take };
            nbits -= take;
            while self.n >= 8 {
                self.out.push(self.acc as u8);
                self.acc >>= 8;
                self.n -= 8;
            }
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn png_chunk(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc_src = Vec::with_capacity(4 + data.len());
    crc_src.extend_from_slice(tag);
    crc_src.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_src).to_be_bytes());
    out
}

#[cfg(any(test, feature = "volume_icon_raster_only"))]
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = if crc & 1 == 1 { 0xEDB8_8320 } else { 0 };
            crc = (crc >> 1) ^ mask;
        }
    }
    !crc
}

#[cfg(test)]
mod volume_icon_preview_tests {
    use super::*;

    #[test]
    fn volume_icon_preview() {
        let icon = render_volume_icon_rgba(
            &VolumeIconStyle::new(VolumeTraySkin::Digits, [0, 120, 215], false),
            100,
            false,
            16,
        );
        assert_eq!(icon.len(), 16 * 16 * 4);
        assert!(icon.chunks_exact(4).any(|px| px[3] > 200));

        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/volume_icon_preview.png");
        write_volume_icon_preview(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn volume_hicon_is_cached_until_invalidate() {
        let style = VolumeIconStyle::new(VolumeTraySkin::Bar, [0, 120, 215], false);
        let icon = create_volume_hicon(&style, 40, false, 16).unwrap();
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::DestroyIcon(icon).unwrap();
        }

        let mut cache = VolumeIconCache::new();
        let first = cache.hicon(&style, 50, false, 16).unwrap();
        let second = cache.hicon(&style, 50, false, 16).unwrap();
        assert_eq!(first.0, second.0);
        let other = cache.hicon(&style, 50, true, 16).unwrap();
        assert_ne!(first.0, other.0);
        cache.invalidate();
        assert!(cache.icons.is_empty());
    }
}
