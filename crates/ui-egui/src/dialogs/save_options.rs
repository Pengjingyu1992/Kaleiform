//! A save format's options before the file is written; on the web, where there is no save panel,
//! also the Save As dialog that names the file and picks the format (SVG and PDF then go on to
//! their own dialogs). The rows come from the format's options in the engine's format table, so a
//! format gains rows as its encoder gains options.
//!
//! Fields: `__action` (the save command it finishes), `__pick` (file name and format
//! editable), `path`, `format`, and one field per option (agents set them with `ui.dialog.set`).
//! Native and `.ai` files show their save options (Kaleiform Options after Save As): each
//! artboard to a separate file, with `all` (true: every artboard) or `range`, Include Linked Files,
//! Embed ICC Profiles, Create PDF-Compatible File and Use Compression.

use std::sync::OnceLock;

use serde_json::{Map, Value, json};
use vectorcraft_engine::cmd::fileio::{self, ArtboardPick, Format, FormatOption, SAVE_FORMATS, SaveMode};

use super::{DialogResult, DialogSpec, form};
use crate::state::Dialog;
use crate::theme::Tokens;
use crate::{VectorcraftApp, io, widgets};

/// Dialog kind.
pub const KIND: &str = "saveOptions";

pub(super) const SPEC: DialogSpec = DialogSpec { heading, body, confirm, min_width: 380.0, ..DialogSpec::FORM };

fn heading(d: &Dialog) -> String {
    if d.bool("__pick") {
        return match SaveMode::of(&d.str("__action")) {
            Some(SaveMode::Copy) => tl!("Save a Copy"),
            Some(SaveMode::Template) => tl!("Save as Template"),
            _ => tl!("Save As"),
        }
        .into();
    }
    crate::i18n::fmt(tl!("{format} Options"), &[("format", fileio::format(&d.str("format")).map_or("", |f| f.label))])
}

/// `f`'s options with the values a save would use (`file.formatOptions`: as last saved in that
/// format, else the defaults). Text options start empty rather than null.
pub(crate) fn option_values(app: &mut VectorcraftApp, f: &Format) -> Map<String, Value> {
    let v = app.session.execute("file.formatOptions", &json!({ "format": f.id })).unwrap_or_default();
    f.options
        .iter()
        .map(|o| {
            let value = v["options"][o.name]["value"].clone();
            (o.name.to_string(), if o.ty == "string" && value.is_null() { json!("") } else { value })
        })
        .collect()
}

/// Formats whose options have a dialog of their own (SVG Options, Save PDF), which follows this
/// one.
fn own_dialog(f: &Format) -> bool {
    matches!(f.id, "svg" | "svgz" | "pdf" | "eps" | "dxf")
}

/// Does `f` save each artboard to a separate file on request (native and `.ai` files)? Its
/// `range` then shows under that checkbox.
fn separates(f: &Format) -> bool {
    f.options.iter().any(|o| o.name == "separateArtboards")
}

/// The options the dialog shows: arrays and objects are for agents, and a page range replaces the
/// single artboard choice; none for a format with its own dialog.
fn shown(f: &Format) -> impl Iterator<Item = &'static FormatOption> {
    let range = f.options.iter().any(|o| o.name == "range");
    let separate = separates(f);
    let options = if own_dialog(f) { &[][..] } else { f.options };
    options.iter().filter(move |o| !(matches!(o.ty, "array" | "object") || (range && o.name == "artboard") || (separate && o.name == "range")))
}

/// An option's label: the save options as the reference app's options dialog names them, the
/// others from their names.
fn label(o: &FormatOption) -> String {
    match o.name {
        "separateArtboards" => tl!("Save each artboard to a separate file").into(),
        "includeLinked" => tl!("Include Linked Files").into(),
        "embedProfiles" => tl!("Embed ICC Profiles").into(),
        "pdfCompatible" => tl!("Create PDF-Compatible File").into(),
        "compress" => tl!("Use Compression").into(),
        _ => form::humanize(o.name),
    }
}

