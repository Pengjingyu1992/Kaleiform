//! Headless frames of the Stroke panel's reach: Units > Stroke in the panel, the Control bar and
//! the Properties panel, mixed values shown blank, Align Stroke for open paths and type, and the
//! weight presets.

use serde_json::json;

use super::tests_appearance::{app_with_rect, click, frame_events, run, text_rect};
use super::*;

type Texts = Vec<(String, egui::Rect)>;

fn texts(ctx: &egui::Context, app: &mut VectorcraftApp, f: impl FnMut(&mut VectorcraftApp, &mut Ui)) -> Texts {
    frame_events(ctx, app, vec![], f)
}

fn shows(t: &Texts, s: &str) -> bool {
    t.iter().any(|(x, _)| x == s)
}

/// The centre of the `n`th (0-based) 24 px button right of a Stroke panel row label.
fn row_button(ctx: &egui::Context, t: &Texts, label: &str, n: usize) -> egui::Pos2 {
    let r = text_rect(t, label);
    let sp = ctx.global_style().spacing.item_spacing.x;
    egui::pos2(r.right() + sp + n as f32 * (24.0 + sp) + 12.0, r.center().y)
}

#[test]
fn weights_and_dashes_show_in_the_stroke_unit_everywhere() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut app = app_with_rect();
    app.session.prefs.units_stroke = "millimeters".into();
    run(&mut app, "stroke.set", json!({"dash": [12, 6]}));
    let mm = vectorcraft_doc::Unit::Millimeters.format(1.0);
    assert_eq!(mm, "0.353 mm");
    let t = texts(&ctx, &mut app, stroke::show);
    assert!(shows(&t, &mm), "Stroke panel: {t:?}");
    assert!(shows(&t, "4.233") && shows(&t, "2.117"), "dash and gap in mm: {t:?}");
    assert!(shows(&texts(&ctx, &mut app, properties::show), &mm), "Properties panel");
    assert!(shows(&texts(&ctx, &mut app, crate::chrome::control_bar), &mm), "Control bar");
}

#[test]
fn mixed_weights_show_blank() {
    let ctx = egui::Context::default();
    let mut app = app_with_rect();
    let a = app.session.active().unwrap().selection.objects[0];
    let b = run(&mut app, "shape.rectangle", json!({"x": 120, "y": 10, "width": 50, "height": 50}))["id"].as_u64().unwrap();
    run(&mut app, "stroke.set", json!({"ids": [b], "weight": 3}));
    run(&mut app, "select.set", json!({ "ids": [a.0, b] }));
    let t = texts(&ctx, &mut app, stroke::show);
    assert!(!shows(&t, "1 pt") && !shows(&t, "3 pt"), "{t:?}");
    run(&mut app, "select.set", json!({ "ids": [b] }));
    assert!(shows(&texts(&ctx, &mut app, stroke::show), "3 pt"), "the cache follows the selection");
}

#[test]
fn inside_and_outside_are_off_for_open_paths_and_type() {
    let ctx = egui::Context::default();
    let mut app = app_with_rect();
    let align = |app: &VectorcraftApp| current_stroke(app).unwrap().align;
    let at = row_button(&ctx, &texts(&ctx, &mut app, stroke::show), "Align Stroke:", 1);
    click(&ctx, &mut app, at, stroke::show);
    assert_eq!(align(&app), vectorcraft_doc::StrokeAlign::Inside, "a closed path takes an inside stroke");
    run(&mut app, "shape.line", json!({"x1": 10, "y1": 150, "x2": 150, "y2": 150}));
    click(&ctx, &mut app, at, stroke::show);
    assert_eq!(align(&app), vectorcraft_doc::StrokeAlign::Center, "an open path doesn't");
    let id = run(&mut app, "text.create", json!({"x": 10, "y": 190, "text": "Hi"}))["id"].as_u64().unwrap();
    run(&mut app, "select.set", json!({ "ids": [id] }));
    run(&mut app, "paint.setStroke", json!({"color": "#000000"}));
    let undo = app.session.active().unwrap().history.undo.len();
    click(&ctx, &mut app, at, stroke::show);
    assert_eq!(app.session.active().unwrap().history.undo.len(), undo, "type doesn't");
}

#[test]
fn the_weight_presets_run_from_a_quarter_point_to_a_hundred() {
    assert!(stroke::WEIGHT_PRESETS.contains(&0.25) && stroke::WEIGHT_PRESETS.contains(&100.0));
    let ctx = egui::Context::default();
    let mut app = app_with_rect();
    // The chevron at the right end of the 120 px weight spinner opens them.
    let t = texts(&ctx, &mut app, stroke::show);
    let r = text_rect(&t, "Weight:");
    let sp = ctx.global_style().spacing.item_spacing.x;
    click(&ctx, &mut app, egui::pos2(r.right() + sp + 110.0, r.center().y), stroke::show);
    let t = texts(&ctx, &mut app, stroke::show);
    assert!(shows(&t, "0.25 pt") && shows(&t, "100 pt"), "{t:?}");
}
