//! Ordered, transparent display surfaces for dense Selection-tool translations.
use std::{collections::HashSet, sync::Arc};

use egui::{Color32, Pos2};
use vectorcraft_color::BlendMode;
use vectorcraft_doc::{ColorMode, Document, Knockout, Node, NodeId, NodeKind, Selection};
use vectorcraft_engine::DeferredMove;
use vectorcraft_geom::{Affine, Rect, Vec2, union_opt};
use vectorcraft_render::{RenderOptions, Rendered, Renderer};

use crate::{CacheKey, VectorcraftApp, canvas::Xf, render_worker};

const MIN_ANCHORS: usize = 8_000;
const MAX_PARTS: usize = 8;
const MAX_PIXELS: u64 = 24 * 1024 * 1024;
const MAX_SIDE: u32 = 8192;

pub struct Scene {
    pub source: Arc<Document>,
    pub roots: Vec<NodeId>,
    pub bounds: Rect,
    parts: Vec<Part>,
}

struct Part {
    moving: bool,
    doc: Document,
    bounds: Option<Rect>,
}

pub struct PartImage {
    pub moving: bool,
    pub rect: Rect,
    pub image: Rendered,
}

struct Texture {
    moving: bool,
    rect: Rect,
    texture: egui::TextureHandle,
}

pub struct Cache {
    uid: u64,
    selection_revision: u64,
    pub scene: Arc<Scene>,
    requested: Option<CacheKey>,
    ready: Option<CacheKey>,
    textures: Vec<Texture>,
    movement: Option<DeferredMove>,
    committed: Option<Arc<Document>>,
    failed: bool,
    pub color: [u8; 3],
}

fn neutral(n: &Node) -> bool {
    n.opacity == 1.0
        && n.blend == BlendMode::Normal
        && n.knockout == Knockout::default()
        && n.mask.is_none()
        && n.appearance.items.is_empty()
        && n.appearance.effects.is_empty()
        && n.trace.is_none()
        && n.shaper.is_none()
        && matches!(n.kind, NodeKind::Group { clip: false, .. } | NodeKind::Layer { clip: false, template: false, preview: true, .. })
}

fn dense_enough(nodes: &[&Node]) -> bool {
    fn cost(n: &Node, left: &mut usize) -> bool {
        let anchors = match &n.kind {
            NodeKind::Path { path, .. } => path.anchor_count(),
            _ => 1,
        };
        if anchors >= *left {
            return true;
        }
        *left -= anchors;
        n.children().is_some_and(|children| children.iter().any(|n| cost(n, left)))
    }
    let mut left = MIN_ANCHORS;
    nodes.iter().any(|n| cost(n, &mut left))
}

impl Scene {
    pub fn new(source: Arc<Document>, selection: &Selection) -> Option<Self> {
        if source.color_mode != ColorMode::Rgb
            || source.mask_edit.is_some()
            || source.pattern_edit.is_some()
            || source.page_knockout
            || !source.text_threads.is_empty()
            || !selection.anchors.is_empty()
            || !selection.guides.is_empty()
        {
            return None;
        }
        let nodes = selection.root_nodes(&source);
        if nodes.is_empty() || !dense_enough(&nodes) {
            return None;
        }
        let mut dependent = false;
        for node in &nodes {
            node.walk(&mut |n| {
                dependent |= n.mask.as_ref().is_some_and(|m| !m.disabled && !m.linked)
                    || n.wrap.is_some()
                    || n.trace.as_ref().is_some_and(|_| n.trace_view() != vectorcraft_doc::TraceView::Result);
            });
        }
        if dependent {
            return None;
        }
        let roots: Vec<_> = nodes.iter().map(|n| n.id).collect();
        let bounds = nodes.iter().fold(None, |b, n| union_opt(b, vectorcraft_render::painted_bounds(n)))?;
        let selected: HashSet<_> = roots.iter().copied().collect();
        let mut holders = HashSet::new();
        fn gather(n: &Node, selected: &HashSet<NodeId>, holders: &mut HashSet<NodeId>) -> bool {
            if selected.contains(&n.id) {
                return true;
            }
            let mut has = false;
            for child in n.children().into_iter().flatten() {
                has |= gather(child, selected, holders);
            }
            if has {
                holders.insert(n.id);
            }
            has
        }
        for n in &source.layers {
            gather(n, &selected, &mut holders);
        }
        let mut chunks: Vec<(bool, Vec<Arc<Node>>)> = Vec::new();
        fn split(n: &Arc<Node>, selected: &HashSet<NodeId>, holders: &HashSet<NodeId>, chunks: &mut Vec<(bool, Vec<Arc<Node>>)>) -> Option<()> {
            if !n.visible {
                return Some(());
            }
            if holders.contains(&n.id) {
                if !neutral(n) {
                    return None;
                }
                for c in n.children()? {
                    split(c, selected, holders, chunks)?;
                }
                return Some(());
            }
            if n.blend != BlendMode::Normal || (!n.isolate && n.blends_through()) || n.wrap.is_some() {
                return None;
            }
            let moving = selected.contains(&n.id);
            if let Some((last, nodes)) = chunks.last_mut()
                && *last == moving
            {
                nodes.push(n.clone());
            } else {
                if chunks.len() >= MAX_PARTS {
                    return None;
                }
                chunks.push((moving, vec![n.clone()]));
            }
            Some(())
        }
        for n in &source.layers {
            split(n, &selected, &holders, &mut chunks)?;
        }
        if !chunks.iter().any(|(moving, _)| *moving) {
            return None;
        }
        let parts = chunks
            .into_iter()
            .map(|(moving, nodes)| {
                let bounds = moving.then(|| nodes.iter().fold(None, |b, n| union_opt(b, vectorcraft_render::painted_bounds(n)))).flatten();
                let mut doc = source.as_ref().clone();
                doc.layers = nodes;
                Part { moving, doc, bounds }
            })
            .collect();
        Some(Self { source, roots, bounds, parts })
    }

