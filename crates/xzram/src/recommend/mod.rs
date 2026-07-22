mod engine;
mod overflow;
mod profile;
mod staging;
mod types;

pub use engine::{recommend, stage_recommended};
pub use overflow::{build_overflow_swapfile, decide_overflow_swapfile, overflow_size_mb};
pub use staging::eval_zram_size_mb;
pub use types::{
    OverflowDecision, RecommendProfile, RecommendationItem, RecommendedDefaults, SystemContext,
    OVERFLOW_FREE_SPACE_MARGIN_MB, OVERFLOW_SWAPFILE_MAX_MB, OVERFLOW_SWAPFILE_PATH,
    OVERFLOW_SWAP_PRIORITY,
};
