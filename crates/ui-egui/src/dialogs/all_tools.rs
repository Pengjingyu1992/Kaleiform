//! The All Tools drawer: every tool as a button; picking one selects it and closes the drawer.

use super::DialogSpec;
use crate::VectorcraftApp;
use crate::state::Dialog;

pub(super) const SPEC: DialogSpec = DialogSpec { heading: |_| tl!("All Tools").into(), body, ok: None, ..DialogSpec::FORM };

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, _: &mut Dialog) -> bool {
    let mut picked = false;
    for g in vectorcraft_tools::TOOL_GROUPS {
        ui.horizontal_wrapped(|ui| {
            for tool in g.iter() {
                let shown = tl!(tool.label);
                let shown = if crate::i18n::current() == crate::i18n::Lang::EN { shown.trim_end_matches(" Tool") } else { shown };
                if ui.button(shown).clicked() {
                    app.select_tool(tool.id);
                    picked = true;
                }
            }
        });
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_button_selects_its_tool() {
        for width in [800.0, 1440.0] {
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx);
            ctx.enable_accesskit();
            let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
            app.run("app.language", serde_json::json!({"lang": "en"})).unwrap();
            app.run("file.new", serde_json::json!({"width": 200, "height": 100})).unwrap();
            for tool in vectorcraft_tools::catalog::all_tools() {
                let mut picked = false;
                let mut draw = |events| {
                    let mut out = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 3000.0))),
                            events,
                            ..Default::default()
                        },
                        |ui| picked |= body(&mut app, ui, &mut Dialog::new("allTools", serde_json::json!({}))),
                    );
                    out.textures_delta.clear();
                    out
                };
                draw(vec![]);
                let out = draw(vec![]);
                let label = tool.label.trim_end_matches(" Tool");
                let tree = out.platform_output.accesskit_update.unwrap();
                let rect = tree.nodes.iter().find(|(_, n)| n.label() == Some(label)).unwrap_or_else(|| panic!("missing {label}")).1.bounds().unwrap();
                let at = egui::pos2(((rect.x0 + rect.x1) / 2.0) as f32, ((rect.y0 + rect.y1) / 2.0) as f32);
                draw(vec![egui::Event::PointerMoved(at)]);
                for pressed in [true, false] {
                    draw(vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }]);
                }
                assert!(picked, "{label} was not clicked at width {width}");
                assert_eq!(app.session.tool_id(), tool.id);
                assert!(app.ui.dialog.is_none());
            }
        }
    }
}