/// Every artboard (the All choice under Save each artboard to a separate file): `all` when set,
/// else no range given.
fn all_artboards(d: &Dialog) -> bool {
    d.fields.get("all").and_then(Value::as_bool).unwrap_or_else(|| d.str("range").trim().is_empty())
}

/// All or Range, under Save each artboard to a separate file (enabled while it is on).
fn artboard_range(ui: &mut egui::Ui, d: &mut Dialog) {
    ui.label("");
    ui.add_enabled_ui(d.bool("separateArtboards"), |ui| {
        ui.horizontal(|ui| {
            ui.add_space(22.0);
            let mut all = all_artboards(d);
            if ui.radio_value(&mut all, true, tl!("All")).changed() | ui.radio_value(&mut all, false, tl!("Range:")).changed() {
                d.fields.insert("all".into(), json!(all));
            }
            let mut range = d.str("range");
            let field = egui::TextEdit::singleline(&mut range).desired_width(90.0).hint_text("1-3, 5");
            if ui.add_enabled(!all, field).changed() {
                d.fields.insert("range".into(), json!(range));
            }
        });
    });
    ui.end_row();
}

/// The Save As format menu (labels in [`SAVE_FORMATS`] order).
fn save_labels() -> &'static [&'static str] {
    static LABELS: OnceLock<Vec<&'static str>> = OnceLock::new();
    LABELS.get_or_init(|| SAVE_FORMATS.iter().filter_map(|id| fileio::format(id)).map(|f| f.label).collect())
}

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    let t = Tokens::get(ui.ctx());
    let Some(f) = fileio::format(&d.str("format")) else { return false };
    let field_width = (ui.available_width() - 92.0).clamp(160.0, 254.0);
    let mut switch = None;
    egui::Grid::new("save-options").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        if d.bool("__pick") {
            form::field_w(ui, d, "path", tl!("File Name:"), field_width - 14.0);
            if SaveMode::of(&d.str("__action")) != Some(SaveMode::Template) {
                ui.label(egui::RichText::new(tl!("Format:")).color(t.text_dim));
                switch = widgets::dropdown(ui, "save-format", f.label, save_labels(), field_width).and_then(|i| SAVE_FORMATS.get(i).copied());
                ui.end_row();
            }
        }
        for o in shown(f) {
            option_row(app, ui, d, o);
        }
    });
    if let Some(id) = switch.filter(|id| *id != f.id) {
        switch_format(app, d, id);
    }
    false
}

/// One option: a checkbox, an artboard menu, a number or a text field.
fn option_row(app: &VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog, o: &FormatOption) {
    let t = Tokens::get(ui.ctx());
    let label = label(o);
    let value = d.fields.get(o.name).cloned().unwrap_or(Value::Null);
    if o.ty == "boolean" {
        ui.label("");
        let mut on = value.as_bool().unwrap_or(false);
        if ui.checkbox(&mut on, label.trim_end_matches(':')).on_hover_text(o.description).changed() {
            d.fields.insert(o.name.into(), json!(on));
        }
        ui.end_row();
        if o.name == "separateArtboards" {
            artboard_range(ui, d);
        }
        return;
    }
    if o.ty == "string" {
        form::field_w(ui, d, o.name, &label, 240.0);
        return;
    }
    ui.label(egui::RichText::new(&label).color(t.text_dim)).on_hover_text(o.description);
    let artboards = app.session.active().filter(|_| o.name == "artboard").map(|st| &st.doc.artboards);
    if let Some(boards) = artboards {
        let i = value.as_u64().unwrap_or(0) as usize;
        let names: Vec<&str> = boards.iter().map(|a| a.name.as_str()).collect();
        if let Some(i) = widgets::dropdown_names(ui, ("save-option", o.name), names.get(i).copied().unwrap_or(""), &names, 254.0) {
            d.fields.insert(o.name.into(), json!(i));
        }
    } else {
        let integer = o.ty == "integer";
        let x = value.as_f64().unwrap_or(0.0);
        let field = ui.scope(|ui| widgets::range_field(ui, ("save-option", o.name), x, 0.0..=f64::MAX, "", if integer { 0 } else { 2 }, 80.0));
        field.response.on_hover_text(o.description);
        if let Some(x) = field.inner {
            d.fields.insert(o.name.into(), if integer { json!(x as i64) } else { json!(x) });
        }
    }
    ui.end_row();
}

