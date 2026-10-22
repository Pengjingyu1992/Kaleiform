//! Public AppKit material behind, not over, winit's Metal view.
#![allow(unsafe_code)]

use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{
    NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSAutoresizingMaskOptions, NSColor, NSColorSpace, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowButton, NSWindowOrderingMode, NSWorkspace,
};
use objc2_foundation::NSPoint;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub struct Material(Retained<NSVisualEffectView>);

pub fn appearance(dark: bool) -> vectorcraft_ui_egui::theme::HostAppearance {
    let mut accent = [10, 132, 255];
    if let Some(color) = NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace()) {
        accent = [color.redComponent(), color.greenComponent(), color.blueComponent()].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    }
    let workspace = NSWorkspace::sharedWorkspace();
    let colors = std::cell::Cell::new(None);
    let name = unsafe { if dark { NSAppearanceNameDarkAqua } else { NSAppearanceNameAqua } };
    if let Some(appearance) = NSAppearance::appearanceNamed(name) {
        appearance.performAsCurrentDrawingAppearance(&block2::StackBlock::new(|| {
            let rgba = |color: Retained<NSColor>| {
                color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace()).map(|c| {
                    [c.redComponent(), c.greenComponent(), c.blueComponent(), c.alphaComponent()].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                })
            };
            let read = || {
                Some(vectorcraft_ui_egui::theme::HostColors {
                    window: rgba(NSColor::windowBackgroundColor())?,
                    control: rgba(NSColor::controlBackgroundColor())?,
                    label: rgba(NSColor::labelColor())?,
                    secondary: rgba(NSColor::secondaryLabelColor())?,
                    disabled: rgba(NSColor::disabledControlTextColor())?,
                    separator: rgba(NSColor::separatorColor())?,
                    under_page: rgba(NSColor::underPageBackgroundColor())?,
                })
            };
            colors.set(read());
        }));
    }
    vectorcraft_ui_egui::theme::HostAppearance {
        accent,
        colors: colors.get(),
        reduce_transparency: workspace.accessibilityDisplayShouldReduceTransparency(),
        reduce_motion: workspace.accessibilityDisplayShouldReduceMotion(),
    }
}

/// Centre the existing system buttons in the app's toolbar; only this window is changed.
pub fn align_buttons(window: &winit::window::Window, ui_zoom: f32, visible: bool) {
    let Ok(handle) = window.window_handle() else { return };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return };
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let Some(window) = view.window() else { return };
    for kind in [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton] {
        if let Some(button) = window.standardWindowButton(kind) {
            button.setHidden(!visible);
            if !visible {
                continue;
            }
            let frame = button.frame();
            // The retained parent is owned by this live NSWindow.
            let Some(parent) = (unsafe { button.superview() }) else { continue };
            // Titlebar views use AppKit's bottom-left coordinates.
            let toolbar_height = f64::from(vectorcraft_ui_egui::workspace::BAR_HEIGHT * ui_zoom);
            let y = parent.frame().size.height - (toolbar_height + frame.size.height) / 2.0;
            if (frame.origin.y - y).abs() > 0.1 {
                button.setFrameOrigin(NSPoint::new(frame.origin.x, y));
            }
        }
    }
}

impl Material {
    pub fn attach(window: &winit::window::Window) -> Result<Self, String> {
        let mtm = MainThreadMarker::new().ok_or("window materials require the main thread")?;
        let handle = window.window_handle().map_err(|e| e.to_string())?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err("window has no AppKit view".into());
        };
        // winit owns this NSView for the borrowed window's lifetime; AppKit access is main-thread only.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        // The parent remains owned by the live NSWindow and is retained by this call.
        let parent = unsafe { view.superview() }.ok_or("window view has no parent")?;
        let effect = NSVisualEffectView::initWithFrame(mtm.alloc(), view.frame());
        effect.setMaterial(NSVisualEffectMaterial::Sidebar);
        effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        effect.setState(NSVisualEffectState::FollowsWindowActiveState);
        effect.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
        // A child of the Metal view would cover its rendered contents. A sibling leaves input and IME ownership unchanged.
        parent.addSubview_positioned_relativeTo(&effect, NSWindowOrderingMode::Below, Some(view));
        Ok(Self(effect))
    }
}

impl Drop for Material {
    fn drop(&mut self) {
        self.0.removeFromSuperview();
    }
}
