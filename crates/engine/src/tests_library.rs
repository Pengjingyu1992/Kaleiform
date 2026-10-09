//! The Libraries panel's libraries (#745): making, renaming and deleting them, adding graphics,
//! colours and text styles from the selection, using them in another document, and keeping them
//! in files across sessions.

use serde_json::{Value, json};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
    s
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

fn objects(s: &Session) -> usize {
    s.doc().unwrap().doc.layers[0].children().map_or(0, |c| c.len())
}

fn hex_of(s: &Session, id: &Value, stroke: bool) -> String {
    let n = s.doc().unwrap().doc.node(NodeId(id.as_u64().unwrap())).unwrap().clone();
    let paint = if stroke { n.appearance.stroke().unwrap().paint.clone() } else { n.appearance.fill().unwrap().paint.clone() };
    paint.color().unwrap().to_hex()
}

#[test]
fn libraries_are_made_renamed_and_deleted() {
    let mut s = session();
    assert_eq!(run(&mut s, "library.list", json!({}))["libraries"], json!([]));
    let id = run(&mut s, "library.create", json!({"name": "Brand"}))["id"].as_str().unwrap().to_string();
    let other = run(&mut s, "library.create", json!({"name": "Brand"}))["id"].as_str().unwrap().to_string();
    assert_ne!(id, other, "two libraries of one name are two files");
    run(&mut s, "library.rename", json!({"library": other, "name": "Client"}));
    let list = run(&mut s, "library.list", json!({}));
    let names: Vec<&str> = list["libraries"].as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Brand", "Client"]);
    assert!(list["folder"].is_null(), "no folder: libraries last for the session");
    run(&mut s, "library.delete", json!({"library": "client"}));
    assert_eq!(run(&mut s, "library.list", json!({}))["libraries"].as_array().unwrap().len(), 1);
    assert!(s.execute("library.get", &json!({"library": "Client"})).is_err());
}

#[test]
fn a_graphic_goes_into_a_library_and_comes_out_in_another_document() {
    let mut s = session();
    let id = run(&mut s, "shape.ellipse", json!({"x": 10, "y": 10, "width": 40, "height": 30}))["id"].clone();
    run(&mut s, "paint.setFill", json!({"color": "#cc3300", "ids": [id]}));
    run(&mut s, "select.set", json!({"ids": [id]}));
    // With no library yet, adding makes "My Library".
    let added = run(&mut s, "library.add", json!({"kind": "graphic"}));
    assert_eq!(added["existing"], false);
    let lib = added["library"].as_str().unwrap().to_string();
    let item = added["id"].as_str().unwrap().to_string();
    let got = run(&mut s, "library.get", json!({"library": lib}));
    assert_eq!(got["name"], "My Library");
    let g = &got["graphics"][0];
    // Its size as it draws: the 1 pt stroke included.
    assert!((g["width"].as_f64().unwrap() - 41.0).abs() < 1e-6 && (g["height"].as_f64().unwrap() - 31.0).abs() < 1e-6, "{g}");
    assert!(!g["thumbnail"].as_str().unwrap().is_empty(), "a thumbnail of the art");
    // The same art again is the item already there.
    assert_eq!(run(&mut s, "library.add", json!({"kind": "graphic"}))["existing"], true);
    // Placed in another document, centred where asked, selected, one undo step.
    run(&mut s, "file.new", json!({"width": 300, "height": 300}));
    let placed = run(&mut s, "library.use", json!({"library": lib, "kind": "graphic", "item": item, "center": [150, 100]}));
    let new_id = placed["ids"][0].clone();
    let st = s.doc().unwrap();
    let b = st.doc.node(NodeId(new_id.as_u64().unwrap())).unwrap().geometric_bounds().unwrap();
    assert!((b.center().x - 150.0).abs() < 1e-6 && (b.center().y - 100.0).abs() < 1e-6, "{b:?}");
    assert_eq!(st.selection.objects, vec![NodeId(new_id.as_u64().unwrap())]);
    assert_eq!(hex_of(&s, &new_id, false), "#cc3300");
    assert_eq!(objects(&s), 1);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(objects(&s), 0);
    // A graphic is removed by id.
    run(&mut s, "library.removeItem", json!({"library": lib, "kind": "graphic", "item": item}));
    assert!(s.execute("library.use", &json!({"library": lib, "kind": "graphic", "item": item})).is_err());
}

