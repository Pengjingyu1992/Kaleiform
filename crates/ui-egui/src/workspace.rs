//! Canvas-first macOS workspace over the existing command and panel implementations.

use egui::{Align2, Context, CornerRadius, FontId, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;

use crate::state::{DockTab, all_panels};
use crate::theme::{self, Tokens};
use crate::{VectorcraftApp, icons, menus, panels, widgets};

pub const BAR_HEIGHT: f32 = 52.0;
pub const STATUS_HEIGHT: f32 = 28.0;
pub const OVERLAY_BREAKPOINT: f32 = 960.0;

pub fn enabled(app: &VectorcraftApp) -> bool {
    app.integrated_titlebar && app.services.native_menu.is_some()
}

fn command(app: &mut VectorcraftApp, ui: &mut Ui, icon: &str, label: &str, id: &str) {
    if widgets::icon_button_enabled(ui, icon, tl!(label), false, menus::enabled(app, id), 28.0).clicked() {
        menus::invoke(app, id, json!({}));
    }
}

pub fn app_bar(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("app_bar")
        .exact_size(BAR_HEIGHT)
        .frame(egui::Frame::NONE.fill(t.app_bar).inner_margin(egui::Margin { left: 0, right: 12, top: 0, bottom: 0 }))
        .show(ui, |ui| {
            crate::titlebar::drag_area(ui, ui.max_rect());
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add_space(78.0 / ui.ctx().zoom_factor());
                let visible =
                    app.ui.dock && !app.ui.dock_collapsed && (ui.ctx().content_rect().width() >= OVERLAY_BREAKPOINT || app.ui.inspector_overlay);
                if widgets::icon_toggle(ui, "dc-options", tl!("Panels"), visible, 28.0).clicked() {
                    let collapsed = visible;
                    app.run("window.collapseDock", json!({"collapsed": collapsed})).ok();
                    app.ui.inspector_overlay = !collapsed;
                }
                let (rect, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                let home = ui.interact(rect, Id::new("workspace-home"), Sense::click());
                if widgets::paint_icon_button(ui, home, "house", tl!("Home"), menus::home_showing(app)).clicked() {
                    app.run("app.home", json!({})).ok();
                }
                let full = ui.max_rect();
                let wide = full.width() >= 1050.0;
                let tiny = full.width() < 620.0;
                let search_width = if wide { 230.0 } else { 28.0 };
                let workspace_name =
                    if crate::workspaces::is_builtin(&app.ui.workspace) { tl!(&app.ui.workspace) } else { &app.ui.workspace }.to_string();
                let text_width = |text: &str| ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(t.font_size), t.text).size().x;
                let workspace_width = if wide { (text_width(&workspace_name) + 36.0).clamp(112.0, 180.0) } else { 28.0 };
                let export_width = if tiny { 28.0 } else { (text_width(tl!("Export")) + 36.0).clamp(72.0, 120.0) };
                let controls = search_width + workspace_width + export_width + if tiny { 12.0 } else { 3.0 * 28.0 + 5.0 * 6.0 };
                let split = (full.right() - controls).max(ui.cursor().left() + 100.0);
                let tabs = Rect::from_min_max(pos2(ui.cursor().left() + 4.0, full.top()), pos2(split - 12.0, full.bottom()));
                crate::chrome::top_tabs::show(app, ui, tabs);
                let right = Rect::from_min_max(pos2(split, full.top()), full.max);
                let mut row = ui.new_child(egui::UiBuilder::new().max_rect(right).layout(egui::Layout::right_to_left(egui::Align::Center)));
                row.spacing_mut().item_spacing.x = 6.0;
                if !tiny {
                    command(app, &mut row, "info", "About Kaleiform", "help.about");
                }
                let export = row
                    .add_enabled_ui(menus::enabled(app, "file.exportAs"), |ui| {
                        if tiny {
                            widgets::icon_button(ui, "share-2", tl!("Export"), false, 28.0)
                        } else {
                            widgets::command_button(ui, "share-2", tl!("Export"), vec2(export_width, 28.0), true)
                        }
                    })
                    .inner;
                if export.clicked() {
                    menus::invoke(app, "file.exportAs", json!({}));
                }
                let ws = if wide {
                    widgets::command_button(&mut row, "chevron-down", &workspace_name, vec2(workspace_width, 28.0), false)
                } else {
                    widgets::icon_button(&mut row, "layout-grid", tl!("Workspace"), false, 28.0)
                };
                egui::Popup::menu(&ws).show(|ui| crate::workspaces::popup(app, ui));
                if !tiny {
                    command(app, &mut row, "redo-2", "Redo", "edit.redo");
                    command(app, &mut row, "undo-2", "Undo", "edit.undo");
                }
                let search = if wide {
                    let (r, response) = row.allocate_exact_size(vec2(search_width, 28.0), Sense::click());
                    row.painter().rect_filled(r, CornerRadius::same(6), t.input);
                    icons::paint(&row, "search", Rect::from_center_size(r.left_center() + vec2(14.0, 0.0), vec2(16.0, 16.0)), t.icon);
                    let text = r.shrink2(vec2(28.0, 0.0));
                    single_line(&row, text, tl!("Search commands and tools"), t.text_dim, false);
                    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Search commands and tools")));
                    response.on_hover_text(tl!("Search commands and tools"))
                } else {
                    widgets::icon_button(&mut row, "search", tl!("Search commands and tools"), false, 28.0)
                };
                if search.clicked() {
                    app.run("help.commandPalette", json!({})).ok();
                }
            });
        });
}

