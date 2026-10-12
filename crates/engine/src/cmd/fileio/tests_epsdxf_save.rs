//! Open, edit and save EPS/DXF without silently replacing incomplete imports.

use serde_json::json;
use vectorcraft_doc::NodeKind;

use super::*;

fn art() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 300, "height": 200, "artboards": 2})).unwrap();
    s.execute("paint.setFill", &json!({"none": true})).unwrap();
    s.execute("shape.rectangle", &json!({"x": 20, "y": 30, "width": 100, "height": 50})).unwrap();
    s
}

fn path(dir: &std::path::Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

#[test]
fn epsdxf_save_as_then_edit_and_save_back_to_the_same_file() {
    let dir = vectorcraft_testkit::temp_dir("epsdxf-save-back");
    for f in ["eps", "dxf"] {
        let mut s = art();
        let p = path(&dir, &format!("art.{f}"));
        let r = s.execute("file.saveAs", &json!({"path": p})).unwrap();
        assert_eq!(r["format"], f);
        assert_eq!((s.doc().unwrap().path.as_deref(), s.doc().unwrap().format, s.doc().unwrap().is_dirty()), (Some(p.as_str()), f, false));
        let before = std::fs::read(&p).unwrap();
        s.execute("shape.ellipse", &json!({"x": 160, "y": 40, "width": 30, "height": 20})).unwrap();
        assert!(s.doc().unwrap().is_dirty());
        s.execute("document.save", &json!({})).unwrap();
        assert!(!s.doc().unwrap().is_dirty());
        assert_ne!(std::fs::read(&p).unwrap(), before);
        let r = s.execute("document.open", &json!({"path": p})).unwrap();
        assert_eq!((r["format"].as_str(), s.doc().unwrap().format), (Some(f), f));
        assert_eq!(s.doc().unwrap().path.as_deref(), Some(p.as_str()));
        let mut paths = 0;
        s.doc().unwrap().doc.walk(|n| paths += usize::from(matches!(n.kind, NodeKind::Path { .. })));
        assert!(paths >= 2, "{f}: both edited shapes are editable paths");
        if f == "eps" {
            assert_eq!(r["restored"], true);
            assert_eq!(s.doc().unwrap().doc.artboards.len(), 2, "the complete native document is carried");
        }
    }
}

#[test]
fn eps_save_preserves_effects_symbols_patterns_and_hidden_art() {
    let dir = vectorcraft_testkit::temp_dir("eps-save-editing");
    let mut s = super::tests_svgedit::rich();
    let hidden = s.doc().unwrap().doc.layers[0].id;
    s.execute("layer.setProps", &json!({"id": hidden.0, "visible": false})).unwrap();
    let p = path(&dir, "rich.eps");
    s.execute("file.saveAs", &json!({"path": p, "modified": null})).unwrap();
    let mut before = super::tests_svgedit::comparable(&s.doc().unwrap().doc);
    before.layers_open = s.doc().unwrap().layers_open.saved(&before);
    before.last_view = s.doc().unwrap().view.clone();
    s.execute("document.open", &json!({"path": p})).unwrap();
    assert_eq!(super::tests_svgedit::comparable(&s.doc().unwrap().doc), before);
}

#[test]
fn epsdxf_save_copy_and_write_failure_leave_the_document_unsaved() {
    for f in ["eps", "dxf"] {
        let mut s = art();
        let plan = save_plan(&s, SaveMode::Copy, &json!({"path": format!("copy.{f}")})).unwrap();
        save_with(&mut s, plan, |_, _| Ok(())).unwrap();
        assert!(s.doc().unwrap().path.is_none() && s.doc().unwrap().is_dirty());
        let plan = save_plan(&s, SaveMode::SaveAs, &json!({"path": format!("failed.{f}")})).unwrap();
        assert!(save_with(&mut s, plan, |_, _| Err(crate::EngineError::Other("disk full".into()))).is_err());
        assert!(s.doc().unwrap().path.is_none() && s.doc().unwrap().is_dirty());
        for partial in [json!({"useArtboards": true}), json!({"range": "1"}), json!({"selectedOnly": true})] {
            assert!(save_plan(&s, SaveMode::SaveAs, &json!({"format": f, "options": partial})).is_err());
        }
    }
}

#[test]
fn epsdxf_empty_drawings_can_be_saved_and_reopened() {
    for f in ["eps", "dxf"] {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 100, "height": 80})).unwrap();
        let r = s.execute("document.save", &json!({"format": f})).unwrap();
        let loaded = load(&format!("empty.{f}"), &vectorcraft_format::base64_decode(r["dataBase64"].as_str().unwrap()).unwrap()).unwrap();
        assert!(loaded.doc.artboards.len() == 1);
    }
}

#[test]
fn dxf_save_keeps_text_live_and_physical_dimensions_stable() {
    let dir = vectorcraft_testkit::temp_dir("dxf-save-units");
    let mut s = art();
    s.execute("text.create", &json!({"x": 40, "y": 150, "text": "editable"})).unwrap();
    let p = path(&dir, "units.dxf");
    s.execute("file.saveAs", &json!({"path": p, "options": {"unit": "in", "preserve": "editability"}})).unwrap();
    for _ in 0..3 {
        s.execute("document.open", &json!({"path": p})).unwrap();
        let mut text = false;
        let mut rect = false;
        s.doc().unwrap().doc.walk(|n| {
            text |= matches!(n.kind, NodeKind::Text(_));
            rect |= matches!(n.kind, NodeKind::Path { .. })
                && n.geometric_bounds().is_some_and(|b| (b.width() - 100.0).abs() < 0.01 && (b.height() - 50.0).abs() < 0.01);
        });
        assert!(text && rect, "text and rectangle dimensions survive repeated saves");
        s.execute("document.save", &json!({"acknowledgeLoss": true})).unwrap();
    }
}

#[test]
fn incomplete_eps_and_dxf_imports_require_acknowledgement_before_replacement() {
    let dir = vectorcraft_testkit::temp_dir("epsdxf-save-loss");
    let mut s = art();
    let eps = s.execute("document.exportEps", &json!({"previewFormat": "tiffColor"})).unwrap();
    let eps = vectorcraft_format::base64_decode(eps["dataBase64"].as_str().unwrap()).unwrap();
    let eps = eps
        .windows(9)
        .position(|w| w == b"native\n% ")
        .map(|at| {
            let mut broken = eps.clone();
            broken[at + 12] = b'z';
            broken
        })
        .unwrap();
    let dxf = b"0\nSECTION\n2\nENTITIES\n0\nLINE\n10\n0\n20\n0\n11\n10\n21\n20\n0\nBODY\n0\nENDSEC\n0\nEOF\n".to_vec();
    for (f, original) in [("eps", eps), ("dxf", dxf)] {
        let p = path(&dir, &format!("source.{f}"));
        std::fs::write(&p, &original).unwrap();
        s.execute("document.open", &json!({"path": p})).unwrap();
        s.execute("shape.rectangle", &json!({"x": 2, "y": 3, "width": 4, "height": 5})).unwrap();
        let e = s.execute("document.save", &json!({})).unwrap_err().to_string();
        assert!(e.contains("acknowledgeLoss"), "{e}");
        assert_eq!(std::fs::read(&p).unwrap(), original);
        let copy = path(&dir, &format!("safe-copy.{f}"));
        s.execute("file.saveCopy", &json!({"path": copy})).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), original);
        s.execute("document.save", &json!({"acknowledgeLoss": true})).unwrap();
        assert_ne!(std::fs::read(&p).unwrap(), original);
    }
}
