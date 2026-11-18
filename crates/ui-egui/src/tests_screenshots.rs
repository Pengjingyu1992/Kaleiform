//! A backend can consume a capture command on a transiently unavailable surface.
//! Simulate dropped/late events without requiring a GPU or sleeping in tests.

use std::sync::{Arc, mpsc};

use egui::{Context, Event, RawInput, UserData, ViewportCommand, ViewportId};
use serde_json::Value;
use vectorcraft_engine::Session;

use crate::VectorcraftApp;

fn fixture() -> (VectorcraftApp, Context, mpsc::Receiver<Value>) {
    let mut app = VectorcraftApp::new(Session::new(), Default::default());
    let (tx, rx) = mpsc::channel();
    app.pending_screenshots.push((7, None, tx, 8000.0));
    app.queued_screenshots.push((7, 0.0, 0, 2));
    (app, Context::default(), rx)
}

fn capture(token: u64) -> Event {
    Event::Screenshot {
        viewport_id: ViewportId::ROOT,
        user_data: UserData::new(token),
        image: Arc::new(egui::ColorImage::filled([2, 3], egui::Color32::BLUE)),
    }
}

fn frame(app: &mut VectorcraftApp, ctx: &Context, now: f64, events: Vec<Event>) -> Vec<u64> {
    let mut out = ctx.run_ui(RawInput { events, ..Default::default() }, |ui| {
        app.collect_screenshots_at(ui.ctx(), now);
        app.issue_screenshots_at(ui.ctx(), now);
    });
    let tokens = out
        .viewport_output
        .values()
        .flat_map(|v| &v.commands)
        .filter_map(|c| match c {
            ViewportCommand::Screenshot(data) => data.data.as_ref()?.downcast_ref::<u64>().copied(),
            _ => None,
        })
        .collect();
    out.textures_delta.clear();
    tokens
}

#[test]
fn a_dropped_capture_is_reissued_once_and_a_late_first_frame_replies_once() {
    let (mut app, ctx, rx) = fixture();
    assert!(frame(&mut app, &ctx, 0.0, vec![]).is_empty());
    assert!(frame(&mut app, &ctx, 10.0, vec![]).is_empty());
    assert_eq!(frame(&mut app, &ctx, 20.0, vec![]), vec![7]);
    // The backend lost the first command: no screenshot event arrives.
    assert!(frame(&mut app, &ctx, 1019.0, vec![]).is_empty());
    frame(&mut app, &ctx, 1019.5, vec![]);
    assert_eq!(frame(&mut app, &ctx, 1020.0, vec![]), vec![7]);
    assert!(rx.try_recv().is_err());
    // Either capture can arrive first. Both carry the original token.
    assert!(frame(&mut app, &ctx, 1030.0, vec![capture(7)]).is_empty());
    let reply = rx.try_recv().unwrap();
    assert_eq!(reply["ok"], true);
    assert_eq!(reply["result"]["width"], 2);
    assert_eq!(reply["result"]["height"], 3);
    assert!(frame(&mut app, &ctx, 1040.0, vec![capture(7)]).is_empty());
    assert!(rx.try_recv().is_err(), "a duplicate event must not send another reply");
    assert!(app.pending_screenshots.is_empty() && app.queued_screenshots.is_empty());
}

#[test]
fn a_successful_capture_cancels_its_unused_retry() {
    let (mut app, ctx, rx) = fixture();
    for now in [0.0, 10.0] {
        assert!(frame(&mut app, &ctx, now, vec![]).is_empty());
    }
    assert_eq!(frame(&mut app, &ctx, 20.0, vec![]), vec![7]);
    assert!(frame(&mut app, &ctx, 30.0, vec![capture(7)]).is_empty());
    assert_eq!(rx.try_recv().unwrap()["ok"], true);
    assert!(frame(&mut app, &ctx, 2000.0, vec![]).is_empty());
    assert!(app.queued_screenshots.is_empty());
}

#[test]
fn missing_frames_still_fail_at_the_original_deadline_without_a_capture_storm() {
    let (mut app, ctx, rx) = fixture();
    let mut tokens = vec![];
    for now in [0.0, 10.0, 20.0, 1000.0, 1010.0, 1020.0, 3000.0, 5000.0, 7999.0] {
        tokens.extend(frame(&mut app, &ctx, now, vec![capture(8)]));
    }
    assert_eq!(tokens, vec![7, 7], "at most two attempts for an unresolved request");
    assert!(rx.try_recv().is_err(), "unrelated screenshot events cannot complete this request");
    assert!(frame(&mut app, &ctx, 8000.0, vec![]).is_empty());
    let reply = rx.try_recv().unwrap();
    assert_eq!(reply["ok"], false);
    assert!(reply["error"].as_str().unwrap().contains("no frame was presented"));
    assert!(app.pending_screenshots.is_empty() && app.queued_screenshots.is_empty());
    assert!(frame(&mut app, &ctx, 9000.0, vec![capture(7)]).is_empty());
    assert!(rx.try_recv().is_err());
}
