//! Image Trace panel: preset, view, mode, palette (Color), threshold or colour count, and the
//! Advanced section (open or closed as it was last left): fidelity, method, and Create: Fills /
//! Strokes with the widest line stroked. Preview applies completed changes; with Preview off,
//! parameters stay staged until Trace / Update Trace. View changes don't recompute the trace or
//! discard staged parameters. Expand is available only after the parameters have been applied.

use egui::Ui;
use serde_json::{Value, json};
use vectorcraft_doc::{Node, NodeId, NodeKind, TraceView};
use vectorcraft_engine::cmd::swatchlib::DOCUMENT_SWATCHES;

use super::{first_selected, library_panel, pstate, set_pstate, swatches};
use crate::VectorcraftApp;
use crate::widgets::{self, menu_item};

const MODES: [(&str, &str); 4] = [("blackAndWhite", "Black and White"), ("grayscale", "Grayscale"), ("color", "Color"), ("logo", "Flat Logo")];

/// Color mode's palettes (`palette` param ids), in panel order.
const PALETTES: [&str; 4] = ["limited", "fullTone", "automatic", "documentLibrary"];

/// Palette `id`, in the UI language.
fn palette_label(id: &str) -> &'static str {
    match id {
        "fullTone" => tl!("Full Tone"),
        "automatic" => tl!("Automatic"),
        "documentLibrary" => tl!("Document Library"),
        _ => tl!("Limited"),
    }
}

/// The name of Document Library's library `key` (the document's swatches or a swatch library); a
/// library that is gone shows its key.
fn library_label(app: &VectorcraftApp, key: &str) -> String {
    match swatches::limit_name(app, key) {
        Some(_) if key == DOCUMENT_SWATCHES => tl!(swatches::DOCUMENT_SWATCHES).to_string(),
        Some(name) => library_panel::library_name(key, &name).to_string(),
        None => key.to_string(),
    }
}

/// Document Library's choices: the document's swatches, then the colour libraries (`current`
/// checked) → the key chosen.
fn library_list(app: &VectorcraftApp, ui: &mut Ui, current: &str) -> Option<String> {
    let document = menu_item(ui, swatches::DOCUMENT_SWATCHES, true, current == DOCUMENT_SWATCHES).then(|| DOCUMENT_SWATCHES.to_string());
    ui.separator();
    library_panel::library_items(ui, &swatches::colour_libraries(app), Some(current)).or(document)
}

/// Panel state: the preset name, the current parameters, the view and the last result's counts.
#[derive(Clone)]
struct TraceUi {
    preset: String,
    params: Value,
    view: TraceView,
    info: Option<(u64, u64, u64)>,
    /// The selected Image Trace object's stored settings last adopted (so edits in progress
    /// aren't overwritten until the object changes).
    synced: Value,
    target: Option<NodeId>,
    document: Option<u64>,
    preview: bool,
    dirty: bool,
}

impl Default for TraceUi {
    fn default() -> Self {
        Self {
            preset: String::new(),
            params: Value::Null,
            view: TraceView::default(),
            info: None,
            synced: Value::Null,
            target: None,
            document: None,
            preview: true,
            dirty: false,
        }
    }
}

fn presets(app: &mut VectorcraftApp) -> Vec<(String, Value)> {
    let v = app.session.execute("imageTrace.presets", &json!({})).unwrap_or_default();
    v["presets"]
        .as_array()
        .map(|a| a.iter().map(|p| (p["name"].as_str().unwrap_or("").to_string(), p["params"].clone())).collect())
        .unwrap_or_default()
}

/// Whether `n` is an Image Trace object: a group so named whose first child is the traced image.
pub(crate) fn is_trace(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Group { clip: false, .. })
        && n.name.as_deref() == Some("Image Trace")
        && n.children().is_some_and(|c| c.first().is_some_and(|i| matches!(i.kind, NodeKind::Image(_))))
}

/// What the selection is: an Image Trace object (with its stored settings), a plain image, or neither.
fn target(app: &VectorcraftApp) -> (bool, bool, Option<Value>) {
    if !app.session.active().is_some_and(|s| s.selection.objects.len() == 1) {
        return (false, false, None);
    }
    let Some(n) = first_selected(app) else { return (false, false, None) };
    (is_trace(&n), matches!(n.kind, NodeKind::Image(_)), n.trace.map(|t| *t))
}

