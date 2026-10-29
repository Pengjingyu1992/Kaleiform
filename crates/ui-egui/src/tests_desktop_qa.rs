//! Dispatch through real desktop widgets: a registered command alone is not a working button.

use egui::{Context, Event, PointerButton, Pos2, RawInput, Rect, pos2, vec2};
use serde_json::json;
use vectorcraft_engine::Session;

use crate::{VectorcraftApp, theme};

struct Desktop {
    app: VectorcraftApp,
    ctx: Context,
    width: f32,
    time: f64,
}

impl Desktop {
    fn new(width: f32) -> Self {
        let ctx = Context::default();
        theme::install_fonts(&ctx);
        ctx.enable_accesskit();
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.custom_titlebar = true;
        app.run("app.language", json!({"lang": "en"})).unwrap();
        app.run("file.new", json!({"width": 320, "height": 240})).unwrap();
        let mut d = Self { app, ctx, width, time: 0.0 };
        d.settle();
        d
    }

    fn frame(&mut self, events: Vec<Event>) -> egui::FullOutput {
        self.time += 0.1;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(self.width, 900.0))),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut out = self.ctx.run_ui(input, |ui| {
            self.app.logic(ui.ctx());
            self.app.ui(ui);
        });
        out.textures_delta.clear();
        out
    }

    fn settle(&mut self) {
        for _ in 0..4 {
            self.frame(vec![]);
        }
    }

    fn at(&mut self, label: &str) -> Pos2 {
        self.settle();
        let out = self.frame(vec![]);
        let nodes = out.platform_output.accesskit_update.unwrap().nodes;
        let node = nodes
            .iter()
            .find(|(_, n)| n.label() == Some(label) && !n.is_disabled())
            .unwrap_or_else(|| panic!("missing enabled {label}: {:?}", nodes.iter().filter_map(|(_, n)| n.label()).collect::<Vec<_>>()));
        let r = node.1.bounds().expect("a visible widget");
        assert!(r.x0 >= 0.0 && r.x1 <= self.width as f64 + 1.0 && r.y0 >= 0.0 && r.y1 <= 901.0, "{label}: {r:?}");
        pos2(((r.x0 + r.x1) / 2.0) as f32, ((r.y0 + r.y1) / 2.0) as f32)
    }

    fn click(&mut self, label: &str) {
        let at = self.at(label);
        self.click_at(at);
    }

    fn click_at(&mut self, at: Pos2) {
        self.frame(vec![Event::PointerMoved(at)]);
        for pressed in [true, false] {
            self.frame(vec![Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() }]);
        }
        self.settle();
    }
}

#[test]
fn desktop_header_buttons_dispatch_at_wide_and_narrow_sizes() {
    for width in [800.0, 1440.0] {
        let mut d = Desktop::new(width);
        d.app.run("shape.rectangle", json!({"x": 20, "y": 20, "width": 40, "height": 30})).unwrap();
        let art = d.app.session.doc().unwrap().doc.layers[0].count();
        d.click("Undo");
        assert_eq!(d.app.session.doc().unwrap().doc.layers[0].count(), art - 1);
        d.click("Redo");
        assert_eq!(d.app.session.doc().unwrap().doc.layers[0].count(), art);
        d.click("Search commands and tools");
        assert!(d.app.ui.palette_open);
        d.app.ui.palette_open = false;
        d.click("New…");
        assert_eq!(d.app.ui.dialog.as_ref().unwrap().kind, "newDocument");
        crate::dialogs::cancel(&mut d.app);
        d.click("Home");
        assert!(d.app.ui.home.is_some());
        let tab = d.ctx.read_response(egui::Id::new(("top-document-tabs", "tab", 0_usize))).unwrap().rect;
        d.click_at(tab.center());
        assert!(d.app.ui.home.is_none());
    }
}

#[test]
fn inspector_buttons_switch_panels_and_restore_after_hiding() {
    for width in [800.0, 1440.0] {
        let mut d = Desktop::new(width);
        d.app.run("window.panel", json!({"panel": "properties"})).unwrap();
        for (label, panel) in [("Layers", "layers"), ("Libraries", "libraries"), ("History", "history"), ("Properties", "properties")] {
            d.click(label);
            assert_eq!(d.app.ui.open_panel.as_deref().unwrap_or(d.app.ui.dock_tab.info().0), panel);
        }
        d.click("Hide Panels");
        assert!(d.app.ui.dock_collapsed);
        d.click("Panels");
        assert!(!d.app.ui.dock_collapsed && d.app.ui.dock);
        d.click("All Panels");
        assert!(egui::Popup::is_any_open(&d.ctx));
    }
}

#[test]
fn custom_window_caption_buttons_expose_names_to_assistive_tools() {
    let mut d = Desktop::new(800.0);
    let out = d.frame(vec![]);
    let tree = out.platform_output.accesskit_update.unwrap();
    for label in ["Minimize", "Maximize", "Close"] {
        let node = tree
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some(label) && n.bounds().is_some_and(|r| r.x0 >= (800.0 - crate::titlebar::WIDTH) as f64))
            .unwrap_or_else(|| panic!("missing caption {label}"));
        assert_eq!(node.1.role(), egui::accesskit::Role::Button);
        assert!(!node.1.is_disabled());
    }
}

#[test]
fn every_registered_ui_command_preserves_document_invariants() {
    // No host callbacks: Open/Save/Print/links cannot show system dialogs or touch user files.
    // Rejection for an absent selection, unsupported host, or required params is valid.
    for fixture in [None, Some(vectorcraft_testkit::fixtures::Fixture::Single), Some(vectorcraft_testkit::fixtures::Fixture::Multi)] {
        for (id, _, _, _) in crate::menus::UI_COMMANDS {
            let session = fixture.map_or_else(Session::new, |f| f.session());
            let mut app = VectorcraftApp::new(session, Default::default());
            app.custom_titlebar = true;
            let run = vectorcraft_testkit::catch_quiet(|| {
                let _ = app.run(id, json!({}));
                crate::dialogs::cancel(&mut app);
                assert!(!app.session.in_interaction(), "{id}: Cancel left an interaction open");
                vectorcraft_testkit::invariants::check_session(&app.session).unwrap();
            });
            assert!(run.is_ok(), "{id} [{fixture:?}]: {run:?}");
        }
    }
}
