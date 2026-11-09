//! Master switches for the Microphone and Volume halves of the app.
//!
//! Backed by `config.modules`; every change is saved and applied immediately.

use super::{
    SettingsApp,
    anim::{self, Easing, Transition, UI},
    color::{TRANSPARENT, lerp},
};
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Styled,
    Window, div, prelude::*, px, relative,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModuleId {
    Microphone,
    Volume,
}

impl ModuleId {
    pub fn name(self) -> &'static str {
        match self {
            Self::Microphone => "Microphone",
            Self::Volume => "Volume",
        }
    }

    /// Solid Solar glyph for the sidebar group header.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Microphone => "microphone-bold",
            Self::Volume => "volume-loud-bold",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Microphone => 0,
            Self::Volume => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ModuleFlags {
    pub microphone: bool,
    pub volume: bool,
}

impl ModuleFlags {
    pub fn get(self, module: ModuleId) -> bool {
        match module {
            ModuleId::Microphone => self.microphone,
            ModuleId::Volume => self.volume,
        }
    }

    pub fn enabled_count(self) -> usize {
        usize::from(self.microphone) + usize::from(self.volume)
    }

    /// An enabled module can be switched off only while another one stays on.
    pub fn can_disable(self, module: ModuleId) -> bool {
        !self.get(module) || self.enabled_count() > 1
    }

    /// Returns false when turning this module off would leave none enabled.
    pub fn set(&mut self, module: ModuleId, enabled: bool) -> bool {
        if !enabled && !self.can_disable(module) {
            return false;
        }
        match module {
            ModuleId::Microphone => self.microphone = enabled,
            ModuleId::Volume => self.volume = enabled,
        }
        true
    }
}

pub(crate) fn flags(app: &SettingsApp) -> ModuleFlags {
    let modules = &app.snapshot.config.modules;
    ModuleFlags {
        microphone: modules.microphone,
        volume: modules.volume,
    }
}

/// Saves the switch to `config.modules`. Returns false (and changes nothing)
/// when it would turn the last enabled module off.
pub(crate) fn set_enabled(
    app: &mut SettingsApp,
    module: ModuleId,
    enabled: bool,
    cx: &mut Context<SettingsApp>,
) -> bool {
    let mut next = flags(app);
    if !next.set(module, enabled) {
        return false;
    }
    app.update_config(cx, |config| {
        config.modules.microphone = next.microphone;
        config.modules.volume = next.volume;
    });
    true
}

const BANNER: Transition = Transition::new(220, Easing::EaseInOut);
const BANNER_H: f32 = 44.;

/// Soft notice shown on a module's pages while that module is switched off.
/// Height collapses so the page title slides up instead of jumping.
pub(crate) fn off_banner(
    app: &mut SettingsApp,
    module: ModuleId,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Option<AnyElement> {
    let shown = !flags(app).get(module);
    let t = anim::tween(
        window,
        cx,
        ("module-off-banner", module.index()),
        shown as u8 as f32,
        BANNER,
    );
    if t <= 0.001 {
        return None;
    }
    let palette = app.palette;
    let pointer = anim::pointer(window, cx, ("module-off-turn-on", module.index()));
    let hovered = pointer.read(cx).hovered;
    let hover = anim::tween(
        window,
        cx,
        ("module-off-turn-on-hover", module.index()),
        hovered as u8 as f32,
        UI,
    );
    let name = module.name();
    Some(
        div()
            .w_full()
            .flex_shrink_0()
            .h(px(BANNER_H * t))
            .mb(px(16. * t))
            .overflow_hidden()
            .opacity(t)
            .child(
                div()
                    .relative()
                    .top(px(-6. * (1. - t)))
                    .h(px(BANNER_H))
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(14.))
                    .rounded(px(10.))
                    .bg(palette.accent_soft.hsla())
                    .text_size(px(13.5))
                    .line_height(relative(1.3))
                    .child(
                        div()
                            .text_color(palette.text.hsla())
                            .child(format!("{name} is turned off —")),
                    )
                    .child(
                        div()
                            .id(("module-off-turn-on", module.index()))
                            .px(px(6.))
                            .py(px(2.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .font_weight(FontWeight(650.))
                            .text_color(palette.accent_text.hsla())
                            .bg(lerp(TRANSPARENT, palette.accent, 0.2 * hover).hsla())
                            .child("turn on")
                            .on_hover(move |hovered, _, cx| {
                                anim::set_hovered(&pointer, *hovered, cx)
                            })
                            .on_click(cx.listener(move |app, _, _, cx| {
                                if set_enabled(app, module, true, cx) {
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .into_any_element(),
    )
}