/// The one selected Image Trace object's preset ("Custom" once its settings were changed) and view.
pub(crate) fn selected_trace(app: &VectorcraftApp) -> Option<(String, TraceView)> {
    let st = app.session.active()?;
    let [id] = st.selection.objects[..] else { return None };
    let n = st.doc.node(id).filter(|n| is_trace(n))?;
    Some((n.trace.as_ref().and_then(|t| t["preset"].as_str()).unwrap_or("Custom").to_string(), n.trace_view()))
}

/// Image Trace › View, in the UI language.
fn view_label(v: TraceView) -> &'static str {
    match v {
        TraceView::Result => tl!("Tracing Result"),
        TraceView::ResultWithOutlines => tl!("Tracing Result with Outlines"),
        TraceView::Outlines => tl!("Outlines"),
        TraceView::OutlinesWithSource => tl!("Outlines with Source Image"),
        TraceView::Source => tl!("Source Image"),
    }
}

/// The View dropdown showing `current` → the view chosen.
fn view_dropdown(ui: &mut Ui, current: TraceView, width: f32) -> Option<TraceView> {
    let labels = TraceView::ALL.map(view_label);
    ui.scope(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        widgets::dropdown_names(ui, "trace-view", view_label(current), &labels, width).and_then(|i| TraceView::ALL.get(i).copied())
    })
    .inner
}

/// Show the selected Image Trace objects with `view` (no new trace).
fn set_view(app: &mut VectorcraftApp, view: TraceView) {
    if let Err(e) = app.run("imageTrace.setView", json!({ "view": view.id() })) {
        app.ui.status = e;
    }
}

/// The selected Image Trace object's View row (Control bar, Properties).
pub(crate) fn view_row(app: &mut VectorcraftApp, ui: &mut Ui, current: TraceView, width: f32) {
    widgets::dim_label(ui, tl!("View:"));
    if let Some(v) = view_dropdown(ui, current, width.min(ui.available_width())) {
        set_view(app, v);
    }
}

/// Trace the selected image, or trace the selected Image Trace object again, with built-in `preset`.
fn trace_with(app: &mut VectorcraftApp, preset: &str) {
    // A failure is reported in the status bar.
    if app.run("imageTrace.make", json!({ "preset": preset })).is_ok() {
        app.ui.open_panel = Some("imageTrace".into());
    }
}

/// The built-in presets to choose from (`current` highlighted) → the one chosen. Listed only
/// while the menu is open.
fn preset_list(app: &mut VectorcraftApp, ui: &mut Ui, current: &str) -> Option<String> {
    let mut chosen = None;
    for (name, _) in presets(app) {
        if ui.add(egui::Button::selectable(name == current, tl!(&name))).clicked() {
            chosen = Some(name);
        }
    }
    chosen
}

/// Width of the Control bar's Image Trace button.
pub(crate) const TRACE_BUTTON_W: f32 = 104.0;

/// The Image Trace button for a selected image (Control bar, Properties): a click traces it with
/// the Default preset, its arrow lists the presets to trace it with.
pub(crate) fn trace_button(app: &mut VectorcraftApp, ui: &mut Ui, width: f32) {
    let (main, arrow) = widgets::split_button(ui, tl!("Image Trace"), width);
    if main.on_hover_text(tl!("Trace the image with the Default preset")).clicked() {
        trace_with(app, "Default");
    }
    let arrow = arrow.on_hover_text(tl!("Tracing Presets"));
    if let Some(p) = egui::Popup::menu(&arrow).show(|ui| preset_list(app, ui, "")).and_then(|r| r.inner) {
        trace_with(app, &p);
    }
}

/// The selected Image Trace object's preset (Control bar, Properties): choosing another traces it
/// again with that preset.
pub(crate) fn preset_dropdown(app: &mut VectorcraftApp, ui: &mut Ui, current: &str, width: f32) {
    let chosen = ui
        .scope(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            widgets::combo(ui, "trace-preset", tl!(current), width, false, |ui| preset_list(app, ui, current))
        })
        .inner;
    if let Some(p) = chosen {
        trace_with(app, &p);
    }
}

/// The Control bar for one selected Image Trace object: its preset, view, the Image Trace panel
/// and Expand. Whether it is one.
pub fn control_bar(app: &mut VectorcraftApp, ui: &mut Ui) -> bool {
    let Some((preset, view)) = selected_trace(app) else { return false };
    widgets::dim_label(ui, tl!("Preset:"));
    preset_dropdown(app, ui, &preset, 150.0);
    view_row(app, ui, view, 190.0);
    if widgets::icon_button(ui, "image", tl!("Image Trace"), false, 24.0).clicked() {
        app.ui.open_panel = Some("imageTrace".into());
    }
    if widgets::flat_button(ui, tl!("Expand"), 64.0).clicked() {
        app.run("imageTrace.expand", json!({})).ok();
    }
    ui.separator();
    true
}

