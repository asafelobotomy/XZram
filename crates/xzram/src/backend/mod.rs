use crate::apply::{ApplyRequest, SwapfileConfig, ZramConfig};
use crate::error::Result;

pub mod swapfile;
pub mod zram_generator;

pub trait SwapBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn is_available(&self) -> bool;
}

pub trait ZramBackendTrait: SwapBackend {
    fn show(&self) -> Result<Option<ZramConfig>>;
    fn configure(&self, config: &ZramConfig) -> Result<()>;
    fn disable(&self) -> Result<()>;
    fn apply(&self) -> Result<()>;
}

pub trait SwapfileBackendTrait: SwapBackend {
    fn list(&self) -> Result<Vec<SwapfileConfig>>;
    fn create(&self, config: &SwapfileConfig) -> Result<()>;
    fn remove(&self, path: &str) -> Result<()>;
    fn resize(&self, path: &str, size_mb: u64) -> Result<()>;
}

pub fn available_zram_backend() -> Box<dyn ZramBackendTrait> {
    Box::new(zram_generator::ZramGeneratorBackend)
}

pub fn available_swapfile_backend() -> Box<dyn SwapfileBackendTrait> {
    Box::new(swapfile::SwapfileBackend)
}

pub fn build_apply_request(
    zram: Option<ZramConfig>,
    disable_zram: bool,
    swapfile: Option<SwapfileConfig>,
    remove_swapfile: Option<String>,
) -> ApplyRequest {
    ApplyRequest {
        zram,
        swapfile,
        disable_zram,
        remove_swapfile,
    }
}
