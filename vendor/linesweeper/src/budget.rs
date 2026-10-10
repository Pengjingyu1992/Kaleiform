//! Opt-in cooperative limits for callers that must be able to stop a sweep.
use std::cell::RefCell;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};
use std::time::Duration;

/// Why a computation stopped. Partial results must be discarded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The caller cancelled it.
    Cancelled,
    /// Its deterministic work allowance was consumed.
    WorkLimit,
    /// Its wall-clock allowance was consumed on a native target.
    TimeLimit,
}
struct State {
    cancelled: AtomicBool,
    work: AtomicU64,
    stop: AtomicU8,
    #[cfg(not(target_arch = "wasm32"))]
    deadline: std::time::Instant,
}
/// A single computation's shared allowance and cancellation handle.
#[derive(Clone)]
pub struct Budget(Arc<State>);
thread_local! {
    static CURRENT: RefCell<Option<Budget>> = const { RefCell::new(None) };
}
impl Budget {
    /// Start an allowance. Work units count subdivisions, events and scanned segments.
    pub fn new(work: u64, time: Duration) -> Self {
        #[cfg(target_arch = "wasm32")]
        let _ = time;
        Self(Arc::new(State {
            cancelled: AtomicBool::new(false),
            work: AtomicU64::new(work),
            stop: AtomicU8::new(0),
            #[cfg(not(target_arch = "wasm32"))]
            deadline: std::time::Instant::now().checked_add(time).unwrap_or_else(std::time::Instant::now),
        }))
    }
    /// Request cancellation from another thread.
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Relaxed);
    }
    /// Remaining deterministic work (also useful for measuring a stress case).
    pub fn remaining_work(&self) -> u64 {
        self.0.work.load(Ordering::Relaxed)
    }
    /// Check the allowance, consuming `work` units.
    pub fn check(&self, work: u64) -> Result<(), Stop> {
        let reason = if self.0.cancelled.load(Ordering::Relaxed) {
            1
        } else if !self.take_work(work) {
            2
        } else {
            #[cfg(not(target_arch = "wasm32"))]
            if std::time::Instant::now() >= self.0.deadline {
                self.0.stop.store(3, Ordering::Relaxed);
            }
            0
        };
        if reason != 0 {
            self.0.stop.store(reason, Ordering::Relaxed);
        }
        match self.0.stop.load(Ordering::Relaxed) {
            0 => Ok(()),
            1 => Err(Stop::Cancelled),
            2 => Err(Stop::WorkLimit),
            _ => Err(Stop::TimeLimit),
        }
    }
    fn take_work(&self, work: u64) -> bool {
        let mut remaining = self.0.work.load(Ordering::Relaxed);
        loop {
            let Some(next) = remaining.checked_sub(work) else { return false };
            match self.0.work.compare_exchange_weak(remaining, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return true,
                Err(actual) => remaining = actual,
            }
        }
    }
    /// Run with this thread's allowance; a stopped partial result never escapes.
    pub fn run<T>(&self, f: impl FnOnce() -> T) -> Result<T, Stop> {
        self.check(0)?;
        struct Restore(Option<Budget>);
        impl Drop for Restore {
            fn drop(&mut self) {
                CURRENT.with(|c| c.replace(self.0.take()));
            }
        }
        let _restore = Restore(CURRENT.with(|c| c.replace(Some(self.clone()))));
        let out = f();
        self.check(0)?;
        Ok(out)
    }
}
/// Check the current scope. Unscoped upstream calls have no limit.
pub fn checkpoint(work: u64) -> bool {
    CURRENT.with(|c| c.borrow().as_ref().is_none_or(|b| b.check(work).is_ok()))
}
