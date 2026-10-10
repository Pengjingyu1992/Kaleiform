use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use vectorcraft_doc::{Appearance, Document, Node};
use vectorcraft_engine::Session;
use vectorcraft_geom::{Rect, shapes};

use crate::{VectorcraftApp, menus};

#[test]
fn menu_state_for_a_large_path_selection() {
    let mut doc = Document::new(600.0, 400.0);
    let mut nodes = vec![];
    let mut ids = vec![];
    let count = std::env::var("VECTORCRAFT_MENU_TEST_OBJECTS").ok().and_then(|n| n.parse::<usize>().ok()).unwrap_or(2_000).min(50_000);
    for _ in 0..count {
        let id = doc.alloc_id();
        nodes.push(Arc::new(Node::path(id, shapes::rectangle(Rect::new(0.0, 0.0, 10.0, 10.0)), Appearance::default())));
        ids.push(id);
    }
    Arc::make_mut(doc.layers.first_mut().unwrap()).children_mut().unwrap().extend(nodes);
    let mut session = Session::new();
    session.add_document(doc, None);
    session.active_mut().unwrap().selection.objects = ids;
    let app = VectorcraftApp::new(session, Default::default());
    fn commands(items: &[menus::Item], out: &mut BTreeSet<&'static str>) {
        for item in items {
            match item {
                menus::Item::Cmd(_, id, _) => {
                    out.insert(id);
                }
                menus::Item::Sub(_, children) => commands(children, out),
                _ => {}
            }
        }
    }
    let mut ids = BTreeSet::new();
    for (_, items) in menus::menu_tree() {
        commands(&items, &mut ids);
    }
    let start = Instant::now();
    let mut disabled = BTreeSet::new();
    let mut timings = vec![];
    for id in ids {
        let t = Instant::now();
        if !menus::enabled(&app, id) {
            disabled.insert(id);
        }
        timings.push((t.elapsed(), id));
    }
    // Native menus keep the toggle kind stable even when no text is selected.
    assert_eq!(menus::checked(&app, "type.orientation.vertical", &serde_json::Value::Null), Some(false));
    assert_eq!(menus::checked(&app, "type.pathOptions", &serde_json::json!({"effect": "rainbow"})), Some(false));
    eprintln!("large selection ({count} paths) menu sweep: {:?}", start.elapsed());
    timings.sort_by_key(|(time, _)| std::cmp::Reverse(*time));
    eprintln!("slowest predicates: {:?}", &timings[..10]);
    for id in [
        "object.blend.release",
        "object.envelope.release",
        "object.repeat.release",
        "effect.expandAppearance",
        "ui.cropImage",
        "object.createObjectMosaic",
        "object.slice.options",
        "links.editOriginal",
    ] {
        assert!(disabled.contains(id), "plain paths must not enable {id}");
    }
    assert!(menus::enabled(&app, "edit.copy"));
}

#[test]
fn dense_ungroup_can_draw_the_next_complete_frame() {
    let mut doc = Document::new(600.0, 400.0);
    let count = std::env::var("VECTORCRAFT_MENU_TEST_OBJECTS").ok().and_then(|n| n.parse::<usize>().ok()).unwrap_or(50_000).min(50_000);
    let mut children = Vec::with_capacity(count);
    for _ in 0..count {
        let id = doc.alloc_id();
        children.push(Arc::new(Node::path(id, shapes::rectangle(Rect::new(10.0, 10.0, 30.0, 30.0)), Appearance::default_art())));
    }
    let group = doc.alloc_id();
    doc.insert(doc.default_layer(), usize::MAX, Node::group(group, children)).unwrap();
    let mut session = Session::new();
    session.add_document(doc, None);
    session.active_mut().unwrap().selection.set([group]);
    let mut app = VectorcraftApp::new(session, Default::default());
    let started = Instant::now();
    app.run("object.ungroup", serde_json::json!({})).unwrap();
    eprintln!("UI ungroup {count} paths: {:?}", started.elapsed());
    let selected = app.session.active().unwrap().selection.objects.clone();
    let before = app.session.active().unwrap().doc.clone();
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    for frame in 0..3 {
        let started = Instant::now();
        let mut out = ctx.run_ui(
            egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))), ..Default::default() },
            |ui| {
                app.logic(ui.ctx());
                app.ui(ui);
            },
        );
        out.textures_delta.clear();
        eprintln!("dense selection complete frame {frame}: {:?}", started.elapsed());
        assert!(started.elapsed().as_secs_f64() < 10.0, "drawing a dense selection must not stall the UI for ten seconds");
        assert!(!app.ui.status.starts_with("Internal error:"), "frame failed: {}", app.ui.status);
        if frame > 0 {
            assert!(out.shapes.len() > 50 && app.canvas_rect.is_some(), "the full window and canvas must be drawn");
        }
        if count > 25_000 {
            assert!(out.shapes.len() < 4000, "selection highlights must stay bounded: {} shapes", out.shapes.len());
        }
        assert_eq!(app.session.active().unwrap().selection.objects, selected);
        assert!(Arc::ptr_eq(&before, &app.session.active().unwrap().doc), "drawing must not modify the art");
    }
}
