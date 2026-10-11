//! Public AppKit material behind, not over, winit's Metal view.
#![allow(unsafe_code)]

use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindowOrderingMode,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub struct Material(Retained<NSVisualEffectView>);

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
