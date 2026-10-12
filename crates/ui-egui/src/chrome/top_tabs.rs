//! Compact document tabs inside the macOS title bar. Other platforms and fullscreen modes
//! keep the separate strip; isolation and pattern breadcrumbs remain above the canvas.

use egui::{Align2, CornerRadius, FontId, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::json;

use crate::theme::Tokens;
use crate::{VectorcraftApp, icons, theme, widgets};

const HEIGHT: f32 = 26.0;
const SURFACE_HEIGHT: f32 = 20.0;
const GAP: f32 = 6.0;
const MORE: f32 = 26.0;
pub(super) const ZOOM_WIDTH: f32 = 144.0;

/// Inset the visible well without reducing the titlebar's click targets.
pub(super) fn surface(rect: Rect) -> Rect {
    rect.shrink((HEIGHT - SURFACE_HEIGHT) / 2.0)
}

pub(super) fn enabled(app: &VectorcraftApp) -> bool {
    app.integrated_titlebar && app.services.native_menu.is_some() && app.ui.screen_mode < 2
}

fn id(part: &str, index: usize) -> Id {
    Id::new(("top-document-tabs", part, index))
}

pub(super) fn show(app: &mut VectorcraftApp, ui: &mut Ui, rect: Rect) {
    if rect.width() < HEIGHT {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let rect = Rect::from_min_max(pos2(rect.left(), rect.center().y - HEIGHT / 2.0), pos2(rect.right(), rect.center().y + HEIGHT / 2.0));
    let new = Rect::from_min_size(rect.min, vec2(HEIGHT, HEIGHT));
    let response = ui.interact(new, id("new", 0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("New…")));
    ui.painter().rect_filled(surface(new), CornerRadius::same(10), if response.hovered() { t.hover } else { t.panel });
    ui.painter().rect_stroke(surface(new), CornerRadius::same(10), Stroke::new(1.0, t.divider), StrokeKind::Inside);
    icons::paint(ui, "plus", Rect::from_center_size(new.center(), vec2(13.0, 13.0)), t.text);
    if response.on_hover_text(tl!("New…")).clicked()
        && let Err(e) = app.run("file.newDialog", json!({}))
    {
        app.status(e);
    }
    let left = new.right() + GAP;
    let available = rect.right() - left;
    if available < MORE {
        return;
    }
    let active = app.session.active_index().filter(|_| app.ui.home.is_none());
    let font = theme::semibold(if app.session.prefs.large_tabs { 13.0 } else { 12.0 });
    let titles: Vec<_> = app
        .session
        .documents()
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let text = d.title();
            let zoom = app.views.get(i).map_or(1.0, |v| v.zoom);
            let detail = super::tab_title(d, zoom, app.ui.view.outline);
            let width = (ui.painter().layout_no_wrap(text.clone(), font.clone(), t.text).size().x + 50.0).clamp(80.0, 240.0);
            (text, detail, d.is_dirty(), width)
        })
        .collect();
    let overflow = titles.len() > 1 && titles.iter().map(|t| t.3 + GAP).sum::<f32>() - GAP > available;
    let room = (available - if overflow { MORE + GAP } else { 0.0 }).max(0.0);
    let widths: Vec<_> = titles.iter().map(|t| t.3.min(room) + GAP).collect();
    let first = super::first_tab_shown(&widths, active, room + GAP);
    let mut x = left;
    let mut activate = None;
    let mut close = None;
    for (i, (text, detail, dirty, width)) in titles.iter().enumerate().skip(first) {
        let width = width.min(room);
        if width < 64.0 || x + width > left + room + 0.5 {
            break;
        }
        let r = Rect::from_min_size(pos2(x, rect.top()), vec2(width, HEIGHT));
        let response = ui.interact(r, id("tab", i), Sense::click());
        let selected = Some(i) == active;
        response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, detail));
        let fill = if selected {
            t.tool_active
        } else if response.hovered() {
            t.hover
        } else {
            t.panel
        };
        ui.painter().rect_filled(surface(r), CornerRadius::same(10), fill);
        ui.painter().rect_stroke(
            surface(r),
            CornerRadius::same(10),
            Stroke::new(1.0, if selected { t.input_border } else { t.divider }),
            StrokeKind::Inside,
        );
        let ink = if selected { t.text_strong } else { t.text_dim };
        if *dirty {
            ui.painter().circle_filled(r.left_center() + vec2(10.0, 0.0), 2.3, ink);
        }
        let text_left = r.left() + if *dirty { 19.0 } else { 10.0 };
        let mut job = egui::text::LayoutJob::simple(text.clone(), font.clone(), ink, (r.right() - 28.0 - text_left).max(0.0));
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.painter().layout_job(job);
        ui.painter().with_clip_rect(r.shrink2(vec2(6.0, 0.0))).galley(pos2(text_left, r.center().y - galley.size().y / 2.0), galley, ink);
        let xr = Rect::from_center_size(r.right_center() - vec2(13.0, 0.0), vec2(18.0, 18.0));
        let xresp = ui.interact(xr, id("close", i), Sense::click());
        xresp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Close")));
        if xresp.hovered() {
            ui.painter().rect_filled(xr, CornerRadius::same(9), t.hover);
        }
        icons::paint(ui, "x", xr.shrink(3.0), if xresp.hovered() { t.text_strong } else { ink });
        if xresp.on_hover_text(tl!("Close")).clicked() {
            close = Some(i);
        } else if response.on_hover_text(detail).clicked() {
            activate = Some(i);
        }
        x += width + GAP;
    }
    if overflow {
        let r = Rect::from_min_size(pos2(rect.right() - MORE, rect.top()), vec2(MORE, HEIGHT));
        let response = ui.interact(r, id("more", 0), Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Show all open documents")));
        ui.painter().rect_filled(surface(r), CornerRadius::same(10), if response.hovered() { t.hover } else { t.panel });
        icons::paint(ui, "chevrons-right", r.shrink(6.0), t.text);
        let response = response.on_hover_text(tl!("Show all open documents"));
        egui::Popup::menu(&response).show(|ui| {
            for (i, (_, detail, _, _)) in titles.iter().enumerate() {
                if widgets::menu_item_name(ui, detail, true, Some(i) == active) {
                    activate = Some(i);
                    ui.close();
                }
            }
        });
    }
    if let Some(i) = close {
        if let Err(e) = crate::unsaved::close(app, i) {
            app.status(e);
        }
    } else if let Some(i) = activate {
        app.ui.home = None;
        app.session.set_active(i);
    }
}

