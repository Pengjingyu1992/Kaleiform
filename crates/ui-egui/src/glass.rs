//! Window materials and bounded, cached canvas backdrops for floating panels.

use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};

use egui::{Color32, CornerRadius, Id, Painter, Rect, Shape, Stroke, StrokeKind, TextureHandle, Ui, pos2};
use image::{Rgba, RgbaImage};

use crate::canvas::Xf;
use crate::state::View;
use crate::theme::Tokens;
use crate::{CacheKey, VectorcraftApp};

const MAX_EDGE: u32 = 384;
const BLUR_POINTS: f32 = 14.0;

pub fn native_material(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(Id::new("glass-native"))).unwrap_or(false)
}

pub fn set_native_material(ctx: &egui::Context, available: bool) {
    ctx.data_mut(|d| d.insert_temp(Id::new("glass-native"), available));
}

#[derive(Clone)]
struct Scene {
    texture: TextureHandle,
    rect: Rect,
}

pub fn begin_frame(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<Scene>(Id::new("glass-scene")));
}

#[derive(Clone)]
struct Snapshot {
    serial: u64,
    key: CacheKey,
    rect: Rect,
    image: RgbaImage,
    sigma: f32,
    blur_ms: f64,
}

struct Worker {
    tx: SyncSender<Snapshot>,
    done: Arc<Mutex<Option<Snapshot>>>,
}

impl Worker {
    fn spawn(ctx: egui::Context) -> Option<Self> {
        if cfg!(target_arch = "wasm32") {
            return None;
        }
        let (tx, jobs) = sync_channel::<Snapshot>(1);
        let done = Arc::new(Mutex::new(None));
        let result = done.clone();
        std::thread::Builder::new()
            .name("kaleiform-glass".into())
            .spawn(move || {
                while let Ok(mut source) = jobs.recv() {
                    while let Ok(newer) = jobs.try_recv() {
                        source = newer;
                    }
                    let started = crate::now_ms();
                    source.image = image::imageops::fast_blur(&source.image, source.sigma);
                    source.blur_ms = crate::now_ms() - started;
                    // Only the newest result survives while the window is minimized.
                    *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(source);
                    ctx.request_repaint();
                }
            })
            .ok()?;
        Some(Self { tx, done })
    }
}

#[derive(Default)]
pub struct BackdropCache {
    serial: u64,
    source: Option<Snapshot>,
    pending: Option<u64>,
    worker: Option<Worker>,
    ready: Option<(Snapshot, TextureHandle)>,
}

impl BackdropCache {
    pub fn inspect(&self) -> serde_json::Value {
        serde_json::json!({
            "sourceSize": self.source.as_ref().map(|s| [s.image.width(), s.image.height()]),
            "ready": self.ready.is_some(),
            "blurMs": self.ready.as_ref().map(|(s, _)| s.blur_ms),
        })
    }

    fn accept(&mut self, ctx: &egui::Context, done: Snapshot, doc: usize) {
        if done.key.doc != doc {
            return;
        }
        let image = egui::ColorImage::from_rgba_premultiplied([done.image.width() as usize, done.image.height() as usize], done.image.as_raw());
        let tex = match self.ready.take() {
            Some((_, mut tex)) => {
                tex.set(image, egui::TextureOptions::LINEAR);
                tex
            }
            None => ctx.load_texture("glass-backdrop", image, egui::TextureOptions::LINEAR),
        };
        self.ready = Some((done, tex));
    }

