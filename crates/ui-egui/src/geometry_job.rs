//! Cancellable desktop Offset Path. The engine owns preparation and atomic commit.
use crate::VectorcraftApp;
use serde_json::Value;
#[cfg(not(target_arch = "wasm32"))]
use serde_json::json;
#[cfg(not(target_arch = "wasm32"))]
use vectorcraft_engine::OffsetResult;
use vectorcraft_engine::OffsetTask;
use vectorcraft_pathops::ComputationBudget;
#[cfg(not(target_arch = "wasm32"))]
use vectorcraft_pathops::offset_budget;

#[derive(Default)]
pub struct GeometryJob {
    task: Option<OffsetTask>,
    budget: Option<ComputationBudget>,
    #[cfg(not(target_arch = "wasm32"))]
    rx: Option<std::sync::mpsc::Receiver<Result<OffsetResult, String>>>,
    cancelled: bool,
    #[cfg(not(target_arch = "wasm32"))]
    preview: bool,
    #[cfg(not(target_arch = "wasm32"))]
    params: Value,
}

impl GeometryJob {
    pub fn running(&self) -> bool {
        self.task.is_some()
    }
    pub fn cancelled(&self) -> bool {
        self.cancelled
    }
    pub fn cancel(&mut self) {
        if let Some(budget) = &self.budget {
            budget.cancel();
            self.cancelled = true;
        }
    }
}

impl Drop for GeometryJob {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn start(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    start_with_preview(app, p, false)
}

pub fn start_preview(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    start_with_preview(app, p, true)
}

fn start_with_preview(app: &mut VectorcraftApp, p: &Value, preview: bool) -> Result<Value, String> {
    #[cfg(target_arch = "wasm32")]
    {
        if preview {
            app.session.begin_interaction("Offset Path").map_err(|e| e.to_string())?;
        }
        return if preview { app.session.preview("object.path.offsetPath", p) } else { app.session.execute("object.path.offsetPath", p) }
            .map_err(|e| e.to_string());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if app.geometry_job.running() {
            return Err("Offset Path: another offset is still running".into());
        }
        let task = OffsetTask::prepare(&app.session, p).map_err(|e| e.to_string())?;
        let snapshot = task.clone();
        let budget = offset_budget();
        let worker_budget = budget.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("offset-path".into())
            .spawn(move || {
                let result = vectorcraft_engine::guard::catch_panic(|| task.compute(&worker_budget))
                    .map_err(|e| format!("Offset Path: {e}"))
                    .and_then(|r| r.map_err(|e| e.to_string()));
                // Closing the app discards the result; the budget was cancelled by Drop.
                let _ = tx.send(result);
            })
            .map_err(|e| format!("Offset Path: {e}"))?;
        app.geometry_job = GeometryJob { task: Some(snapshot), budget: Some(budget), rx: Some(rx), cancelled: false, preview, params: p.clone() };
        Ok(json!({"background": true, "status": "Offset Path"}))
    }
}

pub fn poll(app: &mut VectorcraftApp) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if app.geometry_job.task.as_ref().is_some_and(|t| !t.is_current(&app.session)) {
            app.geometry_job.cancel();
        }
        let Some(rx) = &app.geometry_job.rx else { return };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("Offset Path: worker stopped".into()),
        };
        let cancelled = app.geometry_job.cancelled;
        let still_current = app.geometry_job.task.as_ref().is_some_and(|t| t.is_current(&app.session));
        let preview = app.geometry_job.preview;
        let params = app.geometry_job.params.clone();
        app.geometry_job = GeometryJob::default();
        let result = if cancelled {
            Err("Offset Path: cancelled; original artwork kept".into())
        } else {
            result.and_then(|r| {
                if preview {
                    app.session.begin_interaction("Offset Path").map_err(|e| e.to_string())?;
                    app.session.preview_offset(r).map_err(|e| e.to_string())
                } else {
                    app.session.finish_offset(r).map_err(|e| e.to_string())
                }
            })
        };
        if preview {
            if let Some(dialog) = &mut app.ui.dialog {
                dialog.fields.insert("__offsetReady".into(), if result.is_ok() { params } else { Value::Null });
            }
            if result.is_err() && still_current {
                let _ = app.session.cancel_interaction();
            }
        }
        app.status(match result {
            Ok(_) => "Offset Path: complete".into(),
            Err(e) => e,
        });
        app.sync_views();
    }
    #[cfg(target_arch = "wasm32")]
    let _ = app;
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::Services;
    use std::time::{Duration, Instant};
    use vectorcraft_engine::Session;

    fn app() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(Session::new(), Services::default());
        app.run("file.new", json!({"width": 500, "height": 500})).unwrap();
        app.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 40, "height": 30})).unwrap();
        app.run("select.all", json!({})).unwrap();
        app
    }

    fn finish(app: &mut VectorcraftApp) {
        let t = Instant::now();
        while app.geometry_job.running() && t.elapsed() < Duration::from_secs(5) {
            poll(app);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!app.geometry_job.running(), "worker did not finish");
    }

    #[test]
    fn background_offset_can_be_cancelled_or_discarded_after_an_edit() {
        for cancel in [true, false] {
            let mut app = app();
            let before = app.session.doc().unwrap().doc.clone();
            let history = app.session.doc().unwrap().history.undo.len();
            assert_eq!(app.run("object.path.offsetPath", json!({"offset": 3})).unwrap()["background"], true);
            if cancel {
                app.run("geometry.cancel", json!({})).unwrap();
            } else {
                app.run("select.none", json!({})).unwrap();
            }
            finish(&mut app);
            assert!(std::sync::Arc::ptr_eq(&before, &app.session.doc().unwrap().doc));
            assert_eq!(app.session.doc().unwrap().history.undo.len(), history);
        }
    }

    #[test]
    fn background_offset_finishes_through_the_engine_command() {
        let mut app = app();
        let before = app.session.doc().unwrap().history.undo.len();
        app.run("object.path.offsetPath", json!({"offset": 3})).unwrap();
        assert!(app.run("object.path.offsetPath", json!({"offset": 4})).is_err());
        finish(&mut app);
        assert_eq!(app.session.doc().unwrap().history.undo.len(), before + 1);
        assert_eq!(app.session.journal.last().unwrap().0, "object.path.offsetPath");
    }

    #[test]
    fn editing_while_a_preview_computes_keeps_the_new_edit() {
        let mut app = app();
        start_preview(&mut app, &json!({"offset": 3})).unwrap();
        assert!(!app.session.in_interaction(), "pending geometry must not own live edits");
        app.run("shape.rectangle", json!({"x": 100, "y": 100, "width": 30, "height": 30})).unwrap();
        let edited = app.session.doc().unwrap().doc.clone();
        let history = app.session.doc().unwrap().history.undo.len();
        finish(&mut app);
        assert!(std::sync::Arc::ptr_eq(&edited, &app.session.doc().unwrap().doc));
        assert_eq!(app.session.doc().unwrap().history.undo.len(), history);
    }
}
