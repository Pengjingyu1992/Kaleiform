#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use kurbo::{BezPath, Rect, Shape};
use std::time::{Duration, Instant};
use vectorcraft_geom::PathData;
use vectorcraft_pathops::{ComputationBudget, Join, PathOpsError, try_offset_path};

fn crossings(n: usize) -> PathData {
    let mut bp = BezPath::new();
    for i in 0..n {
        let x = i as f64;
        bp.move_to((x, 0.0));
        bp.line_to((n as f64 - x, 500.0));
    }
    PathData::from_bezpath(&bp)
}

#[test]
fn dense_crossings_stop_at_a_deterministic_work_limit() {
    let p = crossings(300);
    let budget = ComputationBudget::new(10_000, Duration::from_secs(5));
    assert_eq!(try_offset_path(&p, 2.0, Join::Round, 4.0, &budget), Err(PathOpsError::WorkLimit));
    // A stopped scope must not poison a later, ordinary operation on the same thread.
    let square = PathData::from_bezpath(&Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.01));
    let out = try_offset_path(&square, 2.0, Join::Miter, 4.0, &vectorcraft_pathops::offset_budget()).unwrap();
    assert_eq!(out.anchor_count(), 4);
    assert!((vectorcraft_pathops::area(&out, vectorcraft_geom::FillRule::NonZero) - 196.0).abs() < 1e-6);
}

#[test]
fn cancellation_stops_an_active_sweep_and_releases_the_worker() {
    let p = crossings(1200);
    let budget = ComputationBudget::new(100_000_000, Duration::from_secs(5));
    let worker_budget = budget.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        tx.send(try_offset_path(&p, 2.0, Join::Round, 4.0, &worker_budget)).unwrap();
    });
    let started = Instant::now();
    while budget.remaining_work() > 99_999_000 && started.elapsed() < Duration::from_secs(4) {
        std::thread::yield_now();
    }
    assert!(budget.remaining_work() <= 99_999_000, "never reached the sweep");
    budget.cancel();
    let result = rx.recv_timeout(Duration::from_secs(2)).expect("cancelled worker still computing");
    worker.join().unwrap();
    assert_eq!(result, Err(PathOpsError::Cancelled));
}

#[test]
fn an_expired_deadline_is_reported_without_geometry() {
    let p = crossings(2);
    let budget = ComputationBudget::new(100_000, Duration::ZERO);
    assert_eq!(try_offset_path(&p, 2.0, Join::Miter, 4.0, &budget), Err(PathOpsError::TimeLimit));
}