    pub fn publish(&mut self, ctx: &egui::Context, xf: &Xf, doc: usize) {
        if self.worker.is_none() {
            self.worker = Worker::spawn(ctx.clone());
        }
        let done = self.worker.as_ref().and_then(|w| w.done.lock().unwrap_or_else(|e| e.into_inner()).take());
        if let Some(done) = done {
            self.accept(ctx, done, doc);
        }
        if let Some(source) = &self.source
            && source.key.doc == doc
            && self.pending != Some(source.serial)
        {
            if let Some(worker) = &self.worker {
                if worker.tx.try_send(source.clone()).is_ok() {
                    self.pending = Some(source.serial);
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            } else {
                // Browsers have no worker thread here. The same small, bounded image is cached.
                let mut done = source.clone();
                let started = crate::now_ms();
                done.image = image::imageops::fast_blur(&done.image, done.sigma);
                done.blur_ms = crate::now_ms() - started;
                self.pending = Some(done.serial);
                self.accept(ctx, done, doc);
            }
        }
        if let Some((source, texture)) = &self.ready
            && source.key.doc == doc
            && (source.key.rot.to_radians() - xf.rot).abs() < 1e-9
        {
            let old = source_xf(&source.key, source.rect);
            let rect = Rect::from_min_max(xf.to_screen(old.to_doc(source.rect.min)), xf.to_screen(old.to_doc(source.rect.max)));
            ctx.data_mut(|d| d.insert_temp(Id::new("glass-scene"), Scene { texture: texture.clone(), rect }));
        }
    }
}

fn source_xf(key: &CacheKey, rect: Rect) -> Xf {
    Xf::new(rect, &View { zoom: key.zoom, center: vectorcraft_geom::Point::new(key.cx, key.cy), rotation: key.rot, ..Default::default() })
}

/// Capture only a small sample of the completed art raster. It never reads back the GPU or
/// includes other panels, so their text and previous glass frames cannot feed back into the blur.
pub fn capture(app: &mut VectorcraftApp, ctx: &egui::Context, img: &vectorcraft_render::Rendered, key: &CacheKey) {
    let t = Tokens::get(ctx);
    if !t.glass {
        return;
    }
    let Some(rect) = app.canvas_rect.filter(|r| r.is_positive() && r.is_finite()) else { return };
    let Some(st) = app.session.active() else { return };
    let scale = (rect.width().max(rect.height()) / MAX_EDGE as f32).max(3.0);
    let w = (rect.width() / scale).ceil().clamp(1.0, MAX_EDGE as f32) as u32;
    let h = (rect.height() / scale).ceil().clamp(1.0, MAX_EDGE as f32) as u32;
    let xf = source_xf(key, rect);
    let mut sample = RgbaImage::from_pixel(w, h, Rgba(t.pasteboard.to_array()));
    let screen =
        |x: u32, y: u32| pos2(rect.left() + (x as f32 + 0.5) * rect.width() / w as f32, rect.top() + (y as f32 + 0.5) * rect.height() / h as f32);
    if app.ui.view.artboards || key.outline {
        let setup = &st.doc.setup;
        let paper = if setup.simulate_paper && !key.outline { crate::panels::c32(&setup.paper()) } else { Color32::WHITE };
        let checker = st.transparency_grid && setup.background == vectorcraft_doc::Background::Transparent && !key.outline;
        let cell = setup.grid_size.cell().max(1.0) as f64 / key.zoom;
        let mut remaining = MAX_EDGE as usize * MAX_EDGE as usize * 4;
        for ab in &st.doc.artboards {
            let quad = xf.quad(ab.rect);
            let bounds = Rect::from_points(&quad).intersect(rect);
            if !bounds.is_positive() {
                continue;
            }
            let x0 = ((bounds.left() - rect.left()) / rect.width() * w as f32).floor().max(0.0) as u32;
            let x1 = ((bounds.right() - rect.left()) / rect.width() * w as f32).ceil().min(w as f32) as u32;
            let y0 = ((bounds.top() - rect.top()) / rect.height() * h as f32).floor().max(0.0) as u32;
            let y1 = ((bounds.bottom() - rect.top()) / rect.height() * h as f32).ceil().min(h as f32) as u32;
            let work = (x1 - x0) as usize * (y1 - y0) as usize;
            if work > remaining {
                break;
            }
            remaining -= work;
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = xf.to_doc(screen(x, y));
                    if ab.rect.contains(p) {
                        let color = if checker {
                            let a = (((p.x - ab.rect.x0) / cell).floor() as i64).rem_euclid(2);
                            let b = (((p.y - ab.rect.y0) / cell).floor() as i64).rem_euclid(2);
                            let i = ((a + b) % 2) as usize;
                            setup.grid_colors.get(i).map_or(paper, crate::panels::c32)
                        } else {
                            paper
                        };
                        sample.put_pixel(x, y, Rgba(color.to_array()));
                    }
                }
            }
        }
    }
    for (x, y, bg) in sample.enumerate_pixels_mut() {
        let (sx, sy) = match key.pixel {
            Some([x0, y0, x1, y1]) => {
                let p = xf.to_doc(screen(x, y));
                if p.x < x0 as f64 || p.x >= x1 as f64 || p.y < y0 as f64 || p.y >= y1 as f64 {
                    continue;
                }
                (
                    ((p.x - x0 as f64) / x1.saturating_sub(x0).max(1) as f64 * img.width as f64) as u32,
                    ((p.y - y0 as f64) / y1.saturating_sub(y0).max(1) as f64 * img.height as f64) as u32,
                )
            }
            None => (x * img.width / w, y * img.height / h),
        };
        let offset = (sy as usize * img.width as usize + sx as usize).saturating_mul(4);
        if let Some([r, g, b, a]) = img.pixels.get(offset..offset.saturating_add(4)) {
            let blend = |src: u8, dst: u8| (src as u16 + (dst as u16 * (255 - *a as u16) + 127) / 255).min(255) as u8;
            *bg = Rgba([blend(*r, bg.0[0]), blend(*g, bg.0[1]), blend(*b, bg.0[2]), 255]);
        }
    }
    let cache = &mut app.canvas.glass;
    cache.serial = cache.serial.wrapping_add(1);
    cache.source = Some(Snapshot { serial: cache.serial, key: key.clone(), rect, image: sample, sigma: BLUR_POINTS / scale, blur_ms: 0.0 });
}

