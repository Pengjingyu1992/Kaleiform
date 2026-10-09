//! Libraries panel (Window › Libraries, #745): the current library's colours, character and
//! paragraph styles and graphics, from the engine's `library.*` commands. Art dragged off the canvas
//! onto the panel, and swatches dragged in, are added to it; the + button adds the selection's
//! graphic, fill or stroke colour or text styles. A colour click paints the selection through the
//! active Fill/Stroke proxy, a style click applies it to the selected type, and a graphic is
//! dragged onto the canvas (or double-clicked) to place a copy. Libraries are kept on this machine.

use std::collections::HashMap;

use egui::{Rect, Sense, Stroke, StrokeKind, Ui, vec2};
use serde_json::{Value, json};
use vectorcraft_color::Paint;
use vectorcraft_engine::cmd::library::Library;

use crate::theme::Tokens;
use crate::widgets::{self, PanelDrag, menu_item};
use crate::{VectorcraftApp, icons};

/// The size of a graphic's tile.
const TILE: f32 = 56.0;
/// The size of a colour chip.
const CHIP: f32 = 22.0;

/// The current library: its id and a copy of it (the panel acts on it after reading).
fn current(app: &VectorcraftApp) -> Option<(String, Library)> {
    let libs = &app.session.libraries;
    let id = libs.current()?;
    Some((id.to_string(), libs.get(id)?.clone()))
}

/// Run `cmd`, reporting a failure in the status bar.
fn run(app: &mut VectorcraftApp, cmd: &str, p: Value) {
    if let Err(e) = app.run(cmd, p) {
        app.status(e);
    }
}