    fn regions(&self, w: u32, h: u32, view: Affine) -> Option<Vec<(Rect, u32, u32, Affine)>> {
        let scale = view.as_coeffs().first().copied()?.abs();
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let visible = view.inverse().transform_rect_bbox(Rect::new(0.0, 0.0, w as f64, h as f64));
        let mut pixels = 0_u64;
        self.parts
            .iter()
            .map(|part| {
                let (rect, w, h, transform) = if let Some(bounds) = part.bounds {
                    let r = bounds.inflate(2.0 / scale, 2.0 / scale);
                    let r = Rect::new(
                        (r.x0 * scale).floor() / scale,
                        (r.y0 * scale).floor() / scale,
                        (r.x1 * scale).ceil() / scale,
                        (r.y1 * scale).ceil() / scale,
                    );
                    let w = (r.width() * scale).round().max(1.0) as u32;
                    let h = (r.height() * scale).round().max(1.0) as u32;
                    (r, w, h, Affine::scale(scale) * Affine::translate((-r.x0, -r.y0)))
                } else {
                    (visible, w, h, view)
                };
                pixels = pixels.checked_add(u64::from(w) * u64::from(h))?;
                (w <= MAX_SIDE && h <= MAX_SIDE && pixels <= MAX_PIXELS).then_some((rect, w, h, transform))
            })
            .collect()
    }

    pub fn render(&self, renderer: &mut Renderer, w: u32, h: u32, view: Affine, opts: &RenderOptions) -> Result<Vec<PartImage>, String> {
        let regions = self.regions(w, h, view).ok_or("move preview exceeds its display memory budget")?;
        Ok(self
            .parts
            .iter()
            .zip(regions)
            .map(|(part, (rect, w, h, view))| PartImage { moving: part.moving, rect, image: renderer.render(&part.doc, w, h, view, opts) })
            .collect())
    }
}

fn supported(app: &VectorcraftApp) -> bool {
    !cfg!(target_arch = "wasm32")
        && !app.ui.view.outline
        && !app.ui.view.pixel_preview
        && !app.ui.view.trim_view
        && app.view().is_some_and(|v| v.rotation == 0.0)
        && !vectorcraft_render::proof::view().overprint
        && vectorcraft_render::proof::active_proof().is_none()
        && app.session.active().is_some_and(|st| st.shown_mask().is_none())
}

/// A press has resolved the selection. Prepare once; the following samples only change a delta.
pub fn prepare(app: &mut VectorcraftApp) {
    app.session.clear_move_preview();
    app.canvas.move_preview = None;
    if app.session.tool_id() != "selection" || !supported(app) || app.canvas.worker.is_none() {
        return;
    }
    let Some(st) = app.session.active() else { return };
    let Some(scene) = Scene::new(st.doc.clone(), &st.selection) else { return };
    if !app.session.prefs.transform_pattern_tiles {
        let mut pattern = false;
        for n in st.selection.root_nodes(&st.doc) {
            n.walk(&mut |n| pattern |= vectorcraft_doc::pattern::uses_pattern(n, None));
        }
        if pattern {
            return;
        }
    }
    let uid = st.uid;
    let selection_revision = st.revision;
    let color = scene.roots.first().map_or(vectorcraft_doc::LAYER_COLORS[0].1, |id| st.doc.layer_color(*id));
    app.session.prepare_move_preview(scene.roots.clone(), scene.bounds);
    app.canvas.move_preview = Some(Cache {
        uid,
        selection_revision,
        scene: Arc::new(scene),
        requested: None,
        ready: None,
        textures: vec![],
        movement: None,
        committed: None,
        failed: false,
        color,
    });
}