pub fn inspector(app: &mut VectorcraftApp, ui: &mut Ui) {
    if app.ui.dock_collapsed {
        return;
    }
    let available = ui.available_rect_before_wrap();
    if ui.ctx().content_rect().width() < OVERLAY_BREAKPOINT {
        if !app.ui.inspector_overlay {
            return;
        }
        let ctx = ui.ctx().clone();
        let r = Rect::from_min_max(pos2(available.right() - 300.0, available.top()), available.max);
        egui::Area::new(Id::new("workspace-inspector-overlay")).order(egui::Order::Foreground).constrain(false).fixed_pos(r.min).show(&ctx, |ui| {
            ui.set_width(r.width());
            ui.set_min_height(r.height());
            egui::Frame::NONE.fill(Tokens::get(&ctx).panel).stroke(Stroke::new(1.0, Tokens::get(&ctx).divider)).show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("inspector-overlay-bounds")
                    .max_height(r.height() - 2.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| inspector_body(app, ui, r.height() - 2.0));
            });
        });
        remember_rect(&ctx, r);
    } else {
        let t = Tokens::get(ui.ctx());
        let panel = egui::Panel::right("workspace-inspector")
            .resizable(true)
            .default_size(300.0)
            .size_range(280.0..=440.0)
            .frame(egui::Frame::NONE.fill(t.surface))
            .show(ui, |ui| inspector_body(app, ui, available.height()));
        remember_rect(ui.ctx(), panel.response.rect);
    }
}

fn remember_rect(ctx: &Context, r: Rect) {
    ctx.data_mut(|d| {
        d.insert_temp(crate::floating::dock_rect_id(), r);
        d.insert_temp(crate::floating::icons_rect_id(), r);
    });
}

fn active_panel(app: &VectorcraftApp) -> &str {
    app.ui
        .open_panel
        .as_deref()
        .filter(|id| all_panels().any(|(p, _)| p == *id) && crate::floating::group_of(&app.ui, id).is_none())
        .unwrap_or_else(|| crate::floating::shown_tab(&app.ui).map_or("properties", |t| t.info().0))
}

fn show_panel(app: &mut VectorcraftApp, id: &str) {
    if let Some(tab) = DockTab::from_id(id) {
        app.ui.dock_tab = tab;
        app.ui.open_panel = None;
    } else {
        app.ui.open_panel = Some(id.to_string());
    }
    app.ui.dock_collapsed = false;
    app.ui.inspector_overlay = true;
}

