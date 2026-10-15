//! EPS Options (Export As → EPS): the preview (none, TIFF black and white or colour, transparent
//! or opaque), overprints and the flattener preset transparency is flattened with, fonts, the
//! options (linked files, thumbnails, CMYK PostScript, compatible gradients) and the PostScript
//! language level.
//!
//! Fields: `document.export` params (`format: eps`, `path?`, the artboard choice) and the EPS
//! options of `document.exportEps`. OK remembers the options for next time and writes the file
//! (asking where when there is no path); bad options keep the dialog open.

use serde_json::{Map, Value, json};
use vectorcraft_engine::cmd::fileio::SaveMode;
use vectorcraft_engine::cmd::{FlattenOptions, fileio};
use vectorcraft_eps::{Level, Overprint, Preview};

use super::DialogSpec;
use super::document_setup::{LABEL, check, choice};
use crate::state::Dialog;
use crate::{VectorcraftApp, io, widgets};

pub(super) const KIND: &str = "epsOptions";

pub(super) const SPEC: DialogSpec = DialogSpec {
    heading: |_| tl!("EPS Options").into(),
    body,
    confirm,
    ok: Some("Export"),
    ok_label: Some(|app| if app.ui.dialog.as_ref().is_some_and(|d| d.fields.contains_key("__save")) { "Save" } else { "Export" }),
    min_width: 420.0,
    max_width: Some(460.0),
    scroll_body: true,
    ..DialogSpec::FORM
};

/// Fields that are not remembered between exports (they belong to one export).
const PER_EXPORT: [&str; 8] = ["format", "path", "useArtboards", "range", "artboard", "artboards", "selectedOnly", "acknowledgeLoss"];

/// The options a first EPS Options dialog starts from (the export's defaults).
fn defaults() -> Map<String, Value> {
    let medium = FlattenOptions::preset_label(FlattenOptions::PRESETS[1]).unwrap_or_default();
    let mut m = Map::new();
    for (k, v) in [
        ("previewFormat", json!(Preview::default().id())),
        ("transparentPreview", json!(true)),
        ("overprints", json!(Overprint::default().id())),
        ("flattenerPreset", json!(medium)),
        ("includeLinkedFiles", json!(false)),
        ("thumbnails", json!(true)),
        ("cmykPostScript", json!(true)),
        ("compatibleGradients", json!(false)),
        ("level", json!(Level::default().id())),
    ] {
        m.insert(k.into(), v);
    }
    m
}