/// Resolve completed surfaces and release cancelled/switched/unsupported snapshots before drawing.
pub fn begin_frame(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if let Some(done) = app.canvas.worker.as_mut().and_then(render_worker::Worker::poll_scene)
        && let Some(cache) = &mut app.canvas.move_preview
        && cache.requested.as_ref() == Some(&done.key)
    {
        match done.images {
            Ok(images) => {
                cache.textures = images
                    .into_iter()
                    .map(|part| Texture {
                        moving: part.moving,
                        rect: part.rect,
                        texture: ctx.load_texture(
                            "move-preview",
                            egui::ColorImage::from_rgba_premultiplied([part.image.width as usize, part.image.height as usize], &part.image.pixels),
                            if done.key.anti_alias { egui::TextureOptions::LINEAR } else { egui::TextureOptions::NEAREST },
                        ),
                    })
                    .collect();
                cache.ready = Some(done.key);
            }
            Err(_) => cache.failed = true,
        }
    }
    let active = app.session.active();
    let drop_cache = app.canvas.move_preview.as_ref().is_some_and(|cache| {
        cache.failed
            || !supported(app)
            || active.is_none_or(|st| {
                st.uid != cache.uid
                    || cache.committed.as_ref().is_some_and(|doc| !Arc::ptr_eq(doc, &st.doc))
                    || (st.interaction.is_none() && cache.committed.is_none() && !app.session.tool_busy())
                    || (cache.movement.is_some()
                        && st.interaction.is_none()
                        && cache.committed.is_none()
                        && Arc::ptr_eq(&st.doc, &cache.scene.source))
            })
    });
    if drop_cache {
        if let Err(error) = app.session.materialize_deferred_move() {
            app.status(error.to_string());
        }
        app.session.clear_move_preview();
        app.canvas.move_preview = None;
        if let Some(worker) = &mut app.canvas.worker {
            worker.cancel_scene();
        }
    }
    track(app);
}

pub fn track(app: &mut VectorcraftApp) {
    let Some(cache) = &mut app.canvas.move_preview else { return };
    if let Some(movement) = app.session.deferred_move() {
        cache.movement = Some(movement);
    } else if let Some(st) = app.session.active()
        && st.uid == cache.uid
        && st.interaction.is_none()
        && cache.movement.is_some()
        && !Arc::ptr_eq(&st.doc, &cache.scene.source)
    {
        cache.committed = Some(st.doc.clone());
    }
}

pub fn submit(app: &mut VectorcraftApp, key: &CacheKey, w: u32, h: u32, view: Affine, opts: &RenderOptions) -> bool {
    let active = app.session.deferred_move().is_some();
    let Some(cache) = &mut app.canvas.move_preview else { return false };
    if !active || cache.committed.is_some() {
        return false;
    }
    let mut key = key.clone();
    key.move_preview = Some(cache.selection_revision);
    if cache.ready.as_ref() != Some(&key)
        && cache.requested.as_ref() != Some(&key)
        && let Some(worker) = &mut app.canvas.worker
    {
        cache.requested = Some(key.clone());
        worker.submit_scene(render_worker::Job { key, doc: cache.scene.source.clone(), w, h, view, opts: opts.clone() }, cache.scene.clone());
    }
    active
}

pub fn raster_ready(app: &mut VectorcraftApp) {
    if app
        .canvas
        .move_preview
        .as_ref()
        .is_some_and(|cache| cache.committed.as_ref().is_some_and(|doc| app.session.active().is_some_and(|st| Arc::ptr_eq(doc, &st.doc))))
    {
        app.canvas.move_preview = None;
        app.session.clear_move_preview();
    }
}

pub fn paint(app: &VectorcraftApp, painter: &egui::Painter, xf: &Xf) -> bool {
    let Some(cache) = &app.canvas.move_preview else { return false };
    let Some(movement) = cache.movement else { return false };
    if cache.textures.is_empty() || (app.session.deferred_move().is_none() && cache.committed.is_none()) {
        return false;
    }
    for part in &cache.textures {
        let draw = |offset: Vec2| {
            painter.image(
                part.texture.id(),
                xf.rect_to_screen(part.rect + offset),
                egui::Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            )
        };
        if part.moving {
            if movement.copy {
                draw(Vec2::ZERO);
            }
            draw(movement.offset);
        } else {
            draw(Vec2::ZERO);
        }
    }
    true
}

pub fn inspect(app: &VectorcraftApp) -> serde_json::Value {
    serde_json::json!({
        "deferred": app.session.deferred_move().is_some(),
        "ready": app.canvas.move_preview.as_ref().is_some_and(|c| !c.textures.is_empty()),
        "parts": app.canvas.move_preview.as_ref().map_or(0, |c| c.textures.len()),
        "finishing": app.canvas.move_preview.as_ref().is_some_and(|c| c.committed.is_some()),
    })
}