pub fn edge(painter: &Painter, rect: Rect, t: &Tokens, rounded: bool) {
    if t.glass {
        let radius = if rounded { CornerRadius::same(8) } else { CornerRadius::ZERO };
        painter.rect_stroke(rect.shrink(0.5), radius, Stroke::new(1.0, t.glass_edge), StrokeKind::Inside);
    }
}

pub fn frame<R>(ui: &mut Ui, contents: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    let t = Tokens::get(ui.ctx());
    let frame = egui::Frame::popup(ui.style())
        .fill(if t.glass { Color32::TRANSPARENT } else { t.panel })
        .corner_radius(CornerRadius::same(if t.glass { 8 } else { 4 }))
        .inner_margin(egui::Margin::ZERO);
    let mut prepared = frame.begin(ui);
    let background = prepared.content_ui.painter().add(Shape::Noop);
    let inner = contents(&mut prepared.content_ui);
    let rect = frame.fill_rect(prepared.content_ui.min_rect());
    if t.glass {
        let painter = prepared.content_ui.painter();
        let mut shapes = vec![Shape::rect_filled(rect, 8, t.panel)];
        if let Some(scene) = ui.ctx().data(|d| d.get_temp::<Scene>(Id::new("glass-scene"))) {
            let r = rect.intersect(scene.rect);
            if r.is_positive() {
                let uv = |p: egui::Pos2| pos2((p.x - scene.rect.left()) / scene.rect.width(), (p.y - scene.rect.top()) / scene.rect.height());
                let image =
                    egui::epaint::RectShape::filled(r, 8, Color32::WHITE).with_texture(scene.texture.id(), Rect::from_min_max(uv(r.min), uv(r.max)));
                shapes.push(Shape::Rect(image));
                shapes.push(Shape::rect_filled(rect, 8, t.glass_tint));
            }
        }
        shapes.push(Shape::rect_stroke(rect.shrink(0.5), 8, Stroke::new(1.0, t.glass_edge), StrokeKind::Inside));
        painter.set(background, Shape::Vec(shapes));
    }
    let response = prepared.end(ui);
    egui::InnerResponse { inner, response }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vectorcraft_engine::Session;

    fn canvas() -> (VectorcraftApp, egui::Context, CacheKey) {
        let mut app = VectorcraftApp::new(Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 400, "height": 400})).unwrap();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                app.logic(ui.ctx());
                app.ui(ui);
            });
            out.textures_delta.clear();
        }
        let key = app.canvas.key.clone().unwrap();
        (app, ctx, key)
    }

    #[test]
    fn glass_switch_persists_without_editing_the_document() {
        let (mut app, ctx, _) = canvas();
        let before = app.session.active().unwrap().doc.clone();
        app.run("window.glass", json!({"enabled": false})).unwrap();
        crate::prefs_dialog::apply_runtime(&mut app, &ctx);
        assert!(!Tokens::get(&ctx).glass);
        assert!(app.canvas.glass.source.is_none());
        assert_eq!(crate::menus::checked(&app, "window.glass", &json!({})), Some(false));
        let saved: crate::state::UiState = serde_json::from_value(serde_json::to_value(&app.ui).unwrap()).unwrap();
        assert!(!saved.glass);
        crate::prefs_dialog::open(&mut app, Some("User Interface"));
        app.ui.dialog.as_mut().unwrap().fields.insert("__glass".into(), json!(true));
        crate::prefs_dialog::confirm(&mut app).unwrap();
        crate::prefs_dialog::apply_runtime(&mut app, &ctx);
        assert!(Tokens::get(&ctx).glass);
        assert_eq!(Tokens::get(&ctx).input.a(), 255);
        assert_eq!(Tokens::get(&ctx).pasteboard.a(), 255);
        assert!(Arc::ptr_eq(&before, &app.session.active().unwrap().doc));
    }

    #[test]
    fn backdrop_is_bounded_blurs_edges_and_does_not_follow_a_document_switch() {
        let (mut app, ctx, key) = canvas();
        let art = vectorcraft_render::Rendered { width: 2, height: 1, pixels: vec![0, 0, 0, 255, 255, 255, 255, 255] };
        capture(&mut app, &ctx, &art, &key);
        let source = app.canvas.glass.source.as_ref().unwrap();
        let mut blurred = source.clone();
        blurred.image = image::imageops::fast_blur(&source.image, source.sigma);
        assert!(blurred.image.pixels().any(|p| p.0[0] > 10 && p.0[0] < 245));
        assert!(source.image.pixels().all(|p| p.0[0] == 0 || p.0[0] == 255));
        app.canvas.glass.accept(&ctx, blurred, key.doc);
        let xf = source_xf(&key, app.canvas_rect.unwrap());
        begin_frame(&ctx);
        app.canvas.glass.publish(&ctx, &xf, key.doc);
        assert!(ctx.data(|d| d.get_temp::<Scene>(Id::new("glass-scene"))).is_some());
        begin_frame(&ctx);
        app.canvas.glass.publish(&ctx, &xf, key.doc + 1);
        assert!(ctx.data(|d| d.get_temp::<Scene>(Id::new("glass-scene"))).is_none());

        app.canvas_rect = Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(16000.0, 9000.0)));
        capture(&mut app, &ctx, &art, &key);
        let source = app.canvas.glass.source.as_ref().unwrap();
        assert!(source.image.width() <= MAX_EDGE && source.image.height() <= MAX_EDGE);
        assert!(source.image.as_raw().len() <= MAX_EDGE as usize * MAX_EDGE as usize * 4);
    }

    #[test]
    fn native_material_failure_uses_opaque_chrome() {
        let ctx = egui::Context::default();
        let mut panels = Vec::new();
        for b in crate::theme::Brightness::ALL {
            crate::theme::apply_material(&ctx, b, true);
            assert_eq!(Tokens::get(&ctx).surface.a(), 255);
            set_native_material(&ctx, true);
            crate::theme::apply_material(&ctx, b, true);
            assert!(Tokens::get(&ctx).surface.a() < 255);
            assert_eq!(Tokens::get(&ctx).input.a(), 255);
            panels.push(Tokens::get(&ctx).panel);
            set_native_material(&ctx, false);
        }
        assert!(panels.windows(2).all(|pair| pair[0] != pair[1]));
    }
}
