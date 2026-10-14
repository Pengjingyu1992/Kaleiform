//! Cooperative tracing scope. Helpers may stop early, but a partial result never escapes `run`.
use std::cell::RefCell;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use std::time::Duration;

use vectorcraft_pathops::{ComputationBudget, ComputationStop};

use crate::TraceError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Stage {
    Decode,
    Palette,
    Noise,
    Contours,
    Building,
}

#[derive(Clone)]
pub struct TraceControl {
    budget: ComputationBudget,
    stage: Arc<AtomicU8>,
}

thread_local! {
    static CURRENT: RefCell<Option<TraceControl>> = const { RefCell::new(None) };
}

impl Default for TraceControl {
    fn default() -> Self {
        Self::new(2_000_000_000, Duration::from_secs(120))
    }
}

impl TraceControl {
    pub fn new(work: u64, time: Duration) -> Self {
        Self { budget: ComputationBudget::new(work, time), stage: Arc::new(AtomicU8::new(Stage::Decode as u8)) }
    }
    pub fn cancel(&self) {
        self.budget.cancel();
    }
    pub fn check(&self, work: u64) -> Result<(), TraceError> {
        self.budget.check(work).map_err(|e| match e {
            ComputationStop::Cancelled => TraceError::Cancelled,
            ComputationStop::WorkLimit => TraceError::WorkLimit,
            ComputationStop::TimeLimit => TraceError::TimeLimit,
        })
    }
    pub fn stage(&self) -> Stage {
        match self.stage.load(Ordering::Relaxed) {
            0 => Stage::Decode,
            1 => Stage::Palette,
            2 => Stage::Noise,
            3 => Stage::Contours,
            _ => Stage::Building,
        }
    }
    pub fn set_stage(&self, stage: Stage) {
        self.stage.store(stage as u8, Ordering::Relaxed);
    }
    pub fn run<T>(&self, f: impl FnOnce() -> Result<T, TraceError>) -> Result<T, TraceError> {
        self.check(0)?;
        struct Restore(Option<TraceControl>);
        impl Drop for Restore {
            fn drop(&mut self) {
                CURRENT.with(|c| c.replace(self.0.take()));
            }
        }
        let _restore = Restore(CURRENT.with(|c| c.replace(Some(self.clone()))));
        // Also pass the allowance to curve fitting in pathops.
        let result = self.budget.run(f);
        self.check(0)?;
        result.map_err(|_| TraceError::WorkLimit)?
    }
}

pub(crate) fn current() -> Option<TraceControl> {
    CURRENT.with(|c| c.borrow().clone())
}
pub(crate) fn checkpoint(work: u64) -> bool {
    CURRENT.with(|c| c.borrow().as_ref().is_none_or(|c| c.check(work).is_ok()))
}

/// Charge batches rather than a shared atomic on every pixel.
pub(crate) fn pixel_checkpoint(index: usize) -> bool {
    !index.is_multiple_of(4096) || checkpoint(4096)
}