/// Web Save As: another format brings its own options and file extension.
fn switch_format(app: &mut VectorcraftApp, d: &mut Dialog, id: &str) {
    let (Some(old), Some(new)) = (fileio::format(&d.str("format")), fileio::format(id)) else { return };
    for o in old.options {
        d.fields.remove(o.name);
    }
    d.fields.extend(option_values(app, new));
    d.fields.insert("format".into(), json!(new.id));
    d.fields.insert("path".into(), json!(with_extension(&d.str("path"), new)));
}

/// `path` ending in `f`'s extension (kept when it already names `f`).
fn with_extension(path: &str, f: &Format) -> String {
    if fileio::format_for_name(path).is_some_and(|g| g.id == f.id) {
        return path.to_string();
    }
    std::path::Path::new(path).with_extension(f.extensions[0]).to_string_lossy().to_string()
}

/// Write the file with the chosen options (blank ones fall back to the format's defaults).
fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> DialogResult {
    let f = fileio::format(&d.str("format")).ok_or("unknown format")?;
    let path = d.str("path");
    if path.trim().is_empty() {
        return Err("name the file".into());
    }
    let path = if d.bool("__pick") { with_extension(path.trim(), f) } else { path };
    let mut options: Map<String, Value> = f
        .options
        .iter()
        .filter_map(|o| d.fields.get(o.name).filter(|v| !v.is_null() && v.as_str() != Some("")).map(|v| (o.name.to_string(), v.clone())))
        .collect();
    if separates(f) {
        if !d.bool("separateArtboards") || all_artboards(d) {
            options.remove("range");
        } else {
            // A bad range keeps the dialog open.
            let n = app.session.active().map_or(0, |s| s.doc.artboards.len());
            ArtboardPick { range: Some(d.str("range")), ..Default::default() }.resolve(n)?;
        }
    }
    app.ui.dialog = None;
    match SaveMode::of(&d.str("__action")) {
        Some(mode) if own_dialog(f) => io::ask_format_options(app, mode, f, &path, false),
        Some(mode) => io::save(app, mode, &json!({ "path": path, "format": f.id, "options": options }), false),
        None => Err(format!("`{}` is no save command", d.str("__action"))),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use vectorcraft_engine::Session;

    use super::*;
    use crate::{FilePick, Services, theme};

    type Written = Rc<RefCell<Vec<(String, Vec<u8>)>>>;

    /// A desktop app whose save panel answers `picked` and records what it was shown.
    fn desktop(picked: &'static str) -> (VectorcraftApp, Written, Rc<RefCell<Vec<FilePick>>>) {
        let (written, picks) = (Written::default(), Rc::new(RefCell::new(vec![])));
        let (w, p) = (written.clone(), picks.clone());
        let services = Services {
            pick_save: Some(Box::new(move |f: &FilePick| {
                p.borrow_mut().push(f.clone());
                Some(picked.to_string())
            })),
            write: Some(Box::new(move |path: &str, b: &[u8]| {
                w.borrow_mut().push((path.to_string(), b.to_vec()));
                Ok(())
            })),
            ..Default::default()
        };
        (with_art(services), written, picks)
    }

    /// A browser app: saves are downloads.
    fn web() -> (VectorcraftApp, Written) {
        let written = Written::default();
        let w = written.clone();
        let services = Services {
            download: Some(Box::new(move |name: &str, b: &[u8]| w.borrow_mut().push((name.to_string(), b.to_vec())))),
            ..Default::default()
        };
        (with_art(services), written)
    }

    fn with_art(services: Services) -> VectorcraftApp {
        let mut app = VectorcraftApp::new(Session::new(), services);
        app.run("file.new", json!({"width": 100, "height": 80})).unwrap();
        app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 30, "height": 20})).unwrap();
        app
    }

    /// One headless frame of the dialog layer; `enter` presses OK.
    fn frame(app: &mut VectorcraftApp, enter: bool) {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let events = if enter {
            vec![egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }]
        } else {
            vec![]
        };
        let mut out = ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| super::super::show(app, ui.ctx()));
        out.textures_delta.clear();
    }

    fn dialog(app: &VectorcraftApp) -> &Dialog {
        app.ui.dialog.as_ref().expect("a dialog is open")
    }

    #[test]
    fn enter_on_the_format_picker_does_not_confirm_the_save() {
        let (mut app, written) = web();
        app.run("file.saveAs", json!({})).unwrap();
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut run = |events| {
            ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| super::super::show(&mut app, ui.ctx())).textures_delta.clear();
        };
        for _ in 0..3 {
            run(vec![]);
        }
        let filename = ctx.memory(|m| m.focused()).expect("filename has initial focus");
        let key = |key| {
            [true, false].map(|pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Default::default() }).to_vec()
        };
        run(key(egui::Key::Tab));
        assert_ne!(ctx.memory(|m| m.focused()), Some(filename));
        run(key(egui::Key::Enter));
        assert!(egui::Popup::is_any_open(&ctx));
        assert_eq!(dialog(&app).kind, KIND);
        assert!(written.borrow().is_empty());
    }

    #[test]
    fn a_keyboard_focused_cancel_closes_without_writing() {
        let (mut app, written) = web();
        app.run("file.saveAs", json!({})).unwrap();
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |ui| super::super::show(&mut app, ui.ctx())).textures_delta.clear();
        }
        let cancel = ctx.viewport(|vp| {
            vp.prev_pass
                .widgets
                .layers()
                .flat_map(|(_, ws)| ws.iter())
                .filter(|w| w.rect.height() == 28.0 && w.sense.senses_click())
                .min_by(|a, b| a.rect.left().total_cmp(&b.rect.left()))
                .unwrap()
                .id
        });
        ctx.memory_mut(|m| m.request_focus(cancel));
        let events =
            vec![egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }];
        ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| super::super::show(&mut app, ui.ctx())).textures_delta.clear();
        assert!(app.ui.dialog.is_none());
        assert!(written.borrow().is_empty());
    }

    #[test]
    fn save_as_fits_short_narrow_windows_in_every_theme_and_scale() {
        for brightness in theme::Brightness::ALL {
            for scale in [1.0, 2.0] {
                let (mut app, written) = web();
                app.run("file.saveAs", json!({})).unwrap();
                let ctx = egui::Context::default();
                theme::install_fonts(&ctx);
                theme::apply(&ctx, brightness);
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 500.0));
                let mut out = egui::FullOutput::default();
                for _ in 0..4 {
                    let mut raw = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
                    raw.viewports.get_mut(&egui::ViewportId::ROOT).unwrap().native_pixels_per_point = Some(scale);
                    out = ctx.run_ui(raw, |ui| super::super::show(&mut app, ui.ctx()));
                    out.textures_delta.clear();
                }
                let rect = ctx.memory(|m| m.area_rect(egui::Id::new(("dialog", KIND)))).unwrap();
                assert!(screen.contains_rect(rect), "{brightness:?}/{scale}: {rect:?}");
                assert!(
                    out.shapes
                        .iter()
                        .filter_map(|s| match &s.shape {
                            egui::Shape::Text(t) => Some(t.galley.rect.translate(t.pos.to_vec2())),
                            _ => None,
                        })
                        .all(|r| screen.contains_rect(r)),
                    "save labels stay on screen"
                );
                assert!(written.borrow().is_empty());
            }
        }
    }

    #[test]
    fn first_save_suggests_ai_and_keeps_the_engine_native_default() {
        let (mut app, written, picks) = desktop("art");
        assert_eq!(fileio::save_plan(&app.session, SaveMode::Save, &json!({})).unwrap().format.id, "vectorcraft");
        let layers = app.session.doc().unwrap().doc.layers.clone();
        app.run("file.save", json!({})).unwrap();
        assert_eq!(picks.borrow()[0].name, "Untitled-1.ai");
        assert_eq!(picks.borrow()[0].filters[0], ("PDF-compatible .ai", &["ai"][..]));
        let (path, bytes) = written.borrow()[0].clone();
        assert_eq!(path, "art.ai");
        assert!(bytes.starts_with(b"%PDF") && vectorcraft_pdf::editing(&bytes).is_some_and(|e| e.intact));
        assert_eq!(fileio::load(&path, &bytes).unwrap().doc.layers, layers);
        assert_eq!(app.session.active().unwrap().format, "ai");
    }

    #[test]
    fn first_save_can_choose_eps_without_exporting() {
        let (mut app, written, picks) = desktop("art.eps");
        let layers = app.session.doc().unwrap().doc.layers.clone();
        app.run("file.save", json!({})).unwrap();
        assert_eq!(dialog(&app).kind, "epsOptions");
        assert_eq!(dialog(&app).str("__save"), "document.save");
        assert!(written.borrow().is_empty());
        super::super::confirm(&mut app).unwrap();
        let (path, bytes) = written.borrow()[0].clone();
        assert_eq!(path, "art.eps");
        assert_eq!(fileio::load(&path, &bytes).unwrap().doc.layers, layers);
        assert_eq!(app.session.active().unwrap().format, "eps");
        app.run("shape.ellipse", json!({"x": 50, "y": 10, "width": 20, "height": 20})).unwrap();
        app.run("file.save", json!({})).unwrap();
        assert_eq!(picks.borrow().len(), 1, "subsequent save keeps the EPS path without another picker");
    }

    #[test]
    fn saved_files_and_explicit_native_requests_keep_their_format() {
        for format in ["ai", "eps", "vectorcraft", "pdf", "svg", "svgz", "dxf"] {
            let (mut app, written, picks) = desktop("unused");
            let path = format!("original.{format}");
            let st = app.session.active_mut().unwrap();
            st.path = Some(path.clone());
            st.format = fileio::format(format).unwrap().id;
            app.run("file.save", json!({})).unwrap();
            assert!(picks.borrow().is_empty());
            assert_eq!(written.borrow()[0].0, path);
            assert_eq!(app.session.active().unwrap().format, format);
        }
        let (mut app, written, picks) = desktop("native");
        app.run("file.save", json!({"format": "vectorcraft"})).unwrap();
        assert_eq!(picks.borrow()[0].name, "Untitled-1.vectorcraft");
        assert_eq!(written.borrow()[0].0, "native.vectorcraft");
        assert!(vectorcraft_format::sniff(&written.borrow()[0].1));
    }

    #[test]
    fn cancelling_save_or_save_as_never_changes_the_document_or_writes() {
        for command in ["file.save", "file.saveAs"] {
            let written = Written::default();
            let w = written.clone();
            let mut app = with_art(Services {
                pick_save: Some(Box::new(|pick| {
                    assert!(pick.name.ends_with(".ai"));
                    assert_eq!(pick.filters.len(), 8);
                    None
                })),
                write: Some(Box::new(move |path, bytes| {
                    w.borrow_mut().push((path.to_string(), bytes.to_vec()));
                    Ok(())
                })),
                ..Default::default()
            });
            let doc = app.session.doc().unwrap().doc.clone();
            assert!(app.run(command, json!({})).is_err());
            assert!(written.borrow().is_empty());
            let st = app.session.doc().unwrap();
            assert_eq!(st.doc, doc);
            assert_eq!(st.format, "vectorcraft");
            assert!(st.path.is_none() && st.is_dirty());
        }
    }

    #[test]
    fn save_as_svg_opens_svg_options_then_writes_svg() {
        let (mut app, written, picks) = desktop("art.svg");
        let r = app.run("file.saveAs", json!({})).unwrap();
        assert_eq!((r["pending"].as_str(), r["path"].as_str()), (Some("svgOptions"), Some("art.svg")));
        // Unsaved UI documents default to AI; the picked format still wins.
        let pick = &picks.borrow()[0];
        let labels: Vec<&str> = pick.filters.iter().map(|f| f.0).collect();
        assert_eq!(labels, ["PDF-compatible .ai", "Kaleiform", "Kaleiform Template", "PDF", "SVG", "SVG Compressed", "EPS", "DXF"]);
        assert_eq!(pick.name, "Untitled-1.ai");
        let d = dialog(&app);
        assert_eq!((d.kind.as_str(), d.str("path").as_str(), d.str("mode").as_str()), ("svgOptions", "art.svg", "save"));
        assert!(written.borrow().is_empty(), "nothing is written before OK");
        frame(&mut app, false);
        assert!(app.ui.dialog.is_some(), "drawing keeps it open");
        app.ui.dialog.as_mut().unwrap().fields.insert("outlineText".into(), json!(true));
        frame(&mut app, true);
        assert!(app.ui.dialog.is_none());
        let (path, bytes) = written.borrow()[0].clone();
        assert_eq!(path, "art.svg");
        assert!(bytes.starts_with(b"<?xml"));
        let st = app.session.active().unwrap();
        assert_eq!((st.path.as_deref(), st.format, st.is_dirty()), (Some("art.svg"), "svg", false));
        assert_eq!((&st.save_options["outlineText"], &st.save_options["styling"]), (&json!(true), &json!("css")), "the SVG Options are remembered");
        assert!(app.ui.status.contains("SVG keeps the artwork"), "{}", app.ui.status);
        // Save writes it back as SVG, with the same options and no questions.
        assert!(!crate::menus::enabled(&app, "file.save"), "nothing to save");
        app.run("shape.ellipse", json!({"x": 50, "y": 10, "width": 20, "height": 20})).unwrap();
        assert!(crate::menus::enabled(&app, "file.save"));
        app.run("file.save", json!({})).unwrap();
        assert!(app.ui.dialog.is_none() && picks.borrow().len() == 1);
        assert!(written.borrow()[1].1.starts_with(b"<?xml"));
    }

    #[test]
    fn save_a_copy_and_template_suggest_their_names() {
        let (mut app, written, picks) = desktop("copy.vectorcraft");
        let folder = vectorcraft_testkit::temp_dir("save-template-dialog");
        app.session.prefs.templates_folder = folder.to_string_lossy().into_owned();
        app.run("file.saveCopy", json!({})).unwrap();
        assert_eq!(picks.borrow()[0].name, "Untitled-1 copy.ai");
        assert!(app.ui.dialog.is_none(), "the native format has no options");
        assert!(vectorcraft_format::sniff(&written.borrow()[0].1));
        assert!(app.session.active().unwrap().path.is_none() && app.session.active().unwrap().is_dirty());
        app.run("file.saveAsTemplate", json!({})).unwrap();
        let pick = &picks.borrow()[1];
        assert_eq!(pick.name, "Untitled-1 template.vctemplate");
        assert_eq!(pick.folder.as_deref().map(std::path::Path::new), Some(folder.as_path()));
        assert_eq!(pick.filters, [("Kaleiform Template", &["vctemplate"][..])]);
        assert_eq!(written.borrow()[1].0, "copy.vectorcraft", "the picked name is kept");
    }

    #[test]
    fn web_save_as_names_the_file_and_picks_the_format() {
        let (mut app, written) = web();
        app.run("file.saveAs", json!({})).unwrap();
        let d = dialog(&app);
        assert_eq!((heading(d).as_str(), d.str("path").as_str(), d.str("format").as_str()), ("Save As", "Untitled-1.ai", "ai"));
        frame(&mut app, false);
        let mut d = app.ui.dialog.take().unwrap();
        switch_format(&mut app, &mut d, "pdf");
        assert_eq!((d.str("path").as_str(), d.str("format").as_str()), ("Untitled-1.pdf", "pdf"));
        assert_eq!(shown(fileio::format("pdf").unwrap()).count(), 0, "PDF options have their own dialog");
        app.ui.dialog = Some(d);
        frame(&mut app, true);
        // OK goes on to the Save PDF dialog, whose OK saves.
        assert_eq!((dialog(&app).kind.as_str(), dialog(&app).str("path").as_str()), ("savePdf", "Untitled-1.pdf"));
        assert!(written.borrow().is_empty());
        super::super::confirm(&mut app).unwrap();
        let (name, bytes) = written.borrow()[0].clone();
        assert_eq!(name, "Untitled-1.pdf");
        assert!(bytes.starts_with(b"%PDF"));
        assert_eq!(app.session.active().unwrap().format, "pdf");
        // A blank name isn't saved.
        app.run("file.saveCopy", json!({})).unwrap();
        app.ui.dialog.as_mut().unwrap().fields.insert("path".into(), json!(" "));
        assert!(super::super::confirm(&mut app).is_err());
    }

    #[test]
    fn save_as_pdf_asks_with_the_save_pdf_dialog_and_remembers_its_options() {
        let (mut app, written, _) = desktop("art.pdf");
        assert_eq!(app.run("file.saveAs", json!({})).unwrap()["pending"], "savePdf");
        let d = app.ui.dialog.as_mut().unwrap();
        assert_eq!((d.str("path").as_str(), d.str("__save").as_str()), ("art.pdf", "file.saveAs"));
        d.fields.insert("compatibility".into(), json!("1.5"));
        d.fields.insert("createLayers".into(), json!(true));
        super::super::confirm(&mut app).unwrap();
        assert!(app.ui.dialog.is_none());
        assert!(written.borrow()[0].1.starts_with(b"%PDF-1.5"));
        let st = app.session.active().unwrap();
        assert_eq!((st.path.as_deref(), st.format, st.is_dirty()), (Some("art.pdf"), "pdf", false));
        assert_eq!((st.save_options["compatibility"].clone(), st.save_options["createLayers"].clone()), (json!("1.5"), json!(true)));
        // Save writes the PDF again with those options, without asking.
        app.run("shape.ellipse", json!({"x": 50, "y": 10, "width": 20, "height": 20})).unwrap();
        app.run("file.save", json!({})).unwrap();
        assert!(app.ui.dialog.is_none() && written.borrow()[1].1.starts_with(b"%PDF-1.5"));
        // Save a Copy as PDF starts from them.
        app.run("file.saveCopy", json!({"format": "pdf"})).unwrap();
        assert_eq!(dialog(&app).str("compatibility"), "1.5");
        assert_eq!(dialog(&app).str("__save"), "file.saveCopy");
    }

    #[test]
    fn epsdxf_save_as_uses_format_options_then_save_reuses_them() {
        for (path, kind) in [("art.eps", "epsOptions"), ("art.dxf", "dxfOptions")] {
            let (mut app, written, picks) = desktop(path);
            app.run("file.saveAs", json!({})).unwrap();
            assert_eq!(dialog(&app).kind, kind);
            assert_eq!(dialog(&app).str("__save"), "file.saveAs");
            assert!(written.borrow().is_empty());
            frame(&mut app, false);
            let f = if kind == "epsOptions" { "eps" } else { "dxf" };
            if f == "dxf" {
                assert_eq!(dialog(&app).str("preserve"), "editability");
                app.ui.dialog.as_mut().unwrap().fields.insert("unit".into(), json!("Inches"));
            } else {
                app.ui.dialog.as_mut().unwrap().fields.insert("previewFormat".into(), json!("none"));
            }
            super::super::confirm(&mut app).unwrap();
            assert!(app.ui.dialog.is_none());
            let (p, bytes) = written.borrow()[0].clone();
            assert_eq!(p, path);
            assert_eq!(fileio::load(path, &bytes).unwrap().format.id, f);
            let st = app.session.active().unwrap();
            assert_eq!((st.path.as_deref(), st.format, st.is_dirty()), (Some(path), f, false));
            let options = st.save_options.clone();
            app.run("shape.ellipse", json!({"x": 50, "y": 10, "width": 20, "height": 20})).unwrap();
            app.run("file.save", json!({})).unwrap();
            assert!(app.ui.dialog.is_none() && picks.borrow().len() == 1);
            assert_eq!(app.session.active().unwrap().save_options, options);
            assert_eq!(written.borrow().len(), 2);
            // Save-specific dispatch/overwrite approval must not leak into later exports.
            app.run(if f == "eps" { "ui.epsOptionsDialog" } else { "ui.dxfOptionsDialog" }, json!({})).unwrap();
            assert!(!dialog(&app).fields.contains_key("__save"));
            assert!(!dialog(&app).fields.contains_key("acknowledgeLoss"));
        }
    }

    #[test]
    fn epsdxf_given_paths_copy_cancel_and_invalid_options() {
        for (f, kind) in [("eps", "epsOptions"), ("dxf", "dxfOptions")] {
            let (mut app, written, _) = desktop("unused.vectorcraft");
            let path = format!("copy.{f}");
            app.run("file.saveCopy", json!({"path": path})).unwrap();
            assert_eq!(dialog(&app).kind, kind);
            super::super::cancel(&mut app);
            assert!(written.borrow().is_empty());
            app.run("file.saveCopy", json!({"path": path})).unwrap();
            let (key, bad, good) = if f == "eps" { ("level", json!(9), json!(3)) } else { ("scale", json!(0), json!(1)) };
            app.ui.dialog.as_mut().unwrap().fields.insert(key.into(), bad);
            assert!(super::super::confirm(&mut app).is_err());
            assert_eq!(dialog(&app).kind, kind);
            assert!(written.borrow().is_empty());
            app.ui.dialog.as_mut().unwrap().fields.insert(key.into(), good);
            super::super::confirm(&mut app).unwrap();
            assert_eq!(written.borrow().len(), 1);
            assert!(app.session.active().unwrap().path.is_none() && app.session.active().unwrap().is_dirty());
        }
    }

    #[test]
    fn epsdxf_web_save_as_proceeds_to_the_format_dialog() {
        for (f, kind) in [("eps", "epsOptions"), ("dxf", "dxfOptions")] {
            let (mut app, written) = web();
            app.run("file.saveAs", json!({})).unwrap();
            let mut d = app.ui.dialog.take().unwrap();
            switch_format(&mut app, &mut d, f);
            assert_eq!(shown(fileio::format(f).unwrap()).count(), 0);
            app.ui.dialog = Some(d);
            super::super::confirm(&mut app).unwrap();
            assert_eq!(dialog(&app).kind, kind);
            frame(&mut app, false);
            super::super::confirm(&mut app).unwrap();
            let (name, bytes) = written.borrow()[0].clone();
            assert!(name.ends_with(&format!(".{f}")));
            assert_eq!(fileio::load(&name, &bytes).unwrap().format.id, f);
        }
    }

    #[test]
    fn dxf_overwrite_confirmation_survives_the_options_dialog_once() {
        let (mut app, written, _) = desktop("unused.vectorcraft");
        let exported = app.session.execute("document.exportDxf", &json!({})).unwrap();
        let bytes = vectorcraft_format::base64_decode(exported["dataBase64"].as_str().unwrap()).unwrap();
        fileio::open_bytes(&mut app.session, "original.dxf", &bytes, Some("original.dxf".into())).unwrap();
        app.run("file.saveAs", json!({"path": "original.dxf"})).unwrap();
        assert_eq!(dialog(&app).kind, "confirm");
        super::super::confirm(&mut app).unwrap();
        assert_eq!(dialog(&app).kind, "dxfOptions");
        super::super::confirm(&mut app).unwrap();
        assert!(app.ui.dialog.is_none());
        assert_eq!(written.borrow().len(), 1);
    }
}