fn trace(app: &mut VectorcraftApp, st: &mut TraceUi) {
    let preset = if st.preset == "Custom" { "Default" } else { st.preset.as_str() };
    match app.run("imageTrace.make", json!({ "preset": preset, "params": st.params, "view": st.view.id() })) {
        Ok(r) => {
            st.info = Some((r["paths"].as_u64().unwrap_or(0), r["anchors"].as_u64().unwrap_or(0), r["colors"].as_u64().unwrap_or(0)));
            st.dirty = false;
            st.target = first_selected(app).map(|n| n.id);
            st.document = app.session.active().map(|s| s.uid);
            st.synced = target(app).2.unwrap_or_default();
            st.preset = st.synced["preset"].as_str().unwrap_or("Custom").into();
            st.params = st.synced["params"].clone();
        }
        Err(e) => app.ui.status = e,
    }
}

fn slider(ui: &mut Ui, label: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>, suffix: &str) -> (bool, bool) {
    let mut out = (false, false);
    widgets::label_row(ui, label, 62.0, |ui| {
        let t = crate::theme::Tokens::get(ui.ctx());
        let (min, max) = (*range.start(), *range.end());
        let width = (ui.available_width() - 62.0 - ui.spacing().item_spacing.x).max(24.0);
        let (new, phase) = widgets::color_slider(ui, ("trace-slider", label), ((*v - min) / (max - min)) as f32, width, &|_| t.input_border);
        if let Some(n) = new {
            let n = (min + n as f64 * (max - min)).round();
            out.0 = *v != n;
            *v = n;
        }
        out.1 = phase == widgets::Live::Released;
        if let Some(n) = widgets::plain_field(ui, ("trace-field", label), *v, suffix, 0, 62.0) {
            let n = n.round().clamp(min, max);
            out = (*v != n, *v != n);
            *v = n;
        }
    });
    out
}

