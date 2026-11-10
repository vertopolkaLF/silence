//! Real Mica or Acrylic under a GPUI popup.
//!
//! GPUI presents through a topmost DirectComposition target on its HWND.
//! Windows allows a second, non-topmost target on the same HWND, so a rounded
//! SpriteVisual there sits under GPUI's swap chain. Its brush is the
//! compositor's blurred-wallpaper brush (the source DWM's Mica uses) or its
//! host backdrop brush (whatever is behind the window, as Acrylic uses),
//! blended to the theme's lightness; GPUI paints the material's tint on top.
//!
//! Ported from candle-tray's popup backdrop.

use anyhow::{Context, Result};
use windows::{
    Foundation::{
        IPropertyValue, PropertyValue,
        Numerics::{Vector2, Vector3},
    },
    Graphics::Effects::{
        IGraphicsEffect, IGraphicsEffect_Impl, IGraphicsEffectSource, IGraphicsEffectSource_Impl,
    },
    System::{DispatcherQueue, DispatcherQueueController},
    UI::{
        Color,
        Composition::{
            CompositionBrush, CompositionColorBrush, CompositionEffectSourceParameter,
            CompositionDropShadowSourcePolicy, CompositionRoundedRectangleGeometry, Compositor,
            Desktop::DesktopWindowTarget, DropShadow, LayerVisual, ShapeVisual, SpriteVisual,
        },
    },
    Win32::{
        Foundation::{E_INVALIDARG, E_POINTER, HWND},
        Graphics::{
            Direct2D::{CLSID_D2D1Blend, Common::D2D1_BLEND_MODE_COLOR},
            Dwm::{DWMWA_USE_HOSTBACKDROPBRUSH, DwmSetWindowAttribute},
        },
        System::WinRT::{
            Composition::ICompositorDesktopInterop,
            CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT,
            DispatcherQueueOptions,
            Graphics::Direct2D::{
                GRAPHICS_EFFECT_PROPERTY_MAPPING, GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT,
                IGraphicsEffectD2D1Interop, IGraphicsEffectD2D1Interop_Impl,
            },
        },
    },
    core::{GUID, HSTRING, Interface, PCWSTR, implement},
};

/// Owns the thread's dispatcher queue only if we had to create it. GPUI
/// initialized OLE on this thread; the queue must not change its apartment.
struct QueueOwner(Option<DispatcherQueueController>);

impl QueueOwner {
    fn new() -> Result<Self> {
        if DispatcherQueue::GetForCurrentThread().is_ok() {
            return Ok(Self(None));
        }
        let options = DispatcherQueueOptions {
            dwSize: size_of::<DispatcherQueueOptions>() as u32,
            threadType: DQTYPE_THREAD_CURRENT,
            apartmentType: DQTAT_COM_NONE,
        };
        Ok(Self(Some(unsafe { CreateDispatcherQueueController(options)? })))
    }
}

impl Drop for QueueOwner {
    fn drop(&mut self) {
        if let Some(controller) = &self.0 {
            let _ = controller.ShutdownQueueAsync();
        }
    }
}

/// What shows through the glass.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Material {
    /// The blurred desktop wallpaper.
    Mica,
    /// Whatever is on screen behind the window, blurred.
    Acrylic,
}

pub(crate) struct Backdrop {
    // Keep the lower target and its compositor alive as long as the window.
    target: DesktopWindowTarget,
    compositor: Compositor,
    visual: SpriteVisual,
    geometry: CompositionRoundedRectangleGeometry,
    luminosity: CompositionColorBrush,
    /// The glass's silhouette under the glass itself; only its drop shadow
    /// shows, so the shadow never darkens the material.
    shadow_layer: LayerVisual,
    shadow_shape: ShapeVisual,
    shadow: DropShadow,
    shadow_last: Option<[f32; 3]>,
    _queue: QueueOwner,
    opacity: f32,
    level: u8,
    failed: bool,
    last: Option<[f32; 5]>,
}