pub(super) fn zoom_controls(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let enabled = app.session.active().is_some() && app.ui.home.is_none();
    let mut run = None;
    let (group, _) = ui.allocate_exact_size(vec2(ZOOM_WIDTH, HEIGHT), Sense::hover());
    let mut right = group.right();
    // Right-to-left layout: the two zoom icons, actual size, then fit artboard.
    for (key, command, tip, icon, width) in [
        ("out", "view.zoomOut", "Zoom Out", Some("zoom-out"), 26.0),
        ("in", "view.zoomIn", "Zoom In", Some("zoom-in"), 26.0),
        ("actual", "view.actualSize", "Actual Size", None, 46.0),
        ("fit", "view.fitArtboard", "Fit Artboard in Window", Some("maximize-2"), 26.0),
    ] {
        let r = Rect::from_min_size(pos2(right - width, group.top()), vec2(width, HEIGHT));
        let resp = ui.interact(r, id(key, 0), if enabled { Sense::click() } else { Sense::hover() });
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tl!(tip)));
        ui.painter().rect_filled(surface(r), CornerRadius::same(10), if enabled && resp.hovered() { t.hover } else { t.panel });
        let ink = if enabled { t.text } else { t.text_disabled };
        if let Some(icon) = icon {
            icons::paint(ui, icon, Rect::from_center_size(r.center(), vec2(13.0, 13.0)), ink);
        } else {
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "100%", FontId::proportional(11.5), ink);
        }
        if resp.on_hover_text(tl!(tip)).clicked() {
            run = Some(command);
        }
        right -= width + 6.0;
    }
    if let Some(command) = run
        && let Err(e) = app.run(command, json!({}))
    {
        app.status(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_engine::Session;

    struct MenuBackend;
    impl crate::native_menu::Backend for MenuBackend {
        fn sync(&mut self, _: &crate::native_menu::MenuBar) {}
        fn drain(&mut self) -> Vec<crate::native_menu::Event> {
            vec![]
        }
    }

    struct Window {
        app: VectorcraftApp,
        ctx: egui::Context,
        width: f32,
        scale: f32,
    }

    impl Window {
        fn new(width: f32) -> Self {
            let ctx = egui::Context::default();
            theme::install_fonts(&ctx);
            theme::apply(&ctx, Default::default());
            let mut app = VectorcraftApp::new(Session::new(), Default::default());
            app.integrated_titlebar = true;
            app.services.native_menu = Some(crate::native_menu::NativeMenu::new(Box::new(MenuBackend)));
            Self { app, ctx, width, scale: 1.0 }
        }

        fn document(&mut self, name: &str) {
            self.app.run("file.new", json!({"name": name, "width": 200, "height": 100})).unwrap();
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> (f32, egui::FullOutput) {
            let mut strip = 0.0;
            let mut input =
                egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(self.width, 600.0))), events, ..Default::default() };
            input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap().native_pixels_per_point = Some(self.scale);
            let mut out = self.ctx.run_ui(input, |ui| {
                super::super::app_bar(&mut self.app, ui);
                egui::CentralPanel::default().show(ui, |ui| {
                    let start = ui.cursor().top();
                    super::super::doc_tabs(&mut self.app, ui);
                    strip = ui.cursor().top() - start;
                });
            });
            out.textures_delta.clear();
            (strip, out)
        }

        fn rect(&self, part: &str, index: usize) -> Rect {
            self.ctx.read_response(id(part, index)).unwrap_or_else(|| panic!("missing {part} {index}")).rect
        }

        fn click_at(&mut self, at: egui::Pos2) {
            self.frame(vec![egui::Event::PointerMoved(at)]);
            for pressed in [true, false] {
                self.frame(vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }]);
            }
        }

        fn click(&mut self, part: &str, index: usize) {
            self.click_at(self.rect(part, index).center());
        }
    }

    #[test]
    fn long_names_and_controls_fit_the_titlebar_at_different_widths_and_scales() {
        for width in [480.0, 560.0, 800.0, 1440.0] {
            for scale in [1.0, 2.0] {
                for large in [false, true] {
                    let mut w = Window::new(width);
                    w.scale = scale;
                    w.app.session.prefs.large_tabs = large;
                    w.document("很长的中文文件名-ABCDEFGHIJKLMNOPQRSTUVWXYZ-1234567890.jpeg");
                    w.frame(vec![]);
                    let (strip, out) = w.frame(vec![]);
                    assert_eq!(strip, 0.0, "no duplicate document strip");
                    let tab = w.rect("tab", 0);
                    assert!((tab.center().y - 16.0).abs() <= 1.0 / scale);
                    assert_eq!(tab.height(), HEIGHT);
                    let new = w.rect("new", 0);
                    assert_eq!(new.size(), vec2(26.0, 26.0), "keep the click target");
                    let visible = surface(new);
                    assert_eq!(visible.size(), vec2(20.0, 20.0));
                    assert!(visible.top() >= 5.0 && visible.bottom() <= 28.0, "space above and below the visible well: {visible:?}");
                    assert!(tab.width() >= 64.0 && tab.width() <= 240.0);
                    assert!(tab.contains_rect(w.rect("close", 0)));
                    assert!(w.rect("new", 0).right() < tab.left());
                    if width >= 560.0 {
                        let fit = w.rect("fit", 0);
                        assert!(tab.right() < fit.left());
                        assert!(w.rect("out", 0).right() < width);
                    }
                    for shape in out.shapes {
                        if let egui::Shape::Rect(r) = &shape.shape
                            && r.fill != egui::Color32::TRANSPARENT
                            && r.rect.height() > 10.0
                            && r.rect.height() < 32.0
                            && r.rect.width() < 300.0
                            && r.rect.top() < 32.0
                        {
                            assert!(r.rect.top() >= 5.0 && r.rect.bottom() <= 28.0, "oversized titlebar well: {r:?}");
                        }
                        if let egui::Shape::Text(text) = shape.shape {
                            assert_ne!(text.galley.text(), "Kaleiform", "no centered brand label");
                            if text.galley.text().starts_with("很长") {
                                let bounds = text.visual_bounding_rect().intersect(shape.clip_rect);
                                assert!(bounds.right() <= w.rect("close", 0).left(), "title must not paint over its close button");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn tabs_switch_from_home_and_close_only_the_clicked_clean_document() {
        let mut w = Window::new(1440.0);
        for name in ["First", "Second", "Third"] {
            w.document(name);
        }
        w.app.run("app.home", json!({})).unwrap();
        w.frame(vec![]);
        let tab = w.rect("tab", 0);
        w.click_at(tab.left_center() + vec2(12.0, 0.0));
        assert_eq!(w.app.session.active_index(), Some(0));
        assert!(w.app.ui.home.is_none());
        w.click("close", 1);
        assert_eq!(w.app.session.documents().iter().map(|d| d.title()).collect::<Vec<_>>(), ["First", "Third"]);
        assert_eq!(w.app.views.len(), 2);
        assert_eq!(w.app.session.active_index(), Some(1), "retain the engine's next-document close behavior");
    }

    #[test]
    fn closing_a_dirty_tab_preserves_work_until_explicit_discard() {
        let mut w = Window::new(1440.0);
        w.document("Edited");
        w.app.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 20, "height": 20})).unwrap();
        w.frame(vec![]);
        w.click("close", 0);
        assert_eq!(w.app.session.documents().len(), 1);
        assert_eq!(w.app.ui.dialog.as_ref().unwrap().kind, "saveChanges");
        w.app.ui.dialog = None;
        w.frame(vec![]);
        w.click("close", 0);
        w.app.ui.dialog.as_mut().unwrap().fields.insert("discard".into(), json!(true));
        crate::unsaved::confirm(&mut w.app).unwrap();
        assert!(w.app.session.documents().is_empty());
        assert!(w.app.views.is_empty());
    }

    #[test]
    fn new_and_zoom_buttons_use_the_existing_commands() {
        for icon in ["plus", "x", "chevrons-right", "zoom-in", "zoom-out", "maximize-2"] {
            assert!(icons::exists(icon), "missing {icon}");
        }
        let mut w = Window::new(1440.0);
        w.frame(vec![]);
        w.click("in", 0);
        assert!(w.app.views.is_empty(), "no document means no zoom action");
        w.click("new", 0);
        assert_eq!(w.app.ui.dialog.as_ref().unwrap().kind, "newDocument");
        w.app.ui.dialog = None;
        w.document("Zoom");
        w.app.canvas_rect = Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(460.0, 260.0)));
        w.frame(vec![]);
        w.click("in", 0);
        assert!(w.app.view().unwrap().zoom > 1.0);
        w.click("actual", 0);
        assert_eq!(w.app.view().unwrap().zoom, 1.0);
        w.click("out", 0);
        assert!(w.app.view().unwrap().zoom < 1.0);
        w.click("fit", 0);
        assert!(w.app.view().unwrap().zoom > 1.0);
        assert_eq!(w.app.view().unwrap().center, vectorcraft_geom::Point::new(100.0, 50.0));
    }

    #[test]
    fn overflow_keeps_the_active_tab_visible_and_can_select_hidden_documents() {
        let mut w = Window::new(560.0);
        for i in 0..20 {
            w.document(&format!("Document-{i:02}"));
        }
        assert_eq!(w.app.session.active_index(), Some(19));
        assert!(w.app.ui.home.is_none());
        w.frame(vec![]);
        assert!(w.rect("tab", 19).right() < w.rect("more", 0).left());
        w.click("more", 0);
        let (_, out) = w.frame(vec![]);
        let first = out
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(t) = &shape.shape
                    && t.galley.text().trim_start().starts_with("Document-00 @")
                {
                    Some(t.visual_bounding_rect().center())
                } else {
                    None
                }
            })
            .expect("the overflow menu lists the hidden first document");
        w.click_at(first);
        assert_eq!(w.app.session.active_index(), Some(0));
        w.frame(vec![]);
        assert!(w.rect("tab", 0).right() < w.rect("more", 0).left());
    }

    #[test]
    fn fallback_strips_and_isolation_breadcrumbs_remain_available() {
        let mut w = Window::new(1440.0);
        w.document("Isolation");
        w.app.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 20, "height": 20})).unwrap();
        w.app.run("object.group", json!({})).unwrap();
        w.app.run("object.isolate", json!({})).unwrap();
        assert!(w.frame(vec![]).0 >= 24.0);
        w.app.run("object.exitIsolation", json!({})).unwrap();
        assert_eq!(w.frame(vec![]).0, 0.0);
        w.app.ui.screen_mode = 2;
        assert!(w.frame(vec![]).0 >= 35.0);
        w.app.ui.screen_mode = 0;
        w.app.services.native_menu = None;
        assert!(w.frame(vec![]).0 >= 35.0);
    }
}
