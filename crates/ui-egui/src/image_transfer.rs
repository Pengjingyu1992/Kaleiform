//! A dedicated drag handle, independent of canvas moves and layer reordering.
use crate::VectorcraftApp;
use serde_json::{Value, json};

pub type BeginDrag = Box<dyn FnMut(&[u8], &Value) -> Result<(), String>>;

pub fn handle(app: &mut VectorcraftApp, ui: &mut egui::Ui) {
    if app.services.begin_image_drag.is_none() {
        return;
    }
    let enabled = vectorcraft_engine::find_command("image.transfer").is_some_and(|c| (c.enabled)(&app.session).is_ok());
    let response = ui.add_enabled(enabled, egui::Button::new(tl!("Drag image to another app")).sense(egui::Sense::drag()));
    if response.drag_started() {
        let result = (|| -> Result<(), String> {
            let value = app.run("image.transfer", json!({})).map_err(|e| e.to_string())?;
            let bytes = vectorcraft_format::base64_decode(value["dataBase64"].as_str().ok_or("no PNG was generated")?).ok_or("invalid PNG data")?;
            let begin = app.services.begin_image_drag.as_mut().ok_or("native image drag is unavailable")?;
            begin(&bytes, &value["placement"])
        })();
        if let Err(e) = result {
            app.status(format!("Couldn't drag image: {e}"));
        }
    }
}

/// Only our exchange folders carry placement. Temporary exchange images are always embedded.
pub fn is_transfer_path(path: &str) -> bool {
    let path = std::path::Path::new(path);
    path.file_name().is_some_and(|n| n == "image.png")
        && path
            .parent()
            .and_then(|p| p.file_name())
            .is_some_and(|n| n.to_string_lossy().starts_with(vectorcraft_engine::cmd::image_transfer::FOLDER_PREFIX))
}

pub fn placement(app: &VectorcraftApp, path: &str) -> Result<Option<Value>, String> {
    if !is_transfer_path(path) {
        return Ok(None);
    }
    let file = std::path::Path::new(path).with_file_name("transfer.json");
    let read = app.services.read_preview.as_ref().ok_or("no bounded image-transfer reader")?;
    let bytes = read(&file.to_string_lossy(), 4096)?;
    let p: vectorcraft_engine::cmd::image_transfer::Placement = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if !p.valid() {
        return Err("invalid image transfer placement".into());
    }
    serde_json::to_value(p).map(Some).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::place::{DropAt, DropTarget};
    use std::sync::Arc;
    use vectorcraft_doc::NodeKind;
    use vectorcraft_engine::Session;
    use vectorcraft_geom::Point;

    #[test]
    fn dragging_the_handle_starts_one_native_transfer() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let services = crate::Services {
            begin_image_drag: Some(Box::new(move |bytes, placement| {
                assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
                assert_eq!(placement["version"], 1);
                observed.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })),
            ..Default::default()
        };
        let mut app = VectorcraftApp::new(Session::new(), services);
        app.run("file.new", json!({"width":200,"height":200})).unwrap();
        let mut png = vec![];
        image::RgbaImage::from_pixel(16, 12, image::Rgba([100, 50, 200, 128]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        app.run("file.place", json!({"name":"Source.png","dataBase64":vectorcraft_format::base64_encode(&png),"link":false})).unwrap();
        let ctx = egui::Context::default();
        let frame = |app: &mut VectorcraftApp, events| {
            let mut out = ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| handle(app, ui));
            out.textures_delta.clear();
            out.shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text().contains("Drag image") => Some(t.visual_bounding_rect().center()),
                    _ => None,
                })
                .unwrap()
        };
        let at = frame(&mut app, vec![]);
        frame(&mut app, vec![egui::Event::PointerMoved(at)]);
        frame(
            &mut app,
            vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }],
        );
        frame(&mut app, vec![egui::Event::PointerMoved(at + egui::vec2(50.0, 30.0))]);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        frame(&mut app, vec![egui::Event::PointerMoved(at + egui::vec2(80.0, 60.0))]);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        frame(
            &mut app,
            vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }],
        );
    }

    #[test]
    fn exchange_drop_embeds_full_bytes_and_uses_shared_placement() {
        let folder = std::env::temp_dir().join(format!("{}ui-test-{}", vectorcraft_engine::cmd::image_transfer::FOLDER_PREFIX, std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("image.png");
        let mut png = vec![];
        image::RgbaImage::from_pixel(6000, 8, image::Rgba([100, 50, 200, 128]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        std::fs::write(&file, &png).unwrap();
        std::fs::write(folder.join("transfer.json"), br#"{"version":1,"width":1500,"height":2,"rotation":30,"flipX":true,"flipY":false}"#).unwrap();
        let services = crate::Services {
            read_preview: Some(Arc::new(|path, max| {
                let b = std::fs::read(path).map_err(|e| e.to_string())?;
                if b.len() as u64 > max { Err("too large".into()) } else { Ok(b) }
            })),
            ..Default::default()
        };
        let mut app = VectorcraftApp::new(Session::new(), services);
        app.run("file.new", json!({"width":800,"height":600})).unwrap();
        let doc = app.session.active().unwrap().uid;
        let path = file.to_string_lossy().to_string();
        crate::place::drop_files(
            &mut app,
            vec![(DropTarget::Place(DropAt { doc, at: Point::new(350.0, 250.0), embed: false }), ("image.png".into(), Some(path), vec![]))],
        );
        let st = app.session.active().unwrap();
        let n = st.doc.node(st.selection.objects[0]).unwrap();
        let NodeKind::Image(i) = &n.kind else { panic!() };
        assert!(i.link.is_none());
        assert_eq!((i.width, i.height), (6000, 8));
        assert!(st.doc.images.get(&i.key).unwrap().bytes.as_ref() == png.as_slice());
        assert!((i.xf * Point::new(3000.0, 4.0)).distance(Point::new(350.0, 250.0)) < 1e-7);
        std::fs::remove_dir_all(folder).unwrap();
        let exported = app.run("image.transfer", json!({})).unwrap();
        assert_eq!(vectorcraft_format::base64_decode(exported["dataBase64"].as_str().unwrap()).unwrap(), png);
    }
}
