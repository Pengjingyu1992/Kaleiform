//! Background rendering with one running job, one latest request and one completed image.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc::{SyncSender, sync_channel},
};

use vectorcraft_doc::Document;
use vectorcraft_geom::Affine;
use vectorcraft_render::{RenderOptions, Rendered, Renderer};

use crate::CacheKey;

pub struct Job {
    pub key: CacheKey,
    pub doc: Arc<Document>,
    pub w: u32,
    pub h: u32,
    pub view: Affine,
    pub opts: RenderOptions,
}

pub struct Done {
    pub key: CacheKey,
    pub img: Rendered,
    pub ms: f64,
}

pub struct Worker {
    wake: SyncSender<()>,
    mailbox: Arc<Mutex<Mailbox>>,
    generation: Arc<AtomicU64>,
    pub pending: Option<CacheKey>,
}

#[derive(Default)]
struct Mailbox {
    job: Option<(u64, Job)>,
    done: Option<Done>,
    failed: Option<(CacheKey, String)>,
}

impl Worker {
    /// Spawn the worker thread (None on wasm, where rendering stays synchronous).
    pub fn spawn(ctx: egui::Context) -> Option<Self> {
        if cfg!(target_arch = "wasm32") {
            return None;
        }
        let mailbox = Arc::new(Mutex::new(Mailbox::default()));
        let shared = mailbox.clone();
        let generation = Arc::new(AtomicU64::new(0));
        let latest = generation.clone();
        let (wake, rx) = sync_channel(1);
        std::thread::Builder::new()
            .name("vectorcraft-render".into())
            .spawn(move || {
                let mut r = Renderer::new();
                while rx.recv().is_ok() {
                    let Some((serial, job)) = shared.lock().ok().and_then(|mut m| m.job.take()) else { continue };
                    let t0 = crate::now_ms();
                    let image = vectorcraft_engine::guard::catch_panic(|| r.render(&job.doc, job.w, job.h, job.view, &job.opts));
                    if let Ok(mut m) = shared.lock()
                        && latest.load(Ordering::Acquire) == serial
                    {
                        match image {
                            Ok(img) => m.done = Some(Done { key: job.key, img, ms: crate::now_ms() - t0 }),
                            Err(e) => {
                                m.failed = Some((job.key, format!("Rendering failed: {e}")));
                                r = Renderer::new();
                            }
                        }
                        ctx.request_repaint();
                    }
                }
            })
            .ok()?;
        Some(Self { wake, mailbox, generation, pending: None })
    }

    pub fn submit(&mut self, job: Job) {
        if self.pending.as_ref() == Some(&job.key) {
            return;
        }
        if let Ok(mut m) = self.mailbox.lock() {
            let serial = self.generation.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
            self.pending = Some(job.key.clone());
            m.job = Some((serial, job));
            m.done = None;
            m.failed = None;
            // One queued wake already covers the newest request.
            let _ = self.wake.try_send(());
        }
    }

    pub fn poll(&mut self) -> Option<Done> {
        let done = self.mailbox.lock().ok()?.done.take()?;
        if self.pending.as_ref() != Some(&done.key) {
            return None;
        }
        self.pending = None;
        Some(done)
    }
    pub fn poll_error(&mut self) -> Option<(CacheKey, String)> {
        let error = self.mailbox.lock().ok()?.failed.take()?;
        if self.pending.as_ref() != Some(&error.0) {
            return None;
        }
        self.pending = None;
        Some(error)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut m) = self.mailbox.lock() {
            m.job = None;
            m.done = None;
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn job(doc: usize, revision: u64) -> Job {
        Job {
            key: CacheKey {
                doc,
                revision,
                zoom: 1.0,
                cx: 0.0,
                cy: 0.0,
                w: 32,
                h: 32,
                outline: false,
                trim: false,
                ppp: 1.0,
                hidden: vec![],
                rot: 0.0,
                anti_alias: true,
                placed: 0,
                pixel: None,
                smooth_images: true,
            },
            doc: Arc::new(Document::new(32.0, 32.0)),
            w: 32,
            h: 32,
            view: Affine::IDENTITY,
            opts: RenderOptions::default(),
        }
    }

    #[test]
    fn render_mailbox_keeps_only_the_latest_request_and_completion() {
        let mut worker = Worker::spawn(egui::Context::default()).unwrap();
        for i in 1..=100 {
            worker.submit(job(i, i as u64));
        }
        let start = Instant::now();
        let result = loop {
            if let Some(done) = worker.poll() {
                break done;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(result.key.doc, 100);
        assert_eq!(result.key.revision, 100);
        assert!(worker.pending.is_none());
        assert!(worker.poll().is_none());
    }

    #[test]
    fn first_canvas_frame_and_document_switch_submit_without_synchronous_rendering() {
        let mut app = crate::VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.run("file.new", serde_json::json!({"width":100,"height":100})).unwrap();
        let ctx = egui::Context::default();
        let frame = |app: &mut crate::VectorcraftApp| {
            ctx.run_ui(
                egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0))), ..Default::default() },
                |ui| crate::canvas::show(app, ui),
            )
            .textures_delta
            .clear();
        };
        frame(&mut app);
        assert!(app.canvas.worker.as_ref().unwrap().pending.is_some());
        assert!(app.canvas.key.is_none());
        let start = Instant::now();
        while app.canvas.key.is_none() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(1));
            frame(&mut app);
        }
        let old = app.canvas.key.as_ref().unwrap().doc;
        app.run("file.new", serde_json::json!({"width":50,"height":50})).unwrap();
        frame(&mut app);
        assert_eq!(app.canvas.key.as_ref().unwrap().doc, old);
        assert_ne!(app.canvas.worker.as_ref().unwrap().pending.as_ref().unwrap().doc, old);
    }
}
