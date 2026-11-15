use crate::apply::{SwapfileConfig, ZramConfig};
use crate::error::{Result, XzramError};

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
    /// Persist "zram off" without touching running devices (takes effect at next boot).
    fn disable_at_boot(&self) -> Result<()>;
    fn apply(&self) -> Result<()>;
}

pub trait SwapfileBackendTrait: SwapBackend {
    fn list(&self) -> Result<Vec<SwapfileConfig>>;
    fn create(&self, config: &SwapfileConfig) -> Result<()>;
    fn remove(&self, path: &str) -> Result<()>;
    fn resize(&self, path: &str, size_mb: u64) -> Result<()>;
}

/// xzram manages zram only through systemd-zram-generator, whatever is installed now:
/// legacy zram-tools and hand-made devices are reported by `doctor`, not driven directly.
pub fn available_zram_backend() -> Result<Box<dyn ZramBackendTrait>> {
    Ok(Box::new(zram_generator::ZramGeneratorBackend))
}

pub fn available_swapfile_backend() -> Box<dyn SwapfileBackendTrait> {
    Box::new(swapfile::SwapfileBackend)
}

pub fn ensure_zram_backend() -> Result<Box<dyn ZramBackendTrait>> {
    let backend = available_zram_backend()?;
    if !backend.is_available() {
        return Err(XzramError::Backend(format!(
            "backend '{}' is not available on this system",
            backend.name()
        )));
    }
    Ok(backend)
}