fn sync(app: &VectorcraftApp, st: &mut TraceUi, stored: Option<Value>) {
    let id = app.session.active().and_then(|s| match s.selection.objects.as_slice() {
        [id] => Some(*id),
        _ => None,
    });
    let document = app.session.active().map(|s| s.uid);
    let switched = st.target != id || st.document != document;
    if switched {
        st.target = id;
        st.document = document;
        st.info = None;
        st.dirty = false;
        st.synced = Value::Null;
    }
    if let Some(t) = stored {
        // View changes must not discard parameters staged with Preview off.
        if switched || t["params"] != st.synced["params"] || t["preset"] != st.synced["preset"] {
            st.preset = t["preset"].as_str().unwrap_or("Custom").into();
            st.params = t["params"].clone();
            st.dirty = false;
            if let Some(n) = first_selected(app) {
                let (mut paths, mut anchors) = (0, 0);
                let mut colors = std::collections::BTreeSet::new();
                n.walk(&mut |n| {
                    if let Some(path) = n.path_data() {
                        paths += 1;
                        anchors += path.anchor_count() as u64;
                        for item in &n.appearance.items {
                            if item.visible()
                                && !item.paint().is_none()
                                && let Ok(paint) = serde_json::to_string(item.paint())
                            {
                                colors.insert(paint);
                            }
                        }
                    }
                });
                st.info = Some((paths, anchors, colors.len() as u64));
            }
        }
        st.view = TraceView::of(&t);
        st.synced = t;
    }
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let all = presets(app);
    let mut st: TraceUi = pstate(ui.ctx(), "image-trace");
    if st.params.is_null() {
        st.preset = "Default".into();
        st.params = all.first().map(|p| p.1.clone()).unwrap_or_default();
    }
    let (is_trace, is_image, stored) = target(app);
    sync(app, &mut st, stored);
    let mut retrace = false;
    if !is_trace && !is_image {
        widgets::dim_label(ui, tl!("Select an image to trace"));
    }
    ui.add_enabled_ui(is_trace || is_image, |ui| {
        widgets::label_row(ui, tl!("Preset:"), 62.0, |ui| {
            let names: Vec<&str> = all.iter().map(|p| p.0.as_str()).collect();
            if let Some(p) = widgets::dropdown(ui, "it-preset", &st.preset, &names, ui.available_width()).and_then(|i| all.get(i)) {
                st.preset = p.0.clone();
                st.params = p.1.clone();
                retrace = true;
            }
        });
        widgets::label_row(ui, tl!("View:"), 62.0, |ui| {
            if let Some(v) = view_dropdown(ui, st.view, ui.available_width()) {
                st.view = v;
                if is_trace {
                    set_view(app, v);
                }
            }
        });
        widgets::label_row(ui, tl!("Mode:"), 62.0, |ui| {
            let mode = st.params["mode"].as_str().unwrap_or("blackAndWhite").to_string();
            let label = MODES.iter().find(|m| m.0 == mode).map_or("Black and White", |m| m.1);
            let labels: Vec<&str> = MODES.iter().map(|m| m.1).collect();
            if let Some(m) = widgets::dropdown(ui, "it-mode", label, &labels, ui.available_width()).and_then(|i| MODES.get(i)) {
                st.params["mode"] = json!(m.0);
                st.preset = "Custom".into();
                retrace = true;
            }
        });
        let mode = st.params["mode"].as_str().unwrap_or("blackAndWhite").to_string();
        let palette = st.params["palette"].as_str().unwrap_or("limited").to_string();
        if mode == "color" {
            widgets::label_row(ui, tl!("Palette:"), 62.0, |ui| {
                let labels = PALETTES.map(palette_label);
                if let Some(p) =
                    widgets::dropdown_names(ui, "it-palette", palette_label(&palette), &labels, ui.available_width()).and_then(|i| PALETTES.get(i))
                {
                    st.params["palette"] = json!(p);
                    st.preset = "Custom".into();
                    retrace = true;
                }
            });
            if palette == "documentLibrary" {
                widgets::label_row(ui, tl!("Library:"), 62.0, |ui| {
                    let key = st.params["library"].as_str().unwrap_or(DOCUMENT_SWATCHES).to_string();
                    if let Some(k) =
                        widgets::combo(ui, "it-library", &library_label(app, &key), ui.available_width(), false, |ui| library_list(app, ui, &key))
                    {
                        st.params["library"] = json!(k);
                        st.preset = "Custom".into();
                        retrace = true;
                    }
                });
            }
        }
        // Full Tone and Automatic: how many colours (Less … More); else a count or the threshold.
        let (key, label, range, suffix) = match mode.as_str() {
            "blackAndWhite" => ("threshold", tl!("Threshold"), 0.0..=255.0, ""),
            "grayscale" => ("colors", tl!("Grays"), 2.0..=256.0, ""),
            // Flat Logo finds a handful of flat colours: more than ten is almost always noise.
            "logo" => ("colors", tl!("Colors"), 2.0..=10.0, ""),
            _ if matches!(palette.as_str(), "fullTone" | "automatic") => ("colorDetail", tl!("Colors"), 0.0..=100.0, "%"),
            _ => ("colors", tl!("Colors"), 2.0..=256.0, ""),
        };
        let mut v = st.params[key].as_f64().unwrap_or(0.0);
        let (changed, release) = slider(ui, label, &mut v, range, suffix);
        if changed {
            st.params[key] = if key == "colorDetail" { json!(v.round()) } else { json!(v.round() as u64) };
            st.preset = "Custom".into();
        }
        retrace |= release;

        let ignore = st.params["ignoreWhite"].as_bool().unwrap_or(false);
        if widgets::check(ui, tl!("Ignore White"), ignore, true) {
            st.params["ignoreWhite"] = json!(!ignore);
            st.preset = "Custom".into();
            retrace = true;
        }

        ui.add_space(4.0);
        let open = app.ui.image_trace_advanced;
        if widgets::section_toggle(ui, tl!("Advanced"), open) {
            app.ui.image_trace_advanced = !open;
        }
        if open {
            for (key, label, range, suffix) in [
                ("paths", tl!("Paths"), 0.0..=100.0, "%"),
                ("corners", tl!("Corners"), 0.0..=100.0, "%"),
                ("noise", tl!("Noise"), 1.0..=100.0, " px"),
            ] {
                let mut v = st.params[key].as_f64().unwrap_or(0.0);
                let (changed, release) = slider(ui, label, &mut v, range, suffix);
                if changed {
                    st.params[key] = if key == "noise" { json!(v.round() as u64) } else { json!(v.round()) };
                    st.preset = "Custom".into();
                }
                retrace |= release;
            }
            // Flat Logo always paints biggest shapes first with a hair of overlap, so there is no choice.
            if st.params["mode"] != "logo" {
                widgets::label_row(ui, tl!("Method:"), 62.0, |ui| {
                    let choices = [("abutting", tl!("Abutting")), ("overlapping", tl!("Overlapping"))];
                    let current = choices.iter().find(|(m, _)| st.params["method"] == *m).map_or(choices[0].1, |(_, l)| *l);
                    if let Some((m, _)) =
                        widgets::dropdown_names(ui, "it-method", current, &choices.map(|(_, l)| l), ui.available_width()).and_then(|i| choices.get(i))
                        && st.params["method"] != *m
                    {
                        st.params["method"] = json!(m);
                        st.preset = "Custom".into();
                        retrace = true;
                    }
                });
                // Create: Fills and/or Strokes (one of them stays on), and the widest line stroked.
                let create = |p: &Value, key: &str| p[key].as_bool().unwrap_or(key == "fills");
                let strokes = create(&st.params, "strokes");
                widgets::label_row(ui, tl!("Create:"), 62.0, |ui| {
                    for (key, other, label) in [("fills", "strokes", tl!("Fills")), ("strokes", "fills", tl!("Strokes"))] {
                        let on = create(&st.params, key);
                        if widgets::check(ui, label, on, !on || create(&st.params, other)) {
                            st.params[key] = json!(!on);
                            st.preset = "Custom".into();
                            retrace = true;
                        }
                    }
                });
                let mut v = st.params["strokeWidth"].as_f64().unwrap_or(10.0);
                let (changed, release) = ui.add_enabled_ui(strokes, |ui| slider(ui, tl!("Stroke"), &mut v, 1.0..=100.0, " px")).inner;
                if changed {
                    st.params["strokeWidth"] = json!(v.round());
                    st.preset = "Custom".into();
                }
                retrace |= release;
            }
            let on = st.params["snapCurvesToLines"].as_bool().unwrap_or(false);
            if widgets::check(ui, tl!("Snap Curves To Lines"), on, true) {
                st.params["snapCurvesToLines"] = json!(!on);
                st.preset = "Custom".into();
                retrace = true;
            }
        }
    });
    st.dirty |= retrace || st.params != st.synced["params"] && is_trace;
    widgets::divider(ui);
    if let Some((p, a, c)) = st.info {
        let summary = crate::i18n::fmt(
            tl!("Paths: {paths}    Anchors: {anchors}    Colors: {colors}"),
            &[("paths", &p.to_string()), ("anchors", &a.to_string()), ("colors", &c.to_string())],
        );
        ui.horizontal_wrapped(|ui| {
            for (label, count) in [(tl!("Paths:"), p), (tl!("Anchors:"), a), (tl!("Colors:"), c)] {
                widgets::dim_label(ui, &format!("{label} {count}")).on_hover_text(&summary);
            }
        });
    }
    if st.dirty {
        widgets::dim_label(ui, tl!("Unapplied settings"));
    }
    if widgets::check(ui, tl!("Preview"), st.preview, is_trace || is_image) {
        st.preview = !st.preview;
        retrace = st.preview && st.dirty;
    }
    let mut apply = false;
    ui.horizontal(|ui| {
        let width = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(40.0);
        let r = ui
            .add_enabled_ui(is_trace || is_image, |ui| widgets::flat_button(ui, if is_trace { tl!("Update Trace") } else { tl!("Trace") }, width))
            .inner;
        if r.on_disabled_hover_text(tl!("Select an image to trace")).clicked() {
            apply = true;
        }
        if ui
            .add_enabled_ui(is_trace && !st.dirty, |ui| widgets::flat_button(ui, tl!("Expand"), width))
            .inner
            .on_hover_text(tl!("Convert the trace to editable paths"))
            .on_disabled_hover_text(tl!("Apply tracing settings before expanding"))
            .clicked()
        {
            app.run("imageTrace.expand", json!({})).ok();
        }
    });
    if apply || retrace && st.preview && (is_trace || is_image) {
        trace(app, &mut st);
    }
    set_pstate(ui.ctx(), "image-trace", st);
}