fn inspector_body(app: &mut VectorcraftApp, ui: &mut Ui, height: f32) {
    let t = Tokens::get(ui.ctx());
    let bottom = ui.cursor().top() + height;
    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
    let mut chosen = None;
    egui::Frame::NONE.inner_margin(egui::Margin::same(10)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let tabs_width = (ui.available_width() - 34.0).max(1.0);
            let (r, _) = ui.allocate_exact_size(vec2(tabs_width, 28.0), Sense::hover());
            ui.painter().rect_filled(r, 6, t.input);
            for (i, (id, label)) in
                [("properties", "Properties"), ("layers", "Layers"), ("libraries", "Libraries"), ("history", "History")].into_iter().enumerate()
            {
                let tab = Rect::from_min_size(r.min + vec2(i as f32 * r.width() / 4.0, 0.0), vec2(r.width() / 4.0, r.height()));
                let selected = active_panel(app) == id;
                let resp = ui.interact(tab, Id::new(("workspace-tab", id)), Sense::click_and_drag());
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, tl!(label)));
                if selected || resp.hovered() {
                    ui.painter().rect_filled(tab.shrink(2.0), 5, if selected { t.hover } else { t.panel });
                }
                if resp.has_focus() {
                    ui.painter().rect_stroke(tab.shrink(2.0), 5, Stroke::new(2.0, t.accent), egui::StrokeKind::Inside);
                }
                single_line(ui, tab.shrink2(vec2(4.0, 0.0)), tl!(label), if selected { t.text_strong } else { t.text_dim }, true);
                if resp.clicked() {
                    chosen = Some(id.to_string());
                }
                if resp.dragged() && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| !r.expand(8.0).contains(p)) {
                    crate::floating::tear(app, ui.ctx(), &[id], id, crate::floating::strip_grab(ui.ctx(), tab.left()));
                }
                resp.context_menu(|ui| {
                    if ui.button(tl!("Float Panel")).clicked() {
                        app.run("window.panel.float", json!({"panel": id})).ok();
                        ui.close();
                    }
                    panels::panel_menu_items(app, ui, id);
                });
                resp.on_hover_text(tl!(label));
            }
            let more = widgets::icon_button(ui, "ellipsis", tl!("All Panels"), false, 28.0);
            egui::Popup::menu(&more).show(|ui| {
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for (id, label) in all_panels() {
                        ui.horizontal(|ui| {
                            let mut pinned = app.ui.pinned_panels.iter().any(|p| p == id);
                            let pin = ui.checkbox(&mut pinned, "").on_hover_text(format!("{}: {}", tl!("Pin Panel"), tl!(label)));
                            pin.widget_info(|| {
                                egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, pinned, format!("{}: {}", tl!("Pin Panel"), tl!(label)))
                            });
                            if pin.changed() {
                                app.run("window.panel.pin", json!({"panel": id, "pinned": pinned})).ok();
                            }
                            if ui.selectable_label(active_panel(app) == id, tl!(label)).clicked() {
                                chosen = Some(id.to_string());
                                ui.close();
                            }
                        });
                    }
                });
            });
        });
        ui.horizontal(|ui| {
            let pins = app.ui.pinned_panels.clone();
            let visible = ((ui.available_width() - 34.0) / 34.0).floor().clamp(0.0, 7.0) as usize;
            for id in pins.iter().take(visible) {
                let entry = crate::state::ICON_PANELS.iter().find(|p| p.0 == id).copied().or_else(|| DockTab::from_id(id).map(DockTab::info));
                let Some((_, label, icon)) = entry else { continue };
                if widgets::icon_button(ui, icon, tl!(label), active_panel(app) == id, 28.0).clicked() {
                    chosen = Some(id.clone());
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, "chevrons-right", tl!("Hide Panels"), false, 28.0).clicked() {
                    app.run("window.collapseDock", json!({"collapsed": true})).ok();
                }
            });
        });
    });
    if let Some(id) = chosen {
        if crate::floating::group_of(&app.ui, &id).is_some() {
            app.run("window.panel", json!({"panel": id})).ok();
        } else {
            show_panel(app, &id);
        }
    }
    let active = active_panel(app).to_string();
    if crate::floating::group_of(&app.ui, &active).is_some() {
        return;
    }
    egui::Frame::NONE.inner_margin(egui::Margin { left: 14, right: 14, top: 4, bottom: 12 }).show(ui, |ui| {
        let width = ui.available_width();
        if !matches!(active.as_str(), "properties" | "layers" | "libraries" | "history") {
            ui.horizontal(|ui| {
                let label = all_panels().find(|(id, _)| *id == active).map_or("", |(_, label)| label);
                ui.label(egui::RichText::new(tl!(label)).font(theme::semibold(t.font_size)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, "x", tl!("Close"), false, 24.0).clicked() {
                        app.ui.open_panel = None;
                    }
                    let menu = widgets::icon_button(ui, "ellipsis", tl!("More Options"), false, 24.0);
                    egui::Popup::menu(&menu).show(|ui| {
                        if ui.button(tl!("Float Panel")).clicked() {
                            app.run("window.panel.float", json!({"panel": active})).ok();
                            ui.close();
                        }
                        panels::panel_menu_items(app, ui, &active);
                    });
                });
            });
        }
        let tall = (bottom - ui.cursor().top() - 12.0).max(24.0);
        match active.as_str() {
            "layers" => {
                ui.set_max_height(tall);
                panels::layers::show(app, ui);
            }
            "imageTrace" => panels::image_trace::show_bounded(app, ui, tall),
            _ => {
                egui::ScrollArea::vertical().id_salt(("workspace-panel", &active)).max_height(tall).auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_max_width(width);
                    match active.as_str() {
                        "properties" => panels::properties::show(app, ui),
                        "libraries" => panels::libraries(app, ui),
                        _ => panels::show_icon_panel(app, ui, &active),
                    }
                });
            }
        }
    });
}