impl Backdrop {
    /// Fails where the system has no such brush (Mica on Windows 10).
    pub(crate) fn new(hwnd: HWND, luminosity: u8, material: Material) -> Result<Self> {
        let level = luminosity;
        // Lets brushes reach the desktop; not a window-wide material.
        let enabled = 1i32;
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_HOSTBACKDROPBRUSH,
                (&enabled as *const i32).cast(),
                size_of::<i32>() as u32,
            )
        }
        .context("enabling desktop access for the backdrop brush")?;

        let queue = QueueOwner::new().context("creating backdrop dispatcher queue")?;
        let compositor = Compositor::new().context("creating backdrop compositor")?;
        let interop: ICompositorDesktopInterop = compositor.cast()?;
        // false = the lower slot; GPUI holds the topmost one.
        let target = unsafe { interop.CreateDesktopWindowTarget(hwnd, false) }
            .context("attaching backdrop below GPUI")?;

        let wallpaper: CompositionBrush = match material {
            Material::Mica => compositor
                .TryCreateBlurredWallpaperBackdropBrush()
                .context("creating blurred wallpaper brush")?
                .cast()?,
            Material::Acrylic => compositor
                .CreateHostBackdropBrush()
                .context("creating host backdrop brush")?
                .cast()?,
        };
        let wallpaper_name = HSTRING::from("wallpaper");
        let luminosity_name = HSTRING::from("luminosity");
        let effect = luminosity_blend(
            CompositionEffectSourceParameter::Create(&wallpaper_name)?.cast()?,
            CompositionEffectSourceParameter::Create(&luminosity_name)?.cast()?,
        );
        let factory = compositor
            .CreateEffectFactory(&effect)
            .context("creating Mica luminosity effect")?;
        let brush = factory.CreateBrush()?;
        brush.SetSourceParameter(&wallpaper_name, &wallpaper)?;
        let luminosity = compositor.CreateColorBrushWithColor(gray(luminosity))?;
        brush.SetSourceParameter(&luminosity_name, &luminosity)?;

        let geometry = compositor.CreateRoundedRectangleGeometry()?;
        let clip = compositor.CreateGeometricClipWithGeometry(&geometry)?;
        let visual = compositor.CreateSpriteVisual()?;
        visual.SetBrush(&brush)?;
        visual.SetClip(&clip)?;
        visual.SetOpacity(0.0)?;

        // A layer flattens the silhouette so the shadow takes its rounded
        // shape. Hidden until `set_shadow` asks for one.
        let shadow = compositor.CreateDropShadow()?;
        shadow.SetColor(Color { A: 255, R: 0, G: 0, B: 0 })?;
        shadow.SetSourcePolicy(CompositionDropShadowSourcePolicy::InheritFromVisualContent)?;
        let silhouette = compositor.CreateSpriteShapeWithGeometry(&geometry)?;
        silhouette.SetFillBrush(&luminosity)?;
        let shadow_shape = compositor.CreateShapeVisual()?;
        shadow_shape.Shapes()?.Append(&silhouette)?;
        let shadow_layer = compositor.CreateLayerVisual()?;
        shadow_layer.Children()?.InsertAtTop(&shadow_shape)?;
        shadow_layer.SetShadow(&shadow)?;
        shadow_layer.SetOpacity(0.0)?;
        shadow_layer.SetIsVisible(false)?;

        let root = compositor.CreateContainerVisual()?;
        root.Children()?.InsertAtTop(&shadow_layer)?;
        root.Children()?.InsertAtTop(&visual)?;
        target.SetRoot(&root)?;

        Ok(Self {
            target,
            compositor,
            visual,
            geometry,
            luminosity,
            shadow_layer,
            shadow_shape,
            shadow,
            shadow_last: None,
            _queue: queue,
            opacity: 0.0,
            level,
            failed: false,
            last: None,
        })
    }

    pub(crate) fn usable(&self) -> bool {
        !self.failed
    }

    /// Lightness the wallpaper is blended to (its hue stays).
    pub(crate) fn set_luminosity(&mut self, level: u8) {
        if self.failed || self.level == level {
            return;
        }
        self.level = level;
        if self.luminosity.SetColor(gray(level)).is_err() {
            self.fail();
        }
    }

    /// Places the glass at `x, y, width, height` (physical pixels, window
    /// coordinates) with `radius` corners, at `opacity`.
    pub(crate) fn update(&mut self, [x, y, width, height]: [f32; 4], radius: f32, opacity: f32) {
        if self.failed {
            return;
        }
        let bounds = [x, y, width, height, radius];
        if self.last != Some(bounds) {
            let size = Vector2 { X: width, Y: height };
            let radius = radius.max(0.0).min(width * 0.5).min(height * 0.5);
            let result = (|| -> windows::core::Result<()> {
                self.geometry.SetSize(size)?;
                self.geometry.SetCornerRadius(Vector2 { X: radius, Y: radius })?;
                self.visual.SetSize(size)?;
                self.visual.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
                self.shadow_shape.SetSize(size)?;
                self.shadow_shape.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
                // A layer clips its shadow to its own bounds: give it the
                // glass plus the same margin on every side.
                self.shadow_layer.SetSize(Vector2 {
                    X: width + x.max(0.0) * 2.0,
                    Y: height + y.max(0.0) * 2.0,
                })
            })();
            if result.is_err() {
                self.fail();
                return;
            }
            self.last = Some(bounds);
        }
        self.set_opacity(opacity);
    }

    pub(crate) fn set_opacity(&mut self, opacity: f32) {
        let opacity = opacity.clamp(0.0, 1.0);
        if self.failed || (self.opacity - opacity).abs() < 0.002 {
            return;
        }
        self.opacity = opacity;
        if self.visual.SetOpacity(opacity).is_err()
            || self.shadow_layer.SetOpacity(opacity).is_err()
        {
            self.fail();
        }
    }

    /// Drop shadow around the glass: `offset_y` and the blur's standard
    /// `deviation` in physical pixels, black at `opacity`. Zero opacity
    /// removes it.
    pub(crate) fn set_shadow(&mut self, offset_y: f32, deviation: f32, opacity: f32) {
        let opacity = opacity.clamp(0.0, 1.0);
        let shadow = [offset_y, deviation, opacity];
        if self.failed || self.shadow_last == Some(shadow) {
            return;
        }
        let result = (|| -> windows::core::Result<()> {
            self.shadow.SetOffset(Vector3 { X: 0.0, Y: offset_y, Z: 0.0 })?;
            // Composition's blur radius is three standard deviations.
            self.shadow.SetBlurRadius(deviation.max(0.0) * 3.0)?;
            // A layer's shadow ignores `DropShadow::Opacity`; the color's
            // alpha does the job.
            self.shadow.SetColor(Color {
                A: (opacity * 255.0).round() as u8,
                R: 0,
                G: 0,
                B: 0,
            })?;
            self.shadow_layer.SetIsVisible(opacity > 0.0)
        })();
        if result.is_err() {
            self.fail();
            return;
        }
        self.shadow_last = Some(shadow);
    }

    fn fail(&mut self) {
        // Detach rather than leave a stale glass rectangle behind.
        self.failed = true;
        let _ = self.target.SetRoot(None::<&windows::UI::Composition::Visual>);
    }
}

