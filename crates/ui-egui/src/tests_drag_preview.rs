use std::{sync::Arc, time::Instant};

use serde_json::json;
use vectorcraft_doc::{Appearance, Document, Node, NodeId, NodeKind, OpacityMask};
use vectorcraft_engine::Session;
use vectorcraft_geom::{Point, Rect, shapes};
use vectorcraft_tools::{PointerEvent, PointerKind};

use crate::{VectorcraftApp, canvas};

fn dense(count: usize) -> (VectorcraftApp, NodeId) {
    let mut doc = Document::new(1600.0, 1200.0);
    let mut paths = Vec::new();
    for i in 0..count {
        let (x, y) = ((i % 250) as f64 * 6.0 + 50.0, (i / 250) as f64 * 6.0 + 50.0);
        paths.push(Arc::new(Node::path(doc.alloc_id(), shapes::rectangle(Rect::new(x, y, x + 4.0, y + 4.0)), Appearance::default_art())));
    }
    let id = doc.alloc_id();
    let group = Node::group(id, paths);
    Arc::make_mut(doc.layers.first_mut().unwrap()).children_mut().unwrap().push(Arc::new(group));
    let mut session = Session::new();
    session.add_document(doc, None);
    let mut app = VectorcraftApp::new(session, Default::default());
    app.run("select.set", json!({"ids": [id.0]})).unwrap();
    (app, id)
}

fn frame(app: &mut VectorcraftApp, ctx: &egui::Context) -> egui::FullOutput {
    ctx.run_ui(
        egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0))), ..Default::default() },
        |ui| canvas::show(app, ui),
    )
}

fn pointer(app: &mut VectorcraftApp, kind: PointerKind, x: f64, y: f64) {
    let view = app.view_info();
    canvas::dispatch(app, &PointerEvent { kind, pos: Point::new(x, y), mods: Default::default(), pressure: 1.0 }, view);
}