pub fn menu(app: &mut VectorcraftApp, ui: &mut Ui) {
    let (is_trace, _, _) = target(app);
    let st: TraceUi = pstate(ui.ctx(), "image-trace");
    let pending = st.target == first_selected(app).map(|n| n.id) && st.dirty;
    if menu_item(ui, tl!("Release"), is_trace, false) {
        app.run("imageTrace.release", json!({})).ok();
    }
    if menu_item(ui, tl!("Expand"), is_trace && !pending, false) {
        app.run("imageTrace.expand", json!({})).ok();
    }
    ui.separator();
    if menu_item(ui, tl!("Reset to Default"), true, false) {
        let mut st: TraceUi = pstate(ui.ctx(), "image-trace");
        if let Some((name, params)) = presets(app).into_iter().find(|p| p.0 == "Default") {
            st.preset = name;
            st.params = params;
            st.dirty = true;
            if st.preview && is_trace {
                trace(app, &mut st);
            }
            set_pstate(ui.ctx(), "image-trace", st);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton, Pos2, Rect, vec2};

    /// One headless frame of the panel (and its menu) with `events`: the texts drawn, with where.
    fn frame(app: &mut VectorcraftApp, ctx: &egui::Context, events: Vec<Event>) -> Vec<(String, Rect)> {
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(280.0, 700.0));
        let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, |ui| show(app, ui));
        out.textures_delta.clear();
        crate::tests_removeanchors::shapes_text(&out.shapes.iter().map(|c| c.shape.clone()).collect::<Vec<_>>())
    }

    /// Click the panel's `label` (the first one drawn).
    fn click(app: &mut VectorcraftApp, ctx: &egui::Context, label: &str) {
        let texts = frame(app, ctx, vec![]);
        let at = texts.iter().find(|(t, _)| t == label).map(|(_, r)| r.center()).unwrap_or_else(|| panic!("no `{label}` in {texts:?}"));
        let press = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(app, ctx, vec![Event::PointerMoved(at), press(true)]);
        frame(app, ctx, vec![press(false)]);
    }

    fn traced_app() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 100, "height": 100})).unwrap();
        let img = image::RgbaImage::from_fn(20, 20, |x, _| image::Rgba(if x < 10 { [0, 0, 0, 255] } else { [255, 255, 255, 255] }));
        let mut png = vec![];
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let data = vectorcraft_format::base64_encode(&png);
        app.run("file.place", json!({"name": "half.png", "dataBase64": data, "link": false})).unwrap();
        app.run("imageTrace.make", json!({"preset": "Default"})).unwrap();
        app
    }

    #[test]
    fn traced_properties_keep_labels_inside_the_dock_across_frames() {
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 1000.0));
        for width in [230.0, 300.0, 520.0] {
            let mut app = traced_app();
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx);
            crate::theme::apply(&ctx, Default::default());
            ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
                egui::Panel::right("dock").exact_size(width).show(ui, |_| {});
            })
            .textures_delta
            .clear();
            for view in TraceView::ALL {
                app.run("imageTrace.setView", json!({"view": view.id()})).unwrap();
                for k in 0..30 {
                    let mut out =
                        ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| crate::dock::show(&mut app, ui));
                    out.textures_delta.clear();
                    if k < 2 {
                        continue;
                    }
                    for shape in &out.shapes {
                        if let egui::Shape::Text(text) = &shape.shape
                            && matches!(text.galley.text(), "Preset:" | "View:" | "Appearance" | "Fill" | "Stroke")
                        {
                            let rect = Rect::from_min_size(text.pos, text.galley.size());
                            assert!(
                                shape.clip_rect.contains_rect(rect),
                                "{width}, {view:?}, frame {k}: {:?} at {rect:?} clipped by {:?}",
                                text.galley.text(),
                                shape.clip_rect
                            );
                        }
                    }
                    let dock = egui::containers::panel::PanelState::load(&ctx, egui::Id::new("dock")).unwrap().outer_rect;
                    assert!(dock.width() <= width + 1.0, "{view:?}, frame {k}: {width}-point dock keeps growing: {dock:?}");
                }
            }
        }
    }

    #[test]
    fn tracing_from_a_shortcut_opens_the_full_panel() {
        let mut app = traced_app();
        app.run("imageTrace.release", json!({})).unwrap();
        trace_with(&mut app, "Default");
        assert_eq!(app.ui.open_panel.as_deref(), Some("imageTrace"));
        assert!(selected_trace(&app).is_some());
    }

    #[test]
    fn advanced_is_a_section_that_stays_as_it_was_left() {
        let mut app = traced_app();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let has = |texts: &[(String, Rect)], s: &str| texts.iter().any(|(t, _)| t == s);
        let texts = frame(&mut app, &ctx, vec![]);
        assert!(app.ui.image_trace_advanced && has(&texts, "Advanced") && has(&texts, "Corners"), "open at first: {texts:?}");
        click(&mut app, &ctx, "Advanced");
        assert!(!app.ui.image_trace_advanced, "the header closes it");
        assert!(!has(&frame(&mut app, &ctx, vec![]), "Corners"));
        // Remembered with the UI preferences, across launches.
        let saved: crate::state::UiState = serde_json::from_value(serde_json::to_value(&app.ui).unwrap()).unwrap();
        assert!(!saved.image_trace_advanced);
        let old: crate::state::UiState = serde_json::from_value(json!({})).unwrap();
        assert!(old.image_trace_advanced, "open for preferences saved before the section existed");
        click(&mut app, &ctx, "Advanced");
        assert!(app.ui.image_trace_advanced && has(&frame(&mut app, &ctx, vec![]), "Corners"));
    }

    #[test]
    fn the_view_dropdown_shows_and_sets_the_objects_view() {
        let mut app = traced_app();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let texts = frame(&mut app, &ctx, vec![]);
        assert!(texts.iter().any(|(t, _)| t == "View:") && texts.iter().any(|(t, _)| t == "Tracing Result"), "{texts:?}");
        let steps = app.session.active().unwrap().history.undo.len();
        click(&mut app, &ctx, "Tracing Result");
        click(&mut app, &ctx, "Source Image");
        assert_eq!(selected_trace(&app).map(|t| t.1), Some(TraceView::Source));
        assert_eq!(app.session.active().unwrap().history.undo.len(), steps + 1, "one undo step, no new trace");
        // Another object's view is shown when it is selected.
        app.run("imageTrace.setView", json!({"view": "outlines"})).unwrap();
        assert!(frame(&mut app, &ctx, vec![]).iter().any(|(t, _)| t == "Outlines"));
    }

    #[test]
    fn the_palette_shows_in_color_mode_and_traces_again() {
        let mut app = traced_app();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let has = |texts: &[(String, Rect)], s: &str| texts.iter().any(|(t, _)| t == s);
        let palette = |app: &VectorcraftApp| target(app).2.map(|t| t["params"]["palette"].clone());
        assert!(!has(&frame(&mut app, &ctx, vec![]), "Palette:"), "Black and White has no palette");
        app.run("imageTrace.make", json!({"preset": "6 Colors"})).unwrap();
        let texts = frame(&mut app, &ctx, vec![]);
        assert!(has(&texts, "Palette:") && has(&texts, "Limited") && !has(&texts, "Library:"), "{texts:?}");
        // Document Library traces again with the document's swatches, and names them.
        click(&mut app, &ctx, "Limited");
        click(&mut app, &ctx, "Document Library");
        assert_eq!(palette(&app), Some(json!("documentLibrary")));
        let texts = frame(&mut app, &ctx, vec![]);
        assert!(has(&texts, "Library:") && has(&texts, "Document Swatches"), "{texts:?}");
        // Full Tone's Colors is how many, Less (0%) … More (100%).
        click(&mut app, &ctx, "Document Library");
        click(&mut app, &ctx, "Full Tone");
        assert_eq!(palette(&app), Some(json!("fullTone")));
        let texts = frame(&mut app, &ctx, vec![]);
        assert!(has(&texts, "50%") && !has(&texts, "Library:"), "{texts:?}");
    }

    #[test]
    fn create_strokes_traces_again_and_keeps_one_kind_on() {
        let mut app = traced_app();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let create = |app: &VectorcraftApp| target(app).2.map(|t| (t["params"]["fills"].clone(), t["params"]["strokes"].clone()));
        let texts = frame(&mut app, &ctx, vec![]);
        for s in ["Create:", "Fills", "Strokes", "Stroke", "10 px"] {
            assert!(texts.iter().any(|(t, _)| t == s), "{s}: {texts:?}");
        }
        // Fills alone can't be turned off.
        click(&mut app, &ctx, "Fills");
        assert_eq!(create(&app), Some((json!(true), json!(false))));
        click(&mut app, &ctx, "Strokes");
        assert_eq!(create(&app), Some((json!(true), json!(true))), "traced again with strokes");
        click(&mut app, &ctx, "Fills");
        assert_eq!(create(&app), Some((json!(false), json!(true))));
        click(&mut app, &ctx, "Strokes");
        assert_eq!(create(&app), Some((json!(false), json!(true))), "Strokes alone stays on");
    }

    #[test]
    fn panel_and_menu_draw_headless() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.session.execute("file.new", &json!({"width": 100, "height": 100})).unwrap();
        app.session.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap();
        for _ in 0..2 {
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx);
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                show(&mut app, ui);
                menu(&mut app, ui);
            });
            out.textures_delta.clear();
        }
    }

    #[test]
    fn preview_off_stages_parameters_across_view_changes_and_applies_once() {
        let mut app = traced_app();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        frame(&mut app, &ctx, vec![]);
        let initial = target(&app).2.unwrap();
        let steps = app.session.active().unwrap().history.undo.len();
        click(&mut app, &ctx, "Preview");
        click(&mut app, &ctx, "Ignore White");
        click(&mut app, &ctx, "Snap Curves To Lines");
        assert_eq!(target(&app).2.unwrap(), initial, "staging doesn't modify artwork");
        assert_eq!(app.session.active().unwrap().history.undo.len(), steps);
        let st: TraceUi = pstate(&ctx, "image-trace");
        assert!(st.dirty && st.params["ignoreWhite"] == true && st.params["snapCurvesToLines"] == true);
        click(&mut app, &ctx, "Expand");
        assert!(selected_trace(&app).is_some(), "pending settings can't be expanded accidentally");
        click(&mut app, &ctx, "Tracing Result");
        click(&mut app, &ctx, "Source Image");
        frame(&mut app, &ctx, vec![]);
        let st: TraceUi = pstate(&ctx, "image-trace");
        assert!(st.dirty && st.params["ignoreWhite"] == true, "view change preserves pending edits");
        let before = app.session.active().unwrap().history.undo.len();
        click(&mut app, &ctx, "Update Trace");
        assert_eq!(app.session.active().unwrap().history.undo.len(), before + 1);
        let stored = target(&app).2.unwrap();
        assert_eq!(stored["params"]["ignoreWhite"], true);
        assert_eq!(stored["params"]["snapCurvesToLines"], true);
        assert_eq!(selected_trace(&app).unwrap().1, TraceView::Source);
        assert!(!pstate::<TraceUi>(&ctx, "image-trace").dirty);
        assert!(pstate::<TraceUi>(&ctx, "image-trace").info.is_some());
        app.run("edit.undo", json!({})).unwrap();
        frame(&mut app, &ctx, vec![]);
        assert_eq!(pstate::<TraceUi>(&ctx, "image-trace").params, initial["params"]);
    }

    #[test]
    fn panel_statistics_reappear_after_selection_switch_and_ignore_white_is_common() {
        let mut app = traced_app();
        app.ui.image_trace_advanced = false;
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let id = first_selected(&app).unwrap().id;
        let texts = frame(&mut app, &ctx, vec![]);
        assert!(texts.iter().any(|(s, _)| s == "Ignore White"));
        let info = pstate::<TraceUi>(&ctx, "image-trace").info.unwrap();
        assert!(info.0 > 0 && info.1 > 0 && info.2 > 0);
        app.run("select.none", json!({})).unwrap();
        frame(&mut app, &ctx, vec![]);
        assert!(pstate::<TraceUi>(&ctx, "image-trace").info.is_none());
        app.run("select.set", json!({"ids":[id.0]})).unwrap();
        frame(&mut app, &ctx, vec![]);
        assert_eq!(pstate::<TraceUi>(&ctx, "image-trace").info, Some(info));
    }

    #[test]
    fn full_trace_panel_fits_narrow_and_wide_layouts() {
        for width in [230.0, 280.0, 520.0] {
            for preset in ["Default", "6 Colors", "High Fidelity Photo"] {
                let mut app = traced_app();
                app.run("imageTrace.make", json!({"preset":preset})).unwrap();
                let ctx = egui::Context::default();
                crate::theme::install_fonts(&ctx);
                let screen = Rect::from_min_size(Pos2::ZERO, vec2(width, 900.0));
                for _ in 0..3 {
                    let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| show(&mut app, ui));
                    out.textures_delta.clear();
                    for shape in &out.shapes {
                        if let egui::Shape::Text(text) = &shape.shape {
                            let rect = Rect::from_min_size(text.pos, text.galley.size());
                            assert!(
                                shape.clip_rect.expand(1.0).contains_rect(rect),
                                "{width} / {preset}: {} at {rect:?}, clip {:?}",
                                text.galley.text(),
                                shape.clip_rect
                            );
                        }
                    }
                }
            }
        }
    }
}
