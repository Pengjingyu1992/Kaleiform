//! Recovery selection and confirmed deletion, using only the copies listed in this dialog.

use serde_json::{Value, json};

use super::{DialogSpec, modal};
use crate::{VectorcraftApp, state::Dialog, theme::Tokens, widgets};

pub(super) const SPEC: DialogSpec = DialogSpec::window(show, crate::recovery::confirm);
const DELETE: &str = "__deleteFiles";

fn files(copies: &[Value]) -> Vec<String> {
    copies.iter().filter_map(|c| c["file"].as_str().map(str::to_string)).collect()
}

fn selected(d: &Dialog, all: &[String]) -> Vec<String> {
    d.fields
        .get("selected")
        .and_then(Value::as_array)
        .map_or_else(|| all.to_vec(), |values| all.iter().filter(|f| values.iter().any(|v| v.as_str() == Some(f))).cloned().collect())
}

fn show(app: &mut VectorcraftApp, ctx: &egui::Context) {
    let Some(mut d) = app.ui.dialog.clone() else { return };
    let copies = d.fields.get("copies").and_then(Value::as_array).cloned().unwrap_or_default();
    let all = files(&copies);
    let mut picked = selected(&d, &all);
    let pending = d.fields.get(DELETE).and_then(Value::as_array).cloned();
    let t = Tokens::get(ctx);
    let width = (ctx.content_rect().width() - 72.0).clamp(240.0, 600.0);
    let height = (ctx.content_rect().height() - 240.0).clamp(60.0, 300.0);
    let (mut restore, mut delete, mut close) = (false, false, false);
    let mut requested = None;
    modal::show(ctx, tl!("Recover Documents"), egui::Id::new(("dialog", crate::recovery::KIND)), -20.0, 18, |ui| {
        ui.set_width(width);
        modal::heading(ui, tl!("Recover Documents"));
        ui.add_space(8.0);
        ui.label(tl!("Restored documents open unsaved: save them to keep them."));
        ui.add_space(8.0);
        if pending.is_some() {
            ui.colored_label(t.warning, tl!("Delete these recovery copies?"));
            ui.label(tl!("This cannot be undone."));
        } else {
            ui.horizontal(|ui| {
                let mut checked = !all.is_empty() && picked.len() == all.len();
                if ui.checkbox(&mut checked, tl!("Select All")).changed() {
                    picked = if checked { all.clone() } else { vec![] };
                }
                ui.label(format!("{} / {}", picked.len(), all.len()));
            });
        }
        egui::ScrollArea::vertical().id_salt("recovery-copies").max_height(height).auto_shrink([false, true]).show(ui, |ui| {
            for copy in &copies {
                let file = copy["file"].as_str().unwrap_or_default();
                if pending.as_ref().is_some_and(|p| !p.iter().any(|v| v.as_str() == Some(file))) {
                    continue;
                }
                ui.push_id(file, |ui| {
                    if pending.is_some() {
                        ui.label(copy["title"].as_str().unwrap_or_default());
                    } else {
                        let mut checked = picked.iter().any(|f| f == file);
                        if ui.checkbox(&mut checked, copy["title"].as_str().unwrap_or_default()).changed() {
                            picked.retain(|f| f != file);
                            if checked {
                                picked.push(file.to_string());
                            }
                        }
                    }
                    if let Some(saved) = copy["saved"].as_i64().and_then(|v| u64::try_from(v).ok()) {
                        ui.label(egui::RichText::new(crate::panels::links::date_label(saved.saturating_mul(1000))).size(11.0).color(t.text_dim));
                    }
                    ui.separator();
                });
            }
        });
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            if pending.is_some() {
                if widgets::secondary_button(ui, tl!("Cancel")).clicked() {
                    d.fields.remove(DELETE);
                }
                delete = widgets::primary_button(ui, tl!("Delete")).clicked();
            } else {
                restore = ui
                    .add_enabled_ui(!picked.is_empty(), |ui| widgets::command_button(ui, "rotate-ccw", tl!("Restore"), egui::vec2(94.0, 28.0), true))
                    .inner
                    .clicked();
                if ui
                    .add_enabled_ui(!picked.is_empty(), |ui| {
                        widgets::command_button(ui, "trash-2", tl!("Delete Selected Item"), egui::vec2(142.0, 28.0), false)
                    })
                    .inner
                    .clicked()
                {
                    requested = Some(picked.clone());
                }
                if ui
                    .add_enabled_ui(!all.is_empty(), |ui| widgets::command_button(ui, "trash-2", tl!("Delete All"), egui::vec2(110.0, 28.0), false))
                    .inner
                    .clicked()
                {
                    requested = Some(all.clone());
                }
                close = widgets::secondary_button(ui, tl!("Close")).clicked();
            }
        });
    });
    if let Some(files) = requested {
        d.fields.insert(DELETE.into(), json!(files));
    }
    d.fields.insert("selected".into(), json!(picked));
    if delete {
        d.fields.insert("selected".into(), json!(pending.unwrap_or_default()));
        d.fields.insert("discard".into(), json!(true));
    }
    app.ui.dialog = Some(d);
    if close {
        super::cancel(app);
    } else if (restore || delete)
        && let Err(error) = super::confirm(app)
    {
        app.status(error);
    }
}