impl Drop for Backdrop {
    fn drop(&mut self) {
        let _ = self.target.Close();
        let _ = self.compositor.Close();
    }
}

fn gray(level: u8) -> Color {
    Color {
        A: 255,
        R: level,
        G: level,
        B: level,
    }
}

/// Direct2D Blend (COLOR mode) for Windows Composition, without Win2D: the
/// wallpaper's hue and saturation at the luminosity color's lightness.
#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
struct LuminosityBlend {
    name: std::sync::Mutex<HSTRING>,
    sources: Vec<IGraphicsEffectSource>,
}

fn luminosity_blend(backdrop: IGraphicsEffectSource, luminosity: IGraphicsEffectSource) -> IGraphicsEffect {
    LuminosityBlend {
        name: std::sync::Mutex::new(HSTRING::from("MicaLuminosity")),
        sources: vec![backdrop, luminosity],
    }
    .into()
}

impl IGraphicsEffectSource_Impl for LuminosityBlend_Impl {}

impl IGraphicsEffect_Impl for LuminosityBlend_Impl {
    fn Name(&self) -> windows::core::Result<HSTRING> {
        Ok(self.name.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    fn SetName(&self, name: &HSTRING) -> windows::core::Result<()> {
        *self.name.lock().unwrap_or_else(|e| e.into_inner()) = name.clone();
        Ok(())
    }
}

impl IGraphicsEffectD2D1Interop_Impl for LuminosityBlend_Impl {
    fn GetEffectId(&self) -> windows::core::Result<GUID> {
        Ok(CLSID_D2D1Blend)
    }

    fn GetNamedPropertyMapping(
        &self,
        name: &PCWSTR,
        index: *mut u32,
        mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING,
    ) -> windows::core::Result<()> {
        if name.is_null() || index.is_null() || mapping.is_null() {
            return Err(E_POINTER.into());
        }
        let property = unsafe { name.to_string() }.map_err(|_| windows::core::Error::from(E_INVALIDARG))?;
        if property != "Mode" {
            return Err(E_INVALIDARG.into());
        }
        unsafe {
            index.write(0);
            mapping.write(GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT);
        }
        Ok(())
    }

    fn GetPropertyCount(&self) -> windows::core::Result<u32> {
        Ok(1)
    }

    fn GetProperty(&self, index: u32) -> windows::core::Result<IPropertyValue> {
        if index != 0 {
            return Err(E_INVALIDARG.into());
        }
        // Composition swaps Color and Luminosity: COLOR keeps the backdrop's
        // chroma at the foreground's lightness (Microsoft's Acrylic recipe).
        PropertyValue::CreateUInt32(D2D1_BLEND_MODE_COLOR.0 as u32)?.cast()
    }

    fn GetSource(&self, index: u32) -> windows::core::Result<IGraphicsEffectSource> {
        self.sources
            .get(index as usize)
            .cloned()
            .ok_or_else(|| E_INVALIDARG.into())
    }

    fn GetSourceCount(&self) -> windows::core::Result<u32> {
        Ok(self.sources.len() as u32)
    }
}