fn single_line(ui: &Ui, rect: Rect, label: &str, color: egui::Color32, centered: bool) {
    let mut job =
        egui::text::LayoutJob::simple(label.to_string(), FontId::proportional(Tokens::get(ui.ctx()).font_size), color, rect.width().max(1.0));
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('\u{2026}');
    let galley = ui.painter().layout_job(job);
    let x = if centered { rect.center().x - galley.size().x / 2.0 } else { rect.left() };
    ui.painter().with_clip_rect(rect).galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, color);
}

pub fn overlays(app: &mut VectorcraftApp, ctx: &Context) {
    if !enabled(app) || app.ui.screen_mode >= 3 {
        return;
    }
    if app.ui.performance_hud {
        let t = Tokens::get(ctx);
        egui::Area::new(Id::new("performance-hud")).order(egui::Order::Foreground).anchor(Align2::LEFT_TOP, vec2(76.0, BAR_HEIGHT + 32.0)).show(
            ctx,
            |ui| {
                egui::Frame::NONE.fill(t.panel).corner_radius(6).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                    ui.label(egui::RichText::new(tl!("Performance Diagnostics")).font(theme::semibold(t.font_size)));
                    ui.monospace(format!("Raster {:.2} ms   UI {:.2} ms", app.perf.render_ms, app.perf.frame_ms));
                    let move_preview = crate::move_preview::inspect(app);
                    ui.label(format!("{}: {}", tl!("Move Preview"), move_preview["ready"].as_bool().unwrap_or(false)));
                    ui.label(egui::RichText::new(tl!("Processing time, not display latency")).size(11.0).color(t.text_dim));
                });
            },
        );
    }
    // Tool changes get a short hint without reserving another row of canvas space.
    let id = Id::new("workspace-tool-hint");
    let tool = app.session.tool_id().to_string();
    let now = ctx.input(|i| i.time);
    let previous = ctx.data(|d| d.get_temp::<(String, f64)>(id));
    let until = match previous {
        Some((old, until)) if old == tool => until,
        _ => {
            ctx.data_mut(|d| d.insert_temp(id, (tool.clone(), now + 4.0)));
            now + 4.0
        }
    };
    if now < until && !app.session.tool_busy() && app.ui.dialog.is_none() {
        let t = Tokens::get(ctx);
        let text = crate::chrome::hint_segments(&tool).into_iter().map(|(s, _)| s.into_owned()).collect::<String>();
        if !text.is_empty() {
            let bounds = app.canvas_rect.unwrap_or(ctx.content_rect());
            egui::Area::new(Id::new("workspace-hint"))
                .interactable(false)
                .order(egui::Order::Foreground)
                .pivot(Align2::LEFT_BOTTOM)
                .fixed_pos(bounds.left_bottom() + vec2(14.0, -12.0))
                .show(ctx, |ui| {
                    egui::Frame::NONE.fill(t.panel).corner_radius(6).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                        ui.set_max_width((bounds.width() - 28.0).max(1.0));
                        ui.add(egui::Label::new(egui::RichText::new(text).size(11.0).color(t.text_dim)).truncate());
                    });
                });
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((until - now).max(0.01)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MenuBackend;
    impl crate::native_menu::Backend for MenuBackend {
        fn sync(&mut self, _: &crate::native_menu::MenuBar) {}
        fn drain(&mut self) -> Vec<crate::native_menu::Event> {
            vec![]
        }
    }

    fn app() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.integrated_titlebar = true;
        app.services.native_menu = Some(crate::native_menu::NativeMenu::new(Box::new(MenuBackend)));
        app.run("file.new", json!({"width": 300, "height": 200})).unwrap();
        app
    }

    fn frame(app: &mut VectorcraftApp, ctx: &Context, width: f32, height: f32) {
        for _ in 0..4 {
            let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, height))), ..Default::default() };
            let mut out = ctx.run_ui(raw, |ui| {
                app.logic(ui.ctx());
                app.ui(ui);
            });
            out.textures_delta.clear();
        }
    }

    #[test]
    fn narrow_inspector_overlays_instead_of_shrinking_the_canvas() {
        let mut app = app();
        let ctx = Context::default();
        frame(&mut app, &ctx, 800.0, 500.0);
        let before = app.canvas_rect.unwrap();
        assert!(before.width() > 700.0, "{before:?}");
        app.run("window.panel", json!({"panel": "Image Trace"})).unwrap();
        frame(&mut app, &ctx, 800.0, 500.0);
        assert_eq!(app.canvas_rect, Some(before));
        assert_eq!(active_panel(&app), "imageTrace");
        let overlay = ctx.memory(|m| m.area_rect(Id::new("workspace-inspector-overlay"))).unwrap();
        assert!(overlay.right() <= 801.0 && overlay.bottom() <= 473.0, "{overlay:?}");
        app.run("window.collapseDock", json!({"collapsed": true})).unwrap();
        frame(&mut app, &ctx, 800.0, 500.0);
        assert_eq!(app.canvas_rect, Some(before));
    }

    #[test]
    fn every_panel_is_reachable_and_history_has_its_own_tab() {
        let mut app = app();
        let ctx = Context::default();
        let panels: Vec<_> = all_panels().map(|(id, _)| id).collect();
        for id in panels {
            app.run("window.panel", json!({"panel": id})).unwrap();
            frame(&mut app, &ctx, 1440.0, 900.0);
            assert_eq!(active_panel(&app), id);
            assert!(!app.ui.dock_collapsed && app.ui.dock);
            assert!(app.canvas_rect.unwrap().right() <= 1161.0);
        }
        app.run("window.panel", json!({"panel": "history"})).unwrap();
        assert_eq!(active_panel(&app), "history");
        app.run("window.panel", json!({"panel": "properties"})).unwrap();
        assert!(app.ui.open_panel.is_none());
        assert_eq!(active_panel(&app), "properties");
    }

    #[test]
    fn pinned_panels_are_bounded_persistent_and_validate_their_ids() {
        let mut app = app();
        let original = app.ui.pinned_panels.clone();
        assert!(app.run("window.panel.pin", json!({"panel": "missing"})).is_err());
        assert_eq!(original, app.ui.pinned_panels);
        for panel in ["color", "stroke", "gradient", "swatches"] {
            app.run("window.panel.pin", json!({"panel": panel, "pinned": true})).unwrap();
        }
        assert!(app.run("window.panel.pin", json!({"panel": "brushes", "pinned": true})).is_err());
        let saved = serde_json::to_value(&app.ui).unwrap();
        let restored: crate::state::UiState = serde_json::from_value(saved).unwrap();
        assert_eq!(restored.pinned_panels, app.ui.pinned_panels);
        app.run("window.panel.pin", json!({"panel": "color", "pinned": false})).unwrap();
        assert!(!app.ui.pinned_panels.iter().any(|p| p == "color"));
    }

    #[test]
    fn diagnostics_do_not_change_document_history() {
        let mut app = app();
        let before = app.session.active().unwrap().history.undo.len();
        app.run("window.performanceHud", json!({"enabled": true})).unwrap();
        assert!(app.ui.performance_hud);
        assert_eq!(app.session.active().unwrap().history.undo.len(), before);
        app.run("window.performanceHud", json!({"enabled": false})).unwrap();
        assert!(!app.ui.performance_hud);
    }
}
