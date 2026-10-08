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
    assert_eq!(menus::checked(&app, "type.orientation.vertical", &serde_json::Value::Null), None);
    assert_eq!(menus::checked(&app, "type.pathOptions", &serde_json::json!({"effect": "rainbow"})), None);
    eprintln!("large selection ({count} paths) menu sweep: {:?}", start.elapsed());
    timings.sort_by_key(|(time, _)| std::cmp::Reverse(*time));
    eprintln!("slowest predicates: {:?}", &timings[..10]);
    for id in [
        "object.blend.release",
        "object.envelope.release",
        "object.repeat.release",
        "effect.expandAppearance",
        "object.cropImage",
        "object.createObjectMosaic",
        "object.slice.options",
        "links.editOriginal",
    ] {
        assert!(disabled.contains(id), "plain paths must not enable {id}");
    }
    assert!(menus::enabled(&app, "edit.copy"));
}
