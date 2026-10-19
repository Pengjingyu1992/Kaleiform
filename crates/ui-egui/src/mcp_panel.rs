//! Local bridge management. Authentication and per-command grants are separate backend work.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use egui::{Context, Ui};
use serde_json::json;

use crate::{VectorcraftApp, theme::Tokens, widgets};

pub struct ControlStatus {
    pub port: u16,
    pub address: String,
    pub running: AtomicBool,
    pub paused: AtomicBool,
    pub connections: AtomicUsize,
    shutdown: std::sync::Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl ControlStatus {
    pub fn new(port: u16) -> Arc<Self> {
        Arc::new(Self {
            port,
            address: format!("127.0.0.1:{port}"),
            running: AtomicBool::new(true),
            paused: AtomicBool::new(false),
            connections: AtomicUsize::new(0),
            shutdown: Default::default(),
        })
    }

    pub fn accepting(&self) -> bool {
        self.running.load(Ordering::Relaxed) && !self.paused.load(Ordering::Relaxed)
    }

    pub fn set_shutdown(&self, shutdown: Box<dyn Fn() + Send + Sync>) {
        *self.shutdown.lock().unwrap_or_else(|e| e.into_inner()) = Some(shutdown);
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(shutdown) = self.shutdown.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            shutdown();
        }
    }
}

pub type StartControl = Box<dyn Fn(u16, Context) -> Result<(std::sync::mpsc::Receiver<crate::ControlRequest>, Arc<ControlStatus>), String>>;

#[derive(Clone)]
pub(crate) struct Activity {
    pub method: String,
    pub elapsed_ms: f64,
    pub result: Option<bool>,
}

pub(crate) struct State {
    pub open: bool,
    pub activity_tab: bool,
    pub requested_port: Option<u16>,
    pub activity: VecDeque<Activity>,
    pub port: u16,
    pub error: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self { open: false, activity_tab: false, requested_port: None, activity: VecDeque::new(), port: 7979, error: None }
    }
}

impl State {
    pub fn record(&mut self, method: &str, elapsed_ms: f64, result: Option<bool>) {
        if self.activity.len() == 128 {
            self.activity.pop_front();
        }
        // Only protocol method names are kept; parameters, document data and paths stay out.
        let method = match method {
            "engine.execute"
            | "engine.commands"
            | "command"
            | "document.inspect"
            | "ui.inspect"
            | "ui.menu.invoke"
            | "ui.menu.list"
            | "ui.contextMenu.list"
            | "ui.tool.select"
            | "ui.tool.list"
            | "ui.pointer"
            | "ui.key"
            | "ui.move"
            | "ui.click"
            | "ui.drag"
            | "ui.wheel"
            | "ui.text"
            | "ui.set"
            | "ui.dialog.set"
            | "ui.dialog.confirm"
            | "ui.dialog.cancel"
            | "ui.resize"
            | "ui.focus"
            | "ui.screenshot"
            | "ui.render"
            | "app.open"
            | "app.save"
            | "app.export"
            | "app.quit" => method,
            _ => "Other Request",
        };
        self.activity.push_back(Activity { method: method.to_string(), elapsed_ms, result });
    }
}

pub(crate) fn status(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let label = match app.services.control_status.as_ref() {
        Some(s) if s.running.load(Ordering::Relaxed) && s.paused.load(Ordering::Relaxed) => tl!("MCP · Paused"),
        Some(s) if s.running.load(Ordering::Relaxed) => tl!("MCP · Local"),
        _ => "MCP",
    };
    if ui
        .add(egui::Button::new(egui::RichText::new(tl!(label)).size(11.0).color(t.text_dim)).frame(false))
        .on_hover_text(tl!("MCP Connections"))
        .clicked()
    {
        app.run("window.mcp", json!({})).ok();
    }
}

pub(crate) fn show(app: &mut VectorcraftApp, ctx: &Context) {
    if !app.mcp.open {
        return;
    }
    let mut open = true;
    egui::Window::new(tl!("MCP Connections"))
        .id(egui::Id::new("mcp-connections"))
        .open(&mut open)
        .default_width(660.0)
        .default_height(420.0)
        .max_width((ctx.content_rect().width() - 32.0).max(280.0))
        .max_height((ctx.content_rect().height() - 48.0).max(180.0))
        .vscroll(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(!app.mcp.activity_tab, tl!("Connections")).clicked() {
                    app.mcp.activity_tab = false;
                }
                if ui.selectable_label(app.mcp.activity_tab, tl!("Activity")).clicked() {
                    app.mcp.activity_tab = true;
                }
            });
            ui.separator();
            if app.mcp.activity_tab {
                activity(app, ui);
            } else {
                connections(app, ui);
            }
        });
    app.mcp.open = open;
}

