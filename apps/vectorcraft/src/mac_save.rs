//! AppKit's stock save panel has extension restrictions, not a format picker. Keep document
//! formats visible in an accessory view and update the actual destination before overwrite checks.
#![allow(unsafe_code)]

use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel};
use objc2_app_kit::{NSModalResponseOK, NSPopUpButton, NSSavePanel, NSTextField, NSView};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL};
use vectorcraft_engine::cmd::fileio::{self, SAVE_FORMATS};
use vectorcraft_ui_egui::{FilePick, i18n};

pub fn supports(pick: &FilePick) -> bool {
    pick.filters.len() > 1
        && pick
            .filters
            .iter()
            .all(|(_, exts)| exts.len() == 1 && exts.first().and_then(|ext| fileio::format(ext)).is_some_and(|f| SAVE_FORMATS.contains(&f.id)))
}

fn name_for_format(name: &str, extension: &str) -> String {
    if name.is_empty() || fileio::extension(name).eq_ignore_ascii_case(extension) {
        name.to_string()
    } else if fileio::format_for_name(name).is_some_and(|f| SAVE_FORMATS.contains(&f.id)) {
        std::path::Path::new(name).with_extension(extension).to_string_lossy().into_owned()
    } else {
        format!("{name}.{extension}")
    }
}

struct FormatIvars {
    panel: Retained<NSSavePanel>,
    extensions: Vec<&'static str>,
}

define_class!(
    // NSObject has no subclassing requirements; the target lives until the modal panel closes.
    #[unsafe(super = NSObject)]
    #[name = "KaleiformSaveFormatTarget"]
    #[thread_kind = MainThreadOnly]
    #[ivars = FormatIvars]
    struct FormatTarget;

    unsafe impl NSObjectProtocol for FormatTarget {}

    impl FormatTarget {
        #[unsafe(method(formatChanged:))]
        fn format_changed(&self, sender: &NSPopUpButton) {
            // AppKit must never receive an unwinding Rust callback.
            let _ = vectorcraft_engine::guard::catch_panic(|| self.apply(sender));
        }
    }
);

impl FormatTarget {
    fn new(mtm: MainThreadMarker, panel: Retained<NSSavePanel>, extensions: Vec<&'static str>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(FormatIvars { panel, extensions });
        // NSObject's designated initializer on our newly allocated subclass.
        unsafe { msg_send![super(this), init] }
    }

    fn apply(&self, sender: &NSPopUpButton) {
        let Some(extension) = usize::try_from(sender.indexOfSelectedItem()).ok().and_then(|i| self.ivars().extensions.get(i)) else { return };
        let panel = &self.ivars().panel;
        let name = name_for_format(&panel.nameFieldStringValue().to_string(), extension);
        // Public AppKit API available on the app's macOS 11 minimum; each entry has one format.
        #[allow(deprecated)]
        panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&[NSString::from_str(extension)])));
        panel.setNameFieldStringValue(&NSString::from_str(&name));
    }
}

pub fn pick(pick: &FilePick) -> Option<String> {
    let mtm = MainThreadMarker::new()?;
    let panel = NSSavePanel::savePanel(mtm);
    panel.setTitle(Some(&NSString::from_str(i18n::t("Save"))));
    panel.setCanCreateDirectories(true);
    panel.setCanSelectHiddenExtension(false);
    panel.setExtensionHidden(false);
    panel.setAllowsOtherFileTypes(false);
    if let Some(url) = pick.folder.as_deref().and_then(|p| NSURL::from_file_path(std::path::Path::new(p))) {
        panel.setDirectoryURL(Some(&url));
    }
    panel.setNameFieldStringValue(&NSString::from_str(&pick.name));

    let accessory = NSView::initWithFrame(mtm.alloc(), NSRect::new(NSPoint::ZERO, NSSize::new(440.0, 38.0)));
    let label = NSTextField::labelWithString(&NSString::from_str(i18n::t("Format:")), mtm);
    label.setFrame(NSRect::new(NSPoint::new(8.0, 9.0), NSSize::new(70.0, 22.0)));
    let menu = NSPopUpButton::initWithFrame_pullsDown(mtm.alloc(), NSRect::new(NSPoint::new(80.0, 6.0), NSSize::new(350.0, 26.0)), false);
    let mut extensions = vec![];
    for (name, exts) in &pick.filters {
        let Some(extension) = exts.first() else { continue };
        menu.addItemWithTitle(&NSString::from_str(&format!("{} (.{extension})", i18n::t(name))));
        extensions.push(*extension);
    }
    let target = FormatTarget::new(mtm, panel.clone(), extensions);
    // NSControl does not retain its target; the local retained target spans runModal below.
    unsafe {
        menu.setTarget(Some(&target));
        menu.setAction(Some(sel!(formatChanged:)));
    }
    menu.selectItemAtIndex(0);
    target.apply(&menu);
    accessory.addSubview(&label);
    accessory.addSubview(&menu);
    panel.setAccessoryView(Some(&accessory));
    let response = panel.runModal();
    // Disconnect before the retained Rust target is dropped.
    unsafe { menu.setTarget(None) };
    (response == NSModalResponseOK).then(|| panel.URL()).flatten()?.to_file_path().map(|p| p.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_get_choices_but_single_format_exports_and_templates_do_not() {
        assert!(supports(&FilePick { filters: fileio::save_filters("ai"), ..Default::default() }));
        assert!(!supports(&FilePick { filters: vec![("EPS", &["eps"])], ..Default::default() }));
        assert!(!supports(&FilePick { filters: vec![("Template", &["vctemplate"])], ..Default::default() }));
        assert!(!supports(&FilePick { filters: vec![("Images", &["png", "jpg"]), ("All", &["*"])], ..Default::default() }));
    }

    #[test]
    fn changing_format_replaces_known_extensions_without_damaging_the_stem() {
        for ext in ["ai", "eps", "pdf", "svg", "svgz", "vectorcraft", "vctemplate", "dxf"] {
            assert_eq!(name_for_format("poster.ai", ext), format!("poster.{ext}"));
        }
        assert_eq!(name_for_format("poster.EPS", "eps"), "poster.EPS");
        assert_eq!(name_for_format("poster.v2", "ai"), "poster.v2.ai");
        assert_eq!(name_for_format("poster.v2.ai", "eps"), "poster.v2.eps");
        assert_eq!(name_for_format("poster", "ai"), "poster.ai");
        assert_eq!(name_for_format("", "eps"), "");
    }
}