fn settle(app: &mut VectorcraftApp, ctx: &egui::Context) {
    let start = Instant::now();
    loop {
        frame(app, ctx).textures_delta.clear();
        if app.canvas.key.is_some() && app.canvas.worker.as_ref().is_some_and(|w| w.pending.is_none()) {
            return;
        }
        assert!(start.elapsed().as_secs() < 10, "canvas did not settle");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn dense_move_reuses_snapshot_and_textures_then_cancels_without_artifacts() {
    let (mut app, _) = dense(2500);
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    settle(&mut app, &ctx);
    let original = app.session.doc().unwrap().doc.clone();
    let base_box = app.selection_box().unwrap();
    pointer(&mut app, PointerKind::Down, 802.0, 82.0);
    pointer(&mut app, PointerKind::Drag, 822.0, 102.0);
    assert!(app.session.deferred_move().is_some());
    settle(&mut app, &ctx);
    assert_eq!(crate::move_preview::inspect(&app)["ready"], true);
    for step in 1..=10 {
        pointer(&mut app, PointerKind::Drag, 822.0 + step as f64, 102.0);
        let mut out = frame(&mut app, &ctx);
        let uploads: Vec<_> = out.textures_delta.set.iter().filter(|(id, _)| **id != egui::TextureId::Managed(0)).map(|(id, _)| *id).collect();
        out.textures_delta.clear();
        assert!(uploads.is_empty(), "moving a cached preview must not upload artwork textures: {uploads:?}");
        assert!(Arc::ptr_eq(&app.session.doc().unwrap().doc, &original));
        assert!(app.canvas.worker.as_ref().unwrap().pending.is_none());
    }
    let moved = app.selection_box().unwrap().center() - base_box.center();
    assert_eq!(moved, vectorcraft_geom::Vec2::new(30.0, 20.0));
    app.session.cancel_interaction().unwrap();
    frame(&mut app, &ctx).textures_delta.clear();
    assert!(app.canvas.move_preview.is_none());
    assert!(Arc::ptr_eq(&app.session.doc().unwrap().doc, &original));
}

#[test]
fn committed_preview_stays_until_exact_raster_arrives_and_closing_releases_it() {
    let (mut app, _) = dense(2500);
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    settle(&mut app, &ctx);
    pointer(&mut app, PointerKind::Down, 802.0, 82.0);
    pointer(&mut app, PointerKind::Drag, 826.0, 106.0);
    settle(&mut app, &ctx);
    pointer(&mut app, PointerKind::Up, 826.0, 106.0);
    assert_eq!(crate::move_preview::inspect(&app)["finishing"], true);
    settle(&mut app, &ctx);
    assert!(app.canvas.move_preview.is_none());
    app.session.close_document(0);
    frame(&mut app, &ctx).textures_delta.clear();
    assert!(app.canvas.move_preview.is_none());
    assert!(app.canvas.raster_stamp.is_none());
}

fn compose(
    parts: &[crate::move_preview::PartImage],
    view: vectorcraft_geom::Affine,
    delta: vectorcraft_geom::Vec2,
    copy: bool,
    w: usize,
    h: usize,
) -> Vec<u8> {
    let mut pixels = vec![0_u8; w * h * 4];
    for part in parts {
        let offsets = if part.moving && copy {
            vec![vectorcraft_geom::Vec2::ZERO, delta]
        } else {
            vec![if part.moving { delta } else { vectorcraft_geom::Vec2::ZERO }]
        };
        for offset in offsets {
            let origin = view * (part.rect.origin() + offset);
            for y in 0..part.image.height as usize {
                for x in 0..part.image.width as usize {
                    let (dx, dy) = (origin.x.round() as i64 + x as i64, origin.y.round() as i64 + y as i64);
                    if dx < 0 || dy < 0 || dx >= w as i64 || dy >= h as i64 {
                        continue;
                    }
                    let from = (y * part.image.width as usize + x) * 4;
                    let to = (dy as usize * w + dx as usize) * 4;
                    let alpha = u32::from(part.image.pixels[from + 3]);
                    for channel in 0..4 {
                        pixels[to + channel] = (u32::from(part.image.pixels[from + channel])
                            + (u32::from(pixels[to + channel]) * (255 - alpha) + 127) / 255)
                            .min(255) as u8;
                    }
                }
            }
        }
    }
    pixels
}

#[test]
fn scene_keeps_transparency_clipping_and_foreground_order_at_final_position() {
    use vectorcraft_color::{Color, Paint};
    use vectorcraft_geom::{Affine, Vec2};
    use vectorcraft_render::{RenderOptions, Renderer};
    for clip in [false, true] {
        let (app, id) = dense(2500);
        let mut doc = app.session.doc().unwrap().doc.as_ref().clone();
        let mut selection = app.session.doc().unwrap().selection.clone();
        let make_rect = |id, r, color| Arc::new(Node::path(id, shapes::rectangle(r), Appearance::basic(Paint::solid(color), Paint::None, 0.0)));
        let back_id = doc.alloc_id();
        let front_id = doc.alloc_id();
        let clip_id = doc.alloc_id();
        let moving = doc.node_mut(id).unwrap();
        moving.opacity = 0.6;
        if clip {
            moving.children_mut().unwrap().insert(0, make_rect(clip_id, Rect::new(500.0, 40.0, 1000.0, 150.0), Color::WHITE));
            moving.set_clips(true);
        }
        let children = Arc::make_mut(doc.layers.first_mut().unwrap()).children_mut().unwrap();
        children.insert(0, make_rect(back_id, Rect::new(0.0, 0.0, 1600.0, 1200.0), Color::from_hex("#13ad86").unwrap()));
        children.push(make_rect(front_id, Rect::new(900.0, 50.0, 1100.0, 400.0), Color::from_hex("#db3159").unwrap()));
        let scene = crate::move_preview::Scene::new(Arc::new(doc.clone()), &selection).unwrap();
        let view = Affine::scale(0.25);
        let opts = RenderOptions { artboards: false, ..Default::default() };
        let mut renderer = Renderer::new();
        let parts = scene.render(&mut renderer, 400, 300, view, &opts).unwrap();
        assert_eq!(parts.len(), 3);
        for copy in [false, true] {
            let mut moved = doc.clone();
            let delta = Vec2::new(24.0, 16.0);
            let target = if copy {
                let clone = moved.reid(doc.node(id).unwrap());
                let new_id = clone.id;
                Arc::make_mut(moved.layers.first_mut().unwrap()).children_mut().unwrap().insert(2, Arc::new(clone));
                new_id
            } else {
                id
            };
            moved.node_mut(target).unwrap().transform(Affine::translate(delta), false);
            let expected = renderer.render(&moved, 400, 300, view, &opts).pixels;
            let actual = compose(&parts, view, delta, copy, 400, 300);
            let max = expected.iter().zip(&actual).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            assert!(max <= 2, "clip={clip} copy={copy} maximum pixel difference {max}");
        }
        selection.clear();
        assert!(crate::move_preview::Scene::new(Arc::new(doc), &selection).is_none());
    }
}

#[test]
fn scene_rejects_backdrop_blends_unlinked_masks_and_excessive_display_memory() {
    let (app, id) = dense(2500);
    let original = app.session.doc().unwrap();
    let mut doc = original.doc.as_ref().clone();
    doc.node_mut(id).unwrap().blend = vectorcraft_color::BlendMode::Multiply;
    assert!(crate::move_preview::Scene::new(Arc::new(doc), &original.selection).is_none());
    let mut doc = original.doc.as_ref().clone();
    let mask_id = doc.alloc_id();
    let mask = Node::path(mask_id, shapes::rectangle(Rect::new(0.0, 0.0, 800.0, 400.0)), Appearance::default_art());
    let mut mask = OpacityMask::new(mask, true);
    mask.linked = false;
    doc.node_mut(id).unwrap().mask = Some(Box::new(mask));
    assert!(crate::move_preview::Scene::new(Arc::new(doc), &original.selection).is_none());
    let scene = crate::move_preview::Scene::new(original.doc.clone(), &original.selection).unwrap();
    assert!(scene.render(&mut vectorcraft_render::Renderer::new(), 400, 300, vectorcraft_geom::Affine::scale(100.0), &Default::default()).is_err());
    assert!(matches!(original.doc.node(id).unwrap().kind, NodeKind::Group { .. }));
}

#[test]
fn saving_during_cached_move_keeps_every_editable_anchor_and_exact_svg_geometry() {
    use std::sync::Mutex;
    let (mut app, id) = dense(2500);
    let written = Arc::new(Mutex::new(Vec::new()));
    let sink = written.clone();
    app.services.write = Some(Box::new(move |_, bytes| {
        *sink.lock().map_err(|e| e.to_string())? = bytes.to_vec();
        Ok(())
    }));
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    settle(&mut app, &ctx);
    let original = app.session.doc().unwrap().doc.clone();
    pointer(&mut app, PointerKind::Down, 802.0, 82.0);
    pointer(&mut app, PointerKind::Drag, 826.0, 106.0);
    assert!(app.session.deferred_move().is_some());
    app.run("file.saveAs", json!({"path": "/virtual/dense.vectorcraft", "format": "vectorcraft"})).unwrap();
    pointer(&mut app, PointerKind::Up, 826.0, 106.0);
    let reopened = vectorcraft_format::load(&written.lock().unwrap()).unwrap();
    assert_eq!(reopened.node_count(), original.node_count());
    let mut expected = original.as_ref().clone();
    expected.node_mut(id).unwrap().transform(vectorcraft_geom::Affine::translate((24.0, 24.0)), false);
    assert_eq!(serde_json::to_value(reopened.node(id).unwrap()).unwrap(), serde_json::to_value(expected.node(id).unwrap()).unwrap());
    let options = vectorcraft_svg::ExportOptions::default();
    assert_eq!(vectorcraft_svg::export(&reopened, &options), vectorcraft_svg::export(&expected, &options));
    assert_eq!(app.session.doc().unwrap().history.undo.len(), 1);
}

#[test]
fn cached_move_can_reveal_art_that_started_outside_the_viewport() {
    use vectorcraft_geom::{Affine, Vec2};
    let (app, id) = dense(2500);
    let st = app.session.doc().unwrap();
    let scene = crate::move_preview::Scene::new(st.doc.clone(), &st.selection).unwrap();
    let view = Affine::translate((-300.0, 0.0)) * Affine::scale(0.25);
    let mut renderer = vectorcraft_render::Renderer::new();
    let options = vectorcraft_render::RenderOptions { artboards: false, ..Default::default() };
    let parts = scene.render(&mut renderer, 400, 300, view, &options).unwrap();
    let delta = Vec2::new(1200.0, 0.0);
    let mut moved = st.doc.as_ref().clone();
    moved.node_mut(id).unwrap().transform(Affine::translate(delta), false);
    assert_eq!(compose(&parts, view, delta, false, 400, 300), renderer.render(&moved, 400, 300, view, &options).pixels);
}

#[test]
fn document_switch_discards_the_previous_drag_surfaces() {
    let (mut app, _) = dense(2500);
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    settle(&mut app, &ctx);
    pointer(&mut app, PointerKind::Down, 802.0, 82.0);
    pointer(&mut app, PointerKind::Drag, 826.0, 106.0);
    frame(&mut app, &ctx).textures_delta.clear();
    assert!(app.canvas.move_preview.is_some());
    app.run("file.new", json!({"width": 100, "height": 100})).unwrap();
    frame(&mut app, &ctx).textures_delta.clear();
    assert!(app.canvas.move_preview.is_none());
    assert!(app.session.deferred_move().is_none());
    settle(&mut app, &ctx);
    assert_eq!(app.canvas.key.as_ref().unwrap().doc, app.session.doc().unwrap().uid as usize);
}

#[test]
fn ungrouped_dense_selection_reuses_its_ids_and_moves_with_a_single_scene() {
    let (mut app, _) = dense(2500);
    app.run("object.ungroup", json!({})).unwrap();
    assert_eq!(app.session.doc().unwrap().selection.objects.len(), 2500);
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    settle(&mut app, &ctx);
    let original = app.session.doc().unwrap().doc.clone();
    pointer(&mut app, PointerKind::Down, 802.0, 82.0);
    pointer(&mut app, PointerKind::Drag, 826.0, 106.0);
    assert!(app.session.deferred_move().is_some());
    let ids = app.session.doc().unwrap().selection.objects.as_ptr();
    settle(&mut app, &ctx);
    for dx in 25..40 {
        pointer(&mut app, PointerKind::Drag, 802.0 + dx as f64, 106.0);
        frame(&mut app, &ctx).textures_delta.clear();
        assert!(Arc::ptr_eq(&app.session.doc().unwrap().doc, &original));
        assert_eq!(app.session.doc().unwrap().selection.objects.as_ptr(), ids);
        assert!(app.canvas.worker.as_ref().unwrap().pending.is_none());
    }
    pointer(&mut app, PointerKind::Up, 841.0, 106.0);
    assert_eq!(app.session.doc().unwrap().history.undo.len(), 2);
    app.run("edit.undo", json!({})).unwrap();
    assert!(Arc::ptr_eq(&app.session.doc().unwrap().doc, &original));
}

#[test]
#[ignore = "manual dense-move timing comparison"]
fn dense_trace_drag_benchmark() {
    for (count, optimized) in [(10_000, false), (10_000, true), (50_000, false), (50_000, true)] {
        let (mut app, _) = dense(count);
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        for _ in 0..3 {
            frame(&mut app, &ctx).textures_delta.clear();
        }
        let until = Instant::now();
        while app.canvas.key.is_none() {
            assert!(until.elapsed().as_secs() < 10);
            std::thread::sleep(std::time::Duration::from_millis(1));
            frame(&mut app, &ctx).textures_delta.clear();
        }
        pointer(&mut app, PointerKind::Down, 802.0, 172.0);
        if !optimized {
            app.session.clear_move_preview();
            app.canvas.move_preview = None;
        }
        pointer(&mut app, PointerKind::Drag, 812.0, 182.0);
        assert_eq!(app.session.deferred_move().is_some(), optimized);
        let until = Instant::now();
        loop {
            frame(&mut app, &ctx).textures_delta.clear();
            if app.canvas.worker.as_ref().unwrap().pending.is_none() {
                break;
            }
            assert!(until.elapsed().as_secs() < 10);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let mut samples = Vec::new();
        for step in 1..=12 {
            let start = Instant::now();
            pointer(&mut app, PointerKind::Drag, 802.0 + step as f64 * 3.0, 172.0 + step as f64 * 2.0);
            frame(&mut app, &ctx).textures_delta.clear();
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        pointer(&mut app, PointerKind::Up, 838.0, 196.0);
        samples.sort_by(f64::total_cmp);
        eprintln!("dense move: paths={count} optimized={optimized} median_ms={:.2} max_ms={:.2}", samples[6], samples[11]);
    }
}