/// The name dialog for a new library, or for renaming the current one.
fn ask_name(app: &mut VectorcraftApp, rename: Option<&str>) {
    let (cmd, label, name) = match rename {
        Some(n) => ("library.rename", "Rename Library", n.to_string()),
        None => ("library.create", "Create New Library", "My Library".to_string()),
    };
    app.ui.dialog = Some(crate::state::Dialog::new("command", json!({ "__command": cmd, "__label": label, "name": name })));
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Some((lib_id, lib)) = current(app) else {
        ui.add_space(16.0);
        ui.vertical_centered(|ui| {
            icons::icon(ui, "library", 40.0, t.text_dim);
            ui.add_space(8.0);
            ui.label(egui::RichText::new(tl!("Local Libraries")).size(14.0).color(t.text));
            widgets::dim_label(ui, tl!("Keep graphics, colors and text styles here to use them in any document. Libraries are stored on this machine — no account required."));
            ui.add_space(8.0);
            if ui.button(tl!("Create New Library")).clicked() {
                ask_name(app, None);
            }
        });
        return;
    };
    // The library picker.
    let libs: Vec<(String, String)> = app.session.libraries.all().iter().map(|(i, l)| (i.clone(), l.name.clone())).collect();
    let names: Vec<&str> = libs.iter().map(|(_, n)| n.as_str()).collect();
    if let Some(i) = widgets::dropdown_names(ui, "lib-picker", &lib.name, &names, ui.available_width())
        && let Some((id, _)) = libs.get(i)
    {
        run(app, "library.setCurrent", json!({ "library": id }));
    }
    ui.add_space(4.0);
    let empty = lib.colors.is_empty() && lib.char_styles.is_empty() && lib.para_styles.is_empty() && lib.graphics.is_empty();
    let mut act: Option<(&'static str, Value)> = None;
    let fill_active = app.session.fill_active;
    let list_rect = widgets::list_box(ui, |ui| {
        ui.set_min_height(140.0);
        ui.set_width(ui.available_width());
        if empty {
            super::empty_state(
                ui,
                "library",
                tl!("This library is empty"),
                tl!("Drag art or swatches here, or use + to add the selection's colors and text styles."),
            );
            return ui.min_rect();
        }
        egui::Frame::NONE.inner_margin(egui::Margin::same(6)).show(ui, |ui| {
            if !lib.colors.is_empty() {
                heading(ui, tl!("Colors"));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    for c in &lib.colors {
                        let (r, resp) = ui.allocate_exact_size(vec2(CHIP, CHIP), Sense::click_and_drag());
                        let paint = Paint::solid(c.color);
                        widgets::swatch_tile(ui, r, &paint, false, false);
                        ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, t.border), StrokeKind::Inside);
                        widgets::drag_source(ui, &resp, || PanelDrag::Paint {
                            paint: paint.clone(),
                            params: json!({ "color": c.color }),
                            rows: None,
                        });
                        let resp = resp.on_hover_text(&c.name);
                        if resp.clicked() {
                            act = Some((
                                "library.use",
                                json!({ "kind": "fillColor", "item": c.name, "to": if fill_active { "fill" } else { "stroke" } }),
                            ));
                        }
                        resp.context_menu(|ui| {
                            if menu_item(ui, "Delete", true, false) {
                                act = Some(("library.removeItem", json!({ "kind": "fillColor", "item": c.name })));
                                ui.close();
                            }
                        });
                    }
                });
                ui.add_space(6.0);
            }
            for (para, list, title) in [(false, &lib.char_styles, tl!("Character Styles")), (true, &lib.para_styles, tl!("Paragraph Styles"))] {
                if list.is_empty() {
                    continue;
                }
                heading(ui, title);
                let kind = if para { "paraStyle" } else { "charStyle" };
                for st in list {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, 0.0, t.hover);
                    }
                    icons::paint(
                        ui,
                        if para { "pilcrow" } else { "type" },
                        Rect::from_center_size(r.left_center() + vec2(10.0, 0.0), vec2(14.0, 14.0)),
                        t.icon,
                    );
                    ui.painter().text(
                        r.left_center() + vec2(24.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        &st.name,
                        egui::FontId::proportional(12.5),
                        t.text,
                    );
                    let resp = resp.on_hover_text(tl!("Click to apply to the selected type"));
                    if resp.clicked() {
                        act = Some(("library.use", json!({ "kind": kind, "item": st.name })));
                    }
                    resp.context_menu(|ui| {
                        if menu_item(ui, "Delete", true, false) {
                            act = Some(("library.removeItem", json!({ "kind": kind, "item": st.name })));
                            ui.close();
                        }
                    });
                }
                ui.add_space(6.0);
            }
            if !lib.graphics.is_empty() {
                heading(ui, tl!("Graphics"));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    for g in &lib.graphics {
                        let (r, resp) = ui.allocate_exact_size(vec2(TILE, TILE), Sense::click_and_drag());
                        tile(ui, r, &lib_id, g);
                        ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, if resp.hovered() { t.accent } else { t.border }), StrokeKind::Inside);
                        // Dragged onto the canvas, a copy is placed where it is dropped.
                        widgets::drag_source(ui, &resp, || PanelDrag::LibraryGraphic { library: lib_id.clone(), item: g.id.clone() });
                        let resp = resp.on_hover_text(&g.name);
                        if resp.double_clicked() {
                            act = Some(("library.use", json!({ "kind": "graphic", "item": g.id })));
                        }
                        resp.context_menu(|ui| {
                            if menu_item(ui, "Place", true, false) {
                                act = Some(("library.use", json!({ "kind": "graphic", "item": g.id })));
                                ui.close();
                            }
                            if menu_item(ui, "Delete", true, false) {
                                act = Some(("library.removeItem", json!({ "kind": "graphic", "item": g.id })));
                                ui.close();
                            }
                        });
                    }
                });
            }
        });
        ui.min_rect()
    });
    // Art dragged off the canvas, or a swatch, dropped on the list is added to the library.
    let zone = ui.interact(list_rect, ui.id().with("libraries-drop"), Sense::hover());
    if let Some(ids) = widgets::art_drop(ui, &zone) {
        act = Some(("library.add", json!({ "kind": "graphic", "ids": ids })));
    }
    if let Some(drag) = zone.dnd_hover_payload::<PanelDrag>()
        && let PanelDrag::Paint { paint: Paint::Solid { color, .. }, .. } = &*drag
    {
        ui.painter().rect_stroke(zone.rect, 0.0, Stroke::new(1.5, t.accent), StrokeKind::Inside);
        let color = *color;
        if zone.dnd_release_payload::<PanelDrag>().is_some() {
            act = Some(("library.add", json!({ "kind": "fillColor", "color": color })));
        }
    }
    if let Some((cmd, mut p)) = act {
        p["library"] = json!(lib_id);
        if cmd == "library.use" && p["kind"] == "graphic" {
            // Placed at the centre of the view.
            if let Some(v) = app.view() {
                p["center"] = json!([v.center.x, v.center.y]);
            }
        }
        run(app, cmd, p);
    }
    let has_doc = app.session.active().is_some();
    widgets::bottom_bar(ui, |ui| {
        let add = widgets::icon_button_enabled(ui, "plus", tl!("Add Content"), false, has_doc, 24.0);
        egui::Popup::menu(&add).show(|ui| {
            for (label, kind) in [
                ("Graphic", "graphic"),
                ("Fill Color", "fillColor"),
                ("Stroke Color", "strokeColor"),
                ("Character Style", "charStyle"),
                ("Paragraph Style", "paraStyle"),
            ] {
                if menu_item(ui, label, true, false) {
                    run(app, "library.add", json!({ "library": lib_id, "kind": kind }));
                    ui.close();
                }
            }
        });
    });
}

