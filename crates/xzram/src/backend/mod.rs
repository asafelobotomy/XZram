use crate::apply::{SwapfileConfig, ZramConfig};
use crate::detect::{detect, ZramBackend};
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
    fn apply(&self) -> Result<()>;
}

pub trait SwapfileBackendTrait: SwapBackend {
    fn list(&self) -> Result<Vec<SwapfileConfig>>;
    fn create(&self, config: &SwapfileConfig) -> Result<()>;
    fn remove(&self, path: &str) -> Result<()>;
    fn resize(&self, path: &str, size_mb: u64) -> Result<()>;
}

pub fn available_zram_backend() -> Result<Box<dyn ZramBackendTrait>> {
    let detection = detect()?;
    match detection.zram_backend {
        ZramBackend::SystemdZramGenerator | ZramBackend::None | ZramBackend::Manual => {
            Ok(Box::new(zram_generator::ZramGeneratorBackend))
        }
        ZramBackend::ZramTools => Ok(Box::new(zram_generator::ZramGeneratorBackend)),
    }
}

pub fn zram_backend_warning() -> Result<Option<String>> {
    let detection = detect()?;
    match detection.zram_backend {
        ZramBackend::ZramTools => Ok(Some(
            "Legacy zram-tools detected; consider 'xzram zram migrate' to move to zram-generator"
                .into(),
        )),
        ZramBackend::Manual => Ok(Some(
            "Manual zram configuration detected; xzram will manage via systemd-zram-generator"
                .into(),
        )),
        _ => Ok(None),
    }
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
