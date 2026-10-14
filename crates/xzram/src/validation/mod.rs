pub mod fstab;
mod path;
mod secure;
mod zram;

pub use path::*;
pub use secure::*;
pub use zram::*;

use crate::apply::PendingConfig;
use crate::error::Result;

pub fn validate_staged_pending(pending: &PendingConfig) -> Result<()> {
    if let Some(ref swapfile) = pending.swapfile {
        validate_swapfile_config(swapfile)?;
    }
    if let Some(ref path) = pending.remove_swapfile {
        validate_swapfile_remove_path(path)?;
    }
    if let Some(ref resize) = pending.swapfile_resize {
        validate_swapfile_resize_path(&resize.path, resize.size_mb)?;
    }
    if let Some(ref zram) = pending.zram {
        validate_zram_config(zram)?;
    }
    if let Some(ref sysctl) = pending.sysctl {
        crate::sysctl::validate_sysctl_values(sysctl)?;
    }
    if let Some(ref mm) = pending.mm {
        crate::mm_tunables::validate(mm)?;
    }
    for path in &pending.prepare_swapfiles {
        validate_swapfile_prepare_path(path)?;
    }
    Ok(())
}