/// Open EPS Options for an export whose params (path, artboard choice, any EPS option) are
/// `params`, over the options used last.
pub fn open(app: &mut VectorcraftApp, params: &Value) {
    let mut fields = defaults();
    for given in [&app.ui.eps_options, params] {
        if let Some(o) = given.as_object() {
            fields.extend(o.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
    }
    fields.insert("format".into(), json!("eps"));
    // A built-in preset named by id (`medium`) shows as its name.
    if let Some(label) = fields.get("flattenerPreset").and_then(Value::as_str).and_then(|p| FlattenOptions::preset_label(&p.to_ascii_lowercase())) {
        fields.insert("flattenerPreset".into(), json!(label));
    }
    app.ui.dialog = Some(Dialog::new(KIND, Value::Object(fields)));
}

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    widgets::subheader(ui, tl!("Preview"));
    let previews: Vec<(&str, &str)> = Preview::ALL.iter().map(|p| (p.id(), p.label())).collect();
    choice(ui, d, "previewFormat", tl!("Format:"), &previews);
    // Only the colour preview can keep transparency.
    let color = Preview::from_id(&d.str("previewFormat")) == Some(Preview::TiffColor);
    let transparent = d.bool("transparentPreview");
    ui.horizontal(|ui| {
        ui.add_space(LABEL + 8.0);
        for (label, on) in [(tl!("Transparent"), true), (tl!("Opaque"), false)] {
            if widgets::radio(ui, label, color && transparent == on, color) {
                d.fields.insert("transparentPreview".into(), json!(on));
            }
            ui.add_space(12.0);
        }
    });
    ui.add_space(8.0);
    widgets::subheader(ui, tl!("Transparency"));
    let overprints: Vec<(&str, &str)> = Overprint::ALL.iter().map(|o| (o.id(), o.label())).collect();
    choice(ui, d, "overprints", tl!("Overprints:"), &overprints);
    let presets = app.session.flattener_presets();
    let names: Vec<(&str, &str)> = presets.iter().map(|p| (p.name.as_str(), p.name.as_str())).collect();
    choice(ui, d, "flattenerPreset", tl!("Preset:"), &names);
    ui.add_space(8.0);
    widgets::subheader(ui, tl!("Fonts"));
    // Type is written as glyph outlines, which need no fonts.
    widgets::check(ui, tl!("Embed Fonts (for other applications)"), true, false);
    ui.add_space(8.0);
    widgets::subheader(ui, tl!("Options"));
    check(ui, d, "includeLinkedFiles", tl!("Include Linked Files"));
    check(ui, d, "thumbnails", tl!("Include Document Thumbnails"));
    check(ui, d, "cmykPostScript", tl!("Include CMYK PostScript in RGB Files"));
    check(ui, d, "compatibleGradients", tl!("Compatible Gradient and Gradient Mesh Printing"));
    ui.add_space(4.0);
    let levels: Vec<(&str, &str)> = Level::ALL.iter().map(|l| (l.id(), l.label())).collect();
    // A level set as a number (`level: 2`) reads as its id.
    if let Some(Value::Number(n)) = d.fields.get("level") {
        let id = n.to_string();
        d.fields.insert("level".into(), json!(id));
    }
    choice(ui, d, "level", tl!("PostScript:"), &levels);
    ui.add_space(8.0);
    super::form::caption(
        ui,
        if Level::from_id(&d.str("level")) == Some(Level::Two) || d.bool("compatibleGradients") {
            tl!("Transparency is flattened with the preset. Type is written as outlines; gradients as bands of colour.")
        } else {
            tl!("Transparency is flattened with the preset. Type is written as outlines; gradients stay smooth.")
        },
    );
    false
}

/// Check the options, remember them, and write the file(s).
fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    let params = fileio::eps::resolve(&super::form::params(d), &app.session.prefs.flattener_presets)?;
    // Nothing selected keeps the dialog open too.
    if let Some(st) = app.session.active() {
        fileio::export_source(st, &params).map_err(|e| e.to_string())?;
    }
    app.ui.eps_options = Value::Object(
        d.fields.iter().filter(|(k, _)| !k.starts_with("__") && !PER_EXPORT.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect(),
    );
    app.ui.dialog = None;
    let path = d.fields.get("path").and_then(Value::as_str).map(str::to_string);
    if let Some(mode) = SaveMode::of(&d.str("__save")) {
        return io::save(
            app,
            mode,
            &json!({ "path": path, "format": "eps", "acknowledgeLoss": params["acknowledgeLoss"], "options": params }),
            false,
        );
    }
    io::export(app, Some("eps"), path, &params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eps_save_options_scroll_without_hiding_the_heading_or_footer() {
        for brightness in crate::theme::Brightness::ALL {
            for scale in [1.0, 2.0] {
                for size in [egui::vec2(800.0, 500.0), egui::vec2(800.0, 360.0), egui::vec2(640.0, 500.0)] {
                    let ctx = egui::Context::default();
                    crate::theme::install_fonts(&ctx);
                    crate::theme::apply(&ctx, brightness);
                    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
                    app.run("file.new", json!({})).unwrap();
                    app.run("shape.rectangle", json!({"x":10,"y":10,"width":20,"height":20})).unwrap();
                    open(&mut app, &json!({"__save":"file.saveAs","path":"synthetic.eps"}));
                    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                    let mut out = egui::FullOutput::default();
                    for _ in 0..4 {
                        let mut raw = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
                        raw.viewports.get_mut(&egui::ViewportId::ROOT).unwrap().native_pixels_per_point = Some(scale);
                        out = ctx.run_ui(raw, |ui| crate::dialogs::show(&mut app, ui.ctx()));
                        out.textures_delta.clear();
                    }
                    let rect = ctx.memory(|m| m.area_rect(egui::Id::new(("dialog", KIND)))).unwrap();
                    assert!(screen.contains_rect(rect), "{brightness:?}/{scale}/{size:?}: {rect:?}");
                    for label in ["EPS Options", "Save", "Cancel"] {
                        let (clip, text) = out
                            .shapes
                            .iter()
                            .find_map(|s| match &s.shape {
                                egui::Shape::Text(t) if t.galley.text() == label => Some((s.clip_rect, t)),
                                _ => None,
                            })
                            .unwrap();
                        let bounds = text.galley.rect.translate(text.pos.to_vec2());
                        assert!(rect.contains_rect(bounds) && clip.contains_rect(bounds), "{size:?}: {label} clipped");
                    }
                }
            }
        }
    }
}