/// A section heading in the list.
fn heading(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(text).size(11.5).color(t.text_dim));
    ui.add_space(2.0);
}

/// Graphic `g`'s thumbnail on white in `r`.
fn tile(ui: &Ui, r: Rect, library: &str, g: &vectorcraft_engine::cmd::library::LibraryGraphic) {
    ui.painter().rect_filled(r, 0.0, egui::Color32::WHITE);
    if let Some(tex) = thumbnail(ui.ctx(), library, g) {
        let size = tex.size_vec2();
        let k = ((r.width() - 6.0) / size.x).min((r.height() - 6.0) / size.y).min(1.0);
        let fit = Rect::from_center_size(r.center(), size * k);
        ui.painter().image(tex.id(), fit, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
    }
}

/// The texture of graphic `g`'s PNG thumbnail, decoded once.
fn thumbnail(ctx: &egui::Context, library: &str, g: &vectorcraft_engine::cmd::library::LibraryGraphic) -> Option<egui::TextureHandle> {
    thread_local! {
        static CACHE: crate::graphics::TexCache<HashMap<(String, String, usize), Option<egui::TextureHandle>>> = crate::graphics::TexCache::default();
    }
    let key = (library.to_string(), g.id.clone(), g.thumbnail.len());
    if let Some(t) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return t;
    }
    let tex = vectorcraft_format::base64_decode(&g.thumbnail)
        .and_then(|png| image::load_from_memory_with_format(&png, image::ImageFormat::Png).ok())
        .map(|img| {
            let rgba = img.to_rgba8();
            let image = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
            ctx.load_texture(format!("library-{library}-{}", g.id), image, egui::TextureOptions::LINEAR)
        });
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 512 {
            c.clear();
        }
        c.insert(key, tex.clone());
    });
    tex
}

/// Graphic `item` of `library`'s thumbnail in `r` (the chip a dragged graphic shows at the pointer).
pub(crate) fn chip(app: &VectorcraftApp, ui: &Ui, r: Rect, library: &str, item: &str) {
    if let Some(g) = app.session.libraries.get(library).and_then(|l| l.graphics.iter().find(|g| g.id == item)) {
        tile(ui, r, library, g);
    }
}

/// The panel's ≡ menu: Create New Library, Rename Library…, Delete Library.
pub fn menu(app: &mut VectorcraftApp, ui: &mut Ui) {
    let cur = current(app);
    if menu_item(ui, "Create New Library…", true, false) {
        ask_name(app, None);
        ui.close();
    }
    if menu_item(ui, "Rename Library…", cur.is_some(), false)
        && let Some((_, lib)) = &cur
    {
        ask_name(app, Some(&lib.name));
        ui.close();
    }
    if menu_item(ui, "Delete Library", cur.is_some(), false)
        && let Some((id, lib)) = &cur
    {
        let message = crate::i18n::fmt(tl!("Delete the library “{name}”?"), &[("name", &lib.name)]);
        crate::dialogs::confirm::ask(
            app,
            &message,
            tl!("Its graphics, colors and text styles are deleted from this machine."),
            "library.delete",
            json!({ "library": id }),
        );
        ui.close();
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vectorcraft_engine::Session;

    use super::*;

    fn frame(app: &mut VectorcraftApp) {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(app, ui);
            menu(app, ui);
        });
        out.textures_delta.clear();
    }

    /// The panel draws with no library, an empty one and one holding every kind of item (#745).
    #[test]
    fn the_panel_draws_its_libraries() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 200, "height": 200})).unwrap();
        frame(&mut app);
        app.run("library.create", json!({"name": "Brand"})).unwrap();
        frame(&mut app);
        let id = app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 30, "height": 20})).unwrap()["id"].clone();
        app.run("select.set", json!({"ids": [id]})).unwrap();
        app.run("library.add", json!({"kind": "graphic"})).unwrap();
        app.run("library.add", json!({"kind": "fillColor"})).unwrap();
        let t = app.run("text.create", json!({"x": 10, "y": 80, "text": "Type"})).unwrap()["id"].clone();
        app.run("select.set", json!({"ids": [t]})).unwrap();
        app.run("library.add", json!({"kind": "charStyle"})).unwrap();
        app.run("library.add", json!({"kind": "paraStyle"})).unwrap();
        frame(&mut app);
        let lib = app.session.libraries.get("Brand").unwrap();
        assert_eq!((lib.graphics.len(), lib.colors.len(), lib.char_styles.len(), lib.para_styles.len()), (1, 1, 1, 1));
        // The thumbnail decodes.
        let ctx = egui::Context::default();
        assert!(thumbnail(&ctx, "Brand", &lib.graphics[0]).is_some());
    }
}
