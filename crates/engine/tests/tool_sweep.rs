//! Catalogue-wide safety checks complement the tools' feature-specific tests. A completed or
//! cancelled gesture must leave a valid, serializable document and undo back to its starting art.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};
use vectorcraft_engine::ViewInfo;
use vectorcraft_engine::tools::{Mods, PointerEvent, PointerKind, ToolKey};
use vectorcraft_testkit::fixtures::Fixture;
use vectorcraft_testkit::invariants::{check_all, doc_json, first_diff};

#[test]
fn every_tool_gesture_can_be_finished_or_cancelled_and_undone() {
    let mut failures = vec![];
    for tool in vectorcraft_tools::catalog::all_tools() {
        for fixture in Fixture::ALL {
            for mods in [Mods::default(), Mods { shift: true, ..Default::default() }, Mods { alt: true, ..Default::default() }] {
                for end in ["release", "escape", "switch"] {
                    let mut session = fixture.session();
                    let view = ViewInfo::default();
                    session.select_tool(tool.id, view).unwrap();
                    let before = doc_json(&session.doc().unwrap().doc);
                    let undo = session.doc().unwrap().history.undo.len();
                    let run = vectorcraft_testkit::catch_quiet(|| {
                        for (kind, x, y) in [(PointerKind::Down, 80.0, 80.0), (PointerKind::Drag, 100.0, 90.0), (PointerKind::Drag, 140.0, 120.0)] {
                            // A tool may reject the fixture (for example a missing symbol or
                            // incompatible path); safety and rollback still apply after rejection.
                            let _ = session.pointer(&PointerEvent::new(kind, x, y).with_mods(mods), view);
                        }
                        if end == "release" {
                            let _ = session.pointer(&PointerEvent::new(PointerKind::Up, 160.0, 130.0).with_mods(mods), view);
                        }
                        if end != "switch" {
                            let _ = session.tool_key(ToolKey::Escape, Mods::default(), view);
                        }
                        session.select_tool("zoom", view).unwrap();
                        assert!(!session.in_interaction(), "gesture left an interaction open");
                        check_all(&mut session).unwrap();
                        while session.doc().unwrap().history.undo.len() > undo {
                            session.execute("edit.undo", &json!({})).unwrap();
                        }
                        let after = doc_json(&session.doc().unwrap().doc);
                        assert_eq!(after, before, "{}", first_diff(&before, &after, "$"));
                    });
                    if let Err(error) = run {
                        failures.push(format!("{} [{fixture:?}, {mods:?}, end={end}]: {error}", tool.id));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn every_persistent_tool_option_survives_tool_switches() {
    let mut session = Fixture::Single.session();
    let view = ViewInfo::default();
    for tool in vectorcraft_tools::catalog::all_tools() {
        session.select_tool(tool.id, view).unwrap();
        for (_, keys) in vectorcraft_tools::settings::stores(tool.id) {
            for key in keys {
                let original = session.tool_options()[key].clone();
                let changed = match &original {
                    Value::Bool(value) => json!(!value),
                    Value::Number(value) => json!(value.as_f64().unwrap() + 1.0),
                    _ => original.clone(),
                };
                session.set_tool_option(key, &changed);
                let accepted = session.tool_options()[key].clone();
                session.select_tool("selection", view).unwrap();
                session.select_tool(tool.id, view).unwrap();
                assert_eq!(session.tool_options()[key], accepted, "{}.{key} reset during a tool switch", tool.id);
                session.set_tool_option(key, &original);
            }
        }
    }
}

#[test]
fn escape_rolls_back_selection_and_pen_drags_without_undo() {
    for tool in ["selection", "directSelection", "groupSelection", "pen"] {
        let mut session = Fixture::Single.session();
        let view = ViewInfo::default();
        session.select_tool(tool, view).unwrap();
        let before = doc_json(&session.doc().unwrap().doc);
        let undo = session.doc().unwrap().history.undo.len();
        for (kind, x, y) in [(PointerKind::Down, 80.0, 80.0), (PointerKind::Drag, 140.0, 120.0)] {
            session.pointer(&PointerEvent::new(kind, x, y), view).unwrap();
        }
        session.tool_key(ToolKey::Escape, Mods::default(), view).unwrap();
        assert!(!session.in_interaction(), "{tool}");
        assert_eq!(session.doc().unwrap().history.undo.len(), undo, "{tool}");
        assert_eq!(doc_json(&session.doc().unwrap().doc), before, "{tool}");
        session.pointer(&PointerEvent::new(PointerKind::Up, 140.0, 120.0), view).unwrap();
        assert!(!session.in_interaction(), "late release reopened {tool}");
        assert_eq!(doc_json(&session.doc().unwrap().doc), before, "late release edited {tool}");
    }
}