#[test]
fn colours_come_from_the_selection_and_paint_it() {
    let mut s = session();
    let lib = run(&mut s, "library.create", json!({}))["id"].as_str().unwrap().to_string();
    let a = run(&mut s, "shape.rectangle", json!({"x": 0, "y": 0, "width": 20, "height": 20}))["id"].clone();
    run(&mut s, "paint.setFill", json!({"color": "#123456", "ids": [a]}));
    run(&mut s, "paint.setStroke", json!({"color": "#abcdef", "ids": [a]}));
    run(&mut s, "select.set", json!({"ids": [a]}));
    assert_eq!(run(&mut s, "library.add", json!({"library": lib, "kind": "fillColor"}))["name"], "#123456");
    assert_eq!(run(&mut s, "library.add", json!({"library": lib, "kind": "strokeColor"}))["name"], "#abcdef");
    assert_eq!(run(&mut s, "library.add", json!({"library": lib, "kind": "fillColor"}))["existing"], true);
    let b = run(&mut s, "shape.rectangle", json!({"x": 50, "y": 0, "width": 20, "height": 20}))["id"].clone();
    run(&mut s, "select.set", json!({"ids": [b]}));
    run(&mut s, "library.use", json!({"library": lib, "kind": "fillColor", "item": "#123456"}));
    run(&mut s, "library.use", json!({"library": lib, "kind": "fillColor", "item": "#123456", "to": "stroke"}));
    assert_eq!(hex_of(&s, &b, false), "#123456");
    assert_eq!(hex_of(&s, &b, true), "#123456");
    assert!(s.execute("library.use", &json!({"library": lib, "kind": "fillColor", "item": "#nope"})).is_err());
}

#[test]
fn text_styles_come_from_the_selected_type_and_apply_in_one_step() {
    let mut s = session();
    let lib = run(&mut s, "library.create", json!({}))["id"].as_str().unwrap().to_string();
    assert!(s.execute("library.add", &json!({"library": lib, "kind": "charStyle"})).is_err(), "no type selected");
    let t = run(&mut s, "text.create", json!({"x": 10, "y": 50, "text": "Heading", "size": 31}))["id"].clone();
    run(&mut s, "select.set", json!({"ids": [t]}));
    let name = run(&mut s, "library.add", json!({"library": lib, "kind": "charStyle"}))["name"].as_str().unwrap().to_string();
    assert!(name.ends_with("31 pt"), "{name}");
    run(&mut s, "library.add", json!({"library": lib, "kind": "paraStyle"}));
    // Another document: the style is added and applied, and one undo takes both back.
    run(&mut s, "file.new", json!({"width": 200, "height": 200}));
    let u = run(&mut s, "text.create", json!({"x": 10, "y": 50, "text": "Body", "size": 9}))["id"].clone();
    run(&mut s, "select.set", json!({"ids": [u]}));
    let undo = s.doc().unwrap().history.undo.len();
    let r = run(&mut s, "library.use", json!({"library": lib, "kind": "charStyle", "item": name}));
    assert_eq!(r["style"], name);
    assert_eq!(s.doc().unwrap().history.undo.len(), undo + 1, "one undo step");
    let size = |s: &Session| match &s.doc().unwrap().doc.node(NodeId(u.as_u64().unwrap())).unwrap().kind {
        vectorcraft_doc::NodeKind::Text(t) => t.runs[0].style.size,
        _ => 0.0,
    };
    assert_eq!(size(&s), 31.0);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(size(&s), 9.0);
    assert!(s.doc().unwrap().doc.char_styles.iter().all(|c| c.name != name), "the style went with the undo");
    // A style of that name with other attributes in the document: the library's comes in numbered.
    run(&mut s, "charStyle.new", json!({"name": name, "attrs": {"size": 5}}));
    let r = run(&mut s, "library.use", json!({"library": lib, "kind": "charStyle", "item": name}));
    assert_eq!(r["style"], format!("{name} 2"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn libraries_are_kept_in_files_across_sessions() {
    let dir = std::env::temp_dir().join(format!("vc-libraries-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let folder = dir.to_string_lossy().to_string();
    let mut s = session();
    s.libraries.set_dir(Some(folder.clone()));
    let lib = run(&mut s, "library.create", json!({"name": "Kept"}))["id"].as_str().unwrap().to_string();
    let id = run(&mut s, "shape.rectangle", json!({"x": 0, "y": 0, "width": 10, "height": 10}))["id"].clone();
    run(&mut s, "select.set", json!({"ids": [id]}));
    run(&mut s, "library.add", json!({"library": lib, "kind": "graphic", "name": "Square"}));
    run(&mut s, "library.add", json!({"library": lib, "kind": "fillColor"}));
    let mut t = session();
    t.libraries.set_dir(Some(folder.clone()));
    let got = run(&mut t, "library.get", json!({"library": "Kept"}));
    assert_eq!(got["graphics"][0]["name"], "Square");
    assert_eq!(got["colors"].as_array().unwrap().len(), 1);
    run(&mut t, "library.delete", json!({"library": "Kept"}));
    let mut u = session();
    u.libraries.set_dir(Some(folder));
    assert_eq!(run(&mut u, "library.list", json!({}))["libraries"], json!([]), "deleting removes the file");
    // A damaged file is skipped.
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Bad.vclibrary"), b"{not json").unwrap();
    let mut v = session();
    v.libraries.set_dir(Some(dir.to_string_lossy().to_string()));
    assert_eq!(run(&mut v, "library.list", json!({}))["libraries"], json!([]));
    let _ = std::fs::remove_dir_all(&dir);
}