fn connections(app: &mut VectorcraftApp, ui: &mut Ui) {
    let monitor = app.services.control_status.clone();
    let running = monitor.as_ref().is_some_and(|s| s.running.load(Ordering::Relaxed));
    let paused = monitor.as_ref().is_some_and(|s| s.paused.load(Ordering::Relaxed));
    let t = Tokens::get(ui.ctx());
    widgets::section_header(ui, if running { tl!("Local bridge running") } else { tl!("Local bridge stopped") });
    if let Some(s) = monitor.as_ref().filter(|_| running) {
        app.mcp.port = s.port;
        ui.monospace(&s.address);
        ui.label(format!("{}: {}", tl!("Local connections"), s.connections.load(Ordering::Relaxed)));
        ui.horizontal(|ui| {
            if ui.button(if paused { tl!("Resume Requests") } else { tl!("Pause Requests") }).clicked() {
                app.run("window.mcp.pause", json!({"paused": !paused})).ok();
                ui.ctx().request_repaint();
            }
            if ui.button(tl!("Stop Bridge")).clicked() {
                app.run("window.mcp.stop", json!({})).ok();
                ui.ctx().request_repaint();
            }
        });
    } else {
        ui.horizontal(|ui| {
            ui.label(tl!("Port"));
            ui.add(egui::DragValue::new(&mut app.mcp.port).range(1..=65535));
            ui.add_enabled_ui(app.services.start_control.is_some(), |ui| {
                if ui.button(tl!("Start Local Bridge")).clicked() {
                    app.run("window.mcp.start", json!({"port": app.mcp.port})).ok();
                }
            });
        });
    }
    if let Some(error) = &app.mcp.error {
        ui.colored_label(t.error, crate::i18n::msg(error));
    }
    ui.add_space(12.0);
    ui.add(egui::Label::new(egui::RichText::new(tl!("Client identity is not verified by this bridge.")).size(13.0).color(t.text)));
    ui.label(
        egui::RichText::new(tl!("Connected clients can read and edit documents. Their model service may receive returned content."))
            .size(13.0)
            .color(t.text),
    );
    if running && let (Some(cli), Some(s)) = (&app.services.mcp_cli, &monitor) {
        let config = json!({"mcpServers": {"kaleiform": {"command": cli, "args": ["mcp", "--connect", &s.address]}}});
        let config = serde_json::to_string_pretty(&config).unwrap_or_default();
        ui.add_space(12.0);
        widgets::section_header(ui, tl!("Client Configuration"));
        egui::ScrollArea::both().max_height(150.0).auto_shrink([false, true]).show(ui, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(&config).monospace()).selectable(true).extend());
        });
        if widgets::command_button(ui, "copy", tl!("Copy Configuration"), egui::vec2(150.0, 28.0), false).clicked() {
            ui.ctx().copy_text(config);
        }
    }
}

fn activity(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    if app.mcp.activity.is_empty() {
        ui.label(tl!("No requests yet"));
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for entry in app.mcp.activity.iter().rev() {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if entry.method == "Other Request" { tl!("Other Request") } else { &entry.method }).monospace().color(t.text),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(match entry.result {
                        Some(true) => tl!("Succeeded"),
                        Some(false) => tl!("Failed"),
                        None => tl!("Accepted"),
                    });
                    ui.label(format!("{:.2} ms", entry.elapsed_ms));
                });
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_buttons_resume_pause_and_stop_without_using_the_bridge() {
        fn frame(app: &mut VectorcraftApp, ctx: &Context, events: Vec<egui::Event>) -> egui::FullOutput {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0))),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                app.logic(ui.ctx());
                app.ui(ui);
            });
            out.textures_delta.clear();
            out
        }
        fn label_center(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == label => Some(t.visual_bounding_rect().center()),
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| label_center(s, label)),
                _ => None,
            }
        }
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        let ctx = Context::default();
        let status = ControlStatus::new(7979);
        status.paused.store(true, Ordering::Relaxed);
        app.services.control_status = Some(status.clone());
        app.mcp.open = true;
        for _ in 0..4 {
            frame(&mut app, &ctx, vec![]);
        }
        for (label, running, paused) in
            [(tl!("Resume Requests"), true, false), (tl!("Pause Requests"), true, true), (tl!("Stop Bridge"), false, true)]
        {
            let out = frame(&mut app, &ctx, vec![]);
            let at = out.shapes.iter().find_map(|s| label_center(&s.shape, label)).unwrap_or_else(|| panic!("missing {label}"));
            frame(&mut app, &ctx, vec![egui::Event::PointerMoved(at)]);
            for pressed in [true, false] {
                frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }],
                );
            }
            assert_eq!(status.running.load(Ordering::Relaxed), running, "{label}");
            assert_eq!(status.paused.load(Ordering::Relaxed), paused, "{label}");
        }
        assert!(app.session.documents().is_empty());
    }

    #[test]
    fn activity_is_bounded_and_never_contains_request_parameters() {
        let mut state = State::default();
        for _ in 0..200 {
            state.record("engine.execute", 1.0, Some(true));
        }
        assert_eq!(state.activity.len(), 128);
        assert!(state.activity.iter().all(|a| a.method == "engine.execute"));
        state.record("private-document-name", 0.0, Some(false));
        assert_eq!(state.activity.back().unwrap().method, "Other Request");
    }

    #[test]
    fn paused_and_stopped_bridges_refuse_all_control_requests() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        let ctx = Context::default();
        let status = ControlStatus::new(7979);
        app.services.control_status = Some(status.clone());
        for stopped in [false, true] {
            status.running.store(!stopped, Ordering::Relaxed);
            status.paused.store(!stopped, Ordering::Relaxed);
            let (req, _) = crate::ControlRequest::new("engine.execute", json!({"command": "file.new", "params": {}}));
            let crate::control::Outcome::Done(reply) = crate::control::handle(&mut app, &ctx, &req) else { panic!("unexpected screenshot") };
            assert_eq!(reply["ok"], false);
            assert!(app.session.documents().is_empty());
        }
    }
}
