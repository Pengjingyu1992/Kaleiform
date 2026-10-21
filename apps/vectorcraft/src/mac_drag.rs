//! Native image drags using public AppKit APIs. winit retains ownership of the content view.
#![allow(unsafe_code)]

use std::cell::RefCell;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, SystemTime};

use block2::RcBlock;
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{
    NSDragOperation, NSDraggingContext, NSDraggingItem, NSDraggingSession, NSDraggingSource, NSEvent, NSEventMask, NSImage, NSPasteboardItem, NSView,
};
use objc2_foundation::{NSArray, NSData, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL};
use serde_json::Value;
use vectorcraft_ui_egui::image_transfer::BeginDrag;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

static FINISHED: AtomicBool = AtomicBool::new(false);

define_class!(
    #[unsafe(super = NSObject)]
    #[name = "KaleiformImageDragSource"]
    #[thread_kind = MainThreadOnly]
    #[ivars = egui::Context]
    struct DragSource;
    unsafe impl NSObjectProtocol for DragSource {}
    unsafe impl NSDraggingSource for DragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn operation(&self, _session: &NSDraggingSession, _context: NSDraggingContext) -> NSDragOperation {
            NSDragOperation::Copy
        }
        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn ended(&self, _session: &NSDraggingSession, _point: NSPoint, _operation: NSDragOperation) {
            FINISHED.store(true, Ordering::Release);
            self.ivars().request_repaint();
        }
    }
);

struct Host {
    _window: Arc<winit::window::Window>,
    down: Rc<RefCell<Option<Retained<NSEvent>>>>,
    monitor: Retained<objc2::runtime::AnyObject>,
    source: Retained<DragSource>,
}

impl DragSource {
    fn new(mtm: MainThreadMarker, ctx: egui::Context) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ctx);
        // Initialize NSObject after installing this subclass's Rust ivars.
        unsafe { msg_send![super(this), init] }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // This token came from addLocalMonitor and is only removed on the main thread.
        unsafe { NSEvent::removeMonitor(&self.monitor) };
    }
}

impl Host {
    fn attach(window: Arc<winit::window::Window>, ctx: egui::Context) -> Result<Self, String> {
        let mtm = MainThreadMarker::new().ok_or("image drags require the main thread")?;
        let down = Rc::new(RefCell::new(None));
        let save = down.clone();
        let block = RcBlock::new(move |event: NonNull<NSEvent>| {
            // AppKit lends a live event for this callback; retain it for the upcoming egui drag.
            let e = unsafe { event.as_ref() };
            let _ = vectorcraft_engine::guard::catch_panic(|| *save.borrow_mut() = Some(e.retain()));
            event.as_ptr()
        });
        // Returning the input event unchanged leaves winit's mouse and IME handling intact.
        let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::LeftMouseDown, &block) }
            .ok_or("couldn't observe drag mouse events")?;
        let source = DragSource::new(mtm, ctx);
        Ok(Self { _window: window, down, monitor, source })
    }

    fn begin(&self, png: &[u8], placement: &Value) -> Result<(), String> {
        let mtm = MainThreadMarker::new().ok_or("image drags require the main thread")?;
        let event = self.down.borrow_mut().take().ok_or("start the drag with the mouse")?;
        // Properties may live in a detached native viewport. Use the initiating event's view.
        let view = event.window(mtm).and_then(|w| w.contentView()).ok_or("drag window has no content view")?;
        let file = write_transfer(png, placement)?;
        let url = NSURL::from_file_path(&file).ok_or("invalid image transfer path")?;
        let item = NSPasteboardItem::new();
        let url_string = url.absoluteString().ok_or("invalid image transfer URL")?;
        if !item.setString_forType(&url_string, &NSString::from_str("public.file-url"))
            || !item.setData_forType(&NSData::with_bytes(png), &NSString::from_str("public.png"))
            || !item.setData_forType(
                &NSData::with_bytes(&serde_json::to_vec(placement).map_err(|e| e.to_string())?),
                &NSString::from_str("org.kaleiform-compositor.image-placement"),
            )
        {
            return Err("couldn't publish image drag data".into());
        }
        let dragging = NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), ProtocolObject::from_ref(&*item));
        let image = NSImage::initWithContentsOfURL(NSImage::alloc(), &url).ok_or("couldn't make an image drag preview")?;
        let point = view.convertPoint_fromView(event.locationInWindow(), None);
        // This is only the cursor preview. The pasteboard still contains the complete PNG.
        unsafe {
            dragging.setDraggingFrame_contents(NSRect::new(point, NSSize::new(64.0, 64.0)), Some(&image));
        }
        view.beginDraggingSessionWithItems_event_source(&NSArray::from_retained_slice(&[dragging]), &event, ProtocolObject::from_ref(&*self.source));
        Ok(())
    }
}

