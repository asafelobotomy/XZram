mod engine;
mod engine_build;
mod engine_notes;
mod link;
mod link_sanitize;
mod link_types;
mod overflow;
mod profile;
pub mod scales;
mod staging;
mod types;

pub use engine::{
    recommend, recommend_with_scales, stage_recommended, stage_recommended_with_scales,
};
pub use link::{linked_optimize_context, optimize_linked};
pub use link_types::{LinkedAnchor, LinkedOptimizeContext, LinkedOptimizeResult};
pub use overflow::{build_overflow_swapfile, decide_overflow_swapfile, overflow_size_mb};
pub use scales::{RecommendScales, RecommendSizeScale, RecommendSizeScales};
pub use staging::eval_zram_size_mb;
pub use types::{
    OverflowDecision, RecommendProfile, RecommendationItem, RecommendedDefaults, SystemContext,
    OVERFLOW_FREE_SPACE_MARGIN_MB, OVERFLOW_SWAPFILE_HIGH_MAX_MB, OVERFLOW_SWAPFILE_MAX_MB,
    OVERFLOW_SWAPFILE_PATH, OVERFLOW_SWAP_PRIORITY,
};
