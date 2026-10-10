//! Kaleiform path operations: booleans (Pathfinder), shape builder regions, offset, outline stroke,
//! simplify and the other Object → Path commands.
//!
//! Booleans are curve-preserving: `linesweeper`'s robust sweep-line works directly on cubic
//! Béziers, and the pieces it splits curves into are refitted afterwards so results carry few
//! anchors. See the crate README for the API overview.
#![forbid(unsafe_code)]

mod boolean;
mod edit;
mod fit;
mod offset;
mod pathfinder;
mod planar;

pub use boolean::{BoolOp, DEFAULT_PRECISION, area, boolean, boolean_n, normalize, try_boolean, try_normalize, unite_all};
pub use edit::{
    AverageAxis, SimplifyOptions, add_anchor_points, average, join, remove_anchor, remove_redundant_points, simplify, simplify_with, smooth,
    split_into_grid,
};
pub use linesweeper::budget::{Budget as ComputationBudget, Stop as ComputationStop};
pub use offset::{Cap, Join, offset_path, outline_stroke, stroke_region, try_offset_path};
pub use pathfinder::{FaceMerger, PathfinderOp, Region, Shape, merge_regions, pathfinder, region_at, regions};
pub use planar::{BuilderArrangement, SHAPE_BUILDER_MAX_SEGMENTS, cut_out, encloses_area, interior_point, live_paint, shape_builder};

/// Errors from fallible operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PathOpsError {
    #[error("input contains NaN or infinite coordinates")]
    NonFinite,
    #[error("input path could not be closed")]
    OpenPath,
    #[error("the shapes are too degenerate to combine")]
    Degenerate,
    #[error("path computation cancelled")]
    Cancelled,
    #[error("path computation exceeded its work limit")]
    WorkLimit,
    #[error("path computation exceeded its time limit")]
    TimeLimit,
}

impl From<ComputationStop> for PathOpsError {
    fn from(stop: ComputationStop) -> Self {
        match stop {
            ComputationStop::Cancelled => Self::Cancelled,
            ComputationStop::WorkLimit => Self::WorkLimit,
            ComputationStop::TimeLimit => Self::TimeLimit,
        }
    }
}

/// Per-command allowance, shared across every selected path. Precision is unchanged.
pub fn offset_budget() -> ComputationBudget {
    ComputationBudget::new(1_000_000, std::time::Duration::from_secs(10))
}
