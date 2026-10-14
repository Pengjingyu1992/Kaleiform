//! One cancellable tracing worker and one replaceable pending request.
use std::sync::mpsc::{Receiver, channel};

use serde_json::{Value, json};
use vectorcraft_engine::{TraceResult, TraceTask};
use vectorcraft_trace::{Stage, TraceControl};

use crate::VectorcraftApp;

struct Active {
    task: TraceTask,
    control: TraceControl,
    rx: Receiver<Result<TraceResult, String>>,
    cancelled: bool,
}

#[derive(Default)]
pub(crate) struct TraceJob {
    active: Option<Active>,
    queued: Option<TraceTask>,
    serial: u64,
}

impl TraceJob {
    pub fn running(&self) -> bool {
        self.active.is_some()
    }
    pub fn cancelled(&self) -> bool {
        self.active.as_ref().is_some_and(|a| a.cancelled) && self.queued.is_none()
    }
    pub fn stage(&self) -> Stage {
        self.active.as_ref().map_or(Stage::Decode, |a| a.control.stage())
    }
    pub fn settings(&self, session: &vectorcraft_engine::Session) -> Option<(u64, Value)> {
        self.queued
            .as_ref()
            .or_else(|| self.active.as_ref().filter(|a| !a.cancelled).map(|a| &a.task))
            .filter(|task| task.is_current(session))
            .map(|task| (self.serial, task.settings()))
    }
    pub fn cancel(&mut self) {
        self.queued = None;
        if let Some(active) = &mut self.active {
            active.cancelled = true;
            active.control.cancel();
        }
    }
    fn launch(&mut self, task: TraceTask) -> Result<(), String> {
        let control = TraceControl::default();
        let (snapshot, worker_control) = (task.clone(), control.clone());
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("image-trace".into())
            .spawn(move || {
                let result = vectorcraft_engine::guard::catch_panic(|| task.compute(&worker_control))
                    .map_err(|e| format!("Image Trace: {e}"))
                    .and_then(|r| r.map_err(|e| e.to_string()));
                // The app may have closed or replaced this task.
                let _ = tx.send(result);
            })
            .map_err(|e| format!("Image Trace: {e}"))?;
        self.active = Some(Active { task: snapshot, control, rx, cancelled: false });
        Ok(())
    }
}

impl Drop for TraceJob {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn start(app: &mut VectorcraftApp, p: &Value, expand: bool) -> Result<Value, String> {
    let task = TraceTask::prepare(&app.session, p, expand).map_err(|e| e.to_string())?;
    if app.trace_job.running() {
        app.trace_job.cancel();
        app.trace_job.queued = Some(task);
    } else {
        app.trace_job.launch(task)?;
    }
    app.trace_job.serial = app.trace_job.serial.wrapping_add(1);
    app.status("Computing Image Trace…");
    Ok(json!({ "background": true, "status": "Image Trace" }))
}

pub fn poll(app: &mut VectorcraftApp) {
    if let Some(active) = &mut app.trace_job.active
        && !active.task.is_current(&app.session)
    {
        active.cancelled = true;
        active.control.cancel();
    }
    let Some(active) = &app.trace_job.active else { return };
    let result = match active.rx.try_recv() {
        Ok(r) => r,
        Err(std::sync::mpsc::TryRecvError::Empty) => return,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("Image Trace: worker stopped".into()),
    };
    let cancelled = active.cancelled;
    app.trace_job.active = None;
    if let Some(task) = app.trace_job.queued.take().filter(|t| t.is_current(&app.session)) {
        if let Err(e) = app.trace_job.launch(task) {
            app.status(e);
        }
        return;
    }
    let result = if cancelled {
        Err("Image Trace: cancelled; original artwork kept".into())
    } else {
        result.and_then(|r| app.session.finish_trace(r).map_err(|e| e.to_string()))
    };
    app.status(match result {
        Ok(_) => "Image Trace: complete".into(),
        Err(e) => e,
    });
    app.sync_views();
}

pub fn stage_label(stage: Stage) -> &'static str {
    match stage {
        Stage::Decode => "Decoding trace image…",
        Stage::Palette => "Computing trace colors…",
        Stage::Noise => "Removing trace noise…",
        Stage::Contours => "Tracing contours and curves…",
        Stage::Building => "Building trace paths…",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn app() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width":100,"height":100})).unwrap();
        let img = image::RgbaImage::from_fn(40, 40, |x, _| image::Rgba(if x < 20 { [0, 0, 0, 255] } else { [255, 255, 255, 255] }));
        let mut png = vec![];
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        app.run("file.place", json!({"name":"test.png","dataBase64":vectorcraft_format::base64_encode(&png),"link":false})).unwrap();
        app.services.background_trace = true;
        app
    }

    fn finish(app: &mut VectorcraftApp) {
        let start = Instant::now();
        while app.trace_job.running() && start.elapsed() < Duration::from_secs(5) {
            poll(app);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!app.trace_job.running(), "trace worker did not finish");
    }

    #[test]
    fn desktop_trace_is_async_and_commits_one_undo_step() {
        let mut app = app();
        let before = app.session.doc().unwrap().doc.clone();
        let steps = app.session.doc().unwrap().history.undo.len();
        assert_eq!(app.run("imageTrace.make", json!({})).unwrap()["background"], true);
        assert_eq!(app.session.doc().unwrap().doc, before);
        assert!(app.run("imageTrace.expand", json!({})).is_err());
        finish(&mut app);
        assert_eq!(app.session.doc().unwrap().history.undo.len(), steps + 1);
        app.run("edit.undo", json!({})).unwrap();
        assert_eq!(app.session.doc().unwrap().doc, before);
    }

    #[test]
    fn cancellation_and_stale_results_preserve_the_current_art_and_journal() {
        for edit in [false, true] {
            let mut app = app();
            app.run("imageTrace.make", json!({})).unwrap();
            if edit {
                app.run("shape.rectangle", json!({"x":1,"y":2,"width":10,"height":10})).unwrap();
            } else {
                app.run("imageTrace.cancel", json!({})).unwrap();
            }
            let before = app.session.doc().unwrap().doc.clone();
            let journals = app.session.journal.len();
            finish(&mut app);
            assert_eq!(app.session.doc().unwrap().doc, before);
            assert_eq!(app.session.journal.len(), journals);
        }
    }

    #[test]
    fn only_the_latest_requested_settings_commit() {
        let mut app = app();
        let steps = app.session.doc().unwrap().history.undo.len();
        for threshold in [10, 20, 30, 40, 50] {
            app.run("imageTrace.make", json!({"params":{"threshold":threshold,"ignoreWhite":true}})).unwrap();
        }
        assert!(app.trace_job.queued.is_some());
        finish(&mut app);
        assert_eq!(app.session.doc().unwrap().history.undo.len(), steps + 1);
        let st = app.session.doc().unwrap();
        let n = st.doc.node(st.selection.objects[0]).unwrap();
        assert_eq!(n.trace.as_ref().unwrap()["params"]["threshold"], 50);
    }
}