fn view(window: &winit::window::Window) -> Option<&NSView> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return None };
    // The borrowed live winit window owns this view; access is restricted to the UI thread.
    Some(unsafe { handle.ns_view.cast::<NSView>().as_ref() })
}

type DropPosition = Box<dyn FnMut() -> Option<[f32; 2]>>;

pub fn services(window: Option<Arc<winit::window::Window>>, ctx: egui::Context) -> (Option<BeginDrag>, Option<DropPosition>) {
    let Some(window) = window else { return (None, None) };
    let host = match Host::attach(window.clone(), ctx) {
        Ok(host) => host,
        Err(e) => {
            log::warn!("native image drag unavailable: {e}");
            return (None, None);
        }
    };
    let begin: BeginDrag = Box::new(move |png, placement| host.begin(png, placement));
    let position: DropPosition = Box::new(move || {
        let view = view(&window)?;
        let point = view.window()?.convertPointFromScreen(NSEvent::mouseLocation());
        let point = view.convertPoint_fromView(point, None);
        let y = if view.isFlipped() { point.y } else { view.bounds().size.height - point.y };
        Some([point.x as f32, y as f32])
    });
    (Some(begin), Some(position))
}

/// AppKit consumes the release during a native drag (including Escape). Release egui's handle too.
pub fn finish_input(ctx: &egui::Context, raw: &mut egui::RawInput) {
    if FINISHED.swap(false, Ordering::AcqRel) {
        raw.events.push(egui::Event::PointerButton {
            pos: ctx.pointer_latest_pos().unwrap_or_default(),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        });
    }
}

fn write_transfer(png: &[u8], placement: &Value) -> Result<std::path::PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let p: vectorcraft_engine::cmd::image_transfer::Placement = serde_json::from_value(placement.clone()).map_err(|e| e.to_string())?;
    if !p.valid() || png.is_empty() || png.len() > 512 * 1024 * 1024 {
        return Err("invalid image transfer".into());
    }
    let root = std::env::temp_dir();
    let prefix = vectorcraft_engine::cmd::image_transfer::FOLDER_PREFIX;
    // Delayed cleanup lets the receiving app finish importing after this drag session ends.
    if let Ok(entries) = std::fs::read_dir(&root) {
        for e in entries.flatten().filter(|e| e.file_name().to_string_lossy().starts_with(prefix)) {
            if e.file_type().is_ok_and(|t| t.is_dir())
                && e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > Duration::from_secs(86400))
                && let Err(e) = std::fs::remove_dir_all(e.path())
            {
                log::debug!("expired image transfer: {e}");
            }
        }
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let folder = root.join(format!("{prefix}{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let write = || -> Result<_, String> {
        std::fs::create_dir(&folder).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        let file = folder.join("image.png");
        std::fs::write(&file, png).map_err(|e| e.to_string())?;
        std::fs::write(folder.join("transfer.json"), serde_json::to_vec(&p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        Ok(file)
    };
    let result = write();
    if result.is_err()
        && folder.is_dir()
        && let Err(e) = std::fs::remove_dir_all(&folder)
    {
        log::debug!("failed image transfer cleanup: {e}");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn transfer_file_keeps_exact_png_and_private_placement() {
        use std::os::unix::fs::PermissionsExt;
        let mut bytes = vec![];
        image::RgbaImage::from_pixel(16, 12, image::Rgba([200, 50, 100, 128]))
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        let path = write_transfer(&bytes, &json!({"version":1,"width":16,"height":12})).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let folder = path.parent().unwrap();
        assert_eq!(std::fs::metadata(folder).unwrap().permissions().mode() & 0o777, 0o700);
        let placement: Value = serde_json::from_slice(&std::fs::read(folder.join("transfer.json")).unwrap()).unwrap();
        assert_eq!(placement["width"].as_f64(), Some(16.0));
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn native_completion_releases_the_egui_drag_after_cancel_or_drop() {
        FINISHED.store(true, Ordering::Release);
        let mut raw = egui::RawInput::default();
        finish_input(&egui::Context::default(), &mut raw);
        assert!(raw.events.iter().any(|e| matches!(e, egui::Event::PointerButton { pressed: false, .. })));
        raw.events.clear();
        finish_input(&egui::Context::default(), &mut raw);
        assert!(raw.events.is_empty());
    }
}
