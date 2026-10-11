//! Space-aware resize: in-place fallback when the filesystem can't hold old + new at once.
//!
//! The staged replace in [`super::alloc`] needs free space for a complete second file. When
//! that isn't available we resize the existing file instead, which needs only the growth
//! delta. The old swap is deactivated first (unavoidable), so every failure after that point
//! restores the original size, re-formats and re-activates it.

use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;

use crate::apply;
use crate::error::{Result, XzramError};

const MIB: u64 = 1024 * 1024;
/// Headroom left free on the filesystem so swap allocation can't fill it completely.
pub(crate) const SPACE_MARGIN_MB: u64 = 64;

/// How a resize will be carried out, decided from free space before anything is touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResizeStrategy {
    /// Build a complete new file beside the old one, then swap it in.
    Staged,
    /// Grow or shrink the existing file; needs only the growth delta.
    InPlace,
}

/// Pick a strategy from available bytes, or fail without touching anything.
pub(crate) fn choose_strategy(
    avail_bytes: u64,
    old_bytes: u64,
    new_mb: u64,
) -> Result<ResizeStrategy> {
    let new_bytes = new_mb.saturating_mul(MIB);
    let margin = SPACE_MARGIN_MB * MIB;
    if avail_bytes >= new_bytes.saturating_add(margin) {
        return Ok(ResizeStrategy::Staged);
    }
    let growth = new_bytes.saturating_sub(old_bytes);
    if avail_bytes >= growth.saturating_add(margin) {
        return Ok(ResizeStrategy::InPlace);
    }
    Err(XzramError::Validation(format!(
        "not enough free space to resize swapfile: need {} MiB more (plus {SPACE_MARGIN_MB} MiB \
         headroom), {} MiB available; the existing swap was left unchanged",
        growth.div_ceil(MIB),
        avail_bytes / MIB
    )))
}

/// Bytes available to root on the filesystem holding `dir`.
pub(crate) fn available_bytes(dir: &Path) -> Result<u64> {
    let c_path = std::ffi::CString::new(dir.as_os_str().as_bytes())
        .map_err(|_| XzramError::Validation("path contains NUL".into()))?;
    // SAFETY: zeroed statvfs is a valid out-parameter; c_path is NUL-terminated.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut st) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // Root may use reserved blocks (f_bfree), but stay conservative and use f_bavail.
    Ok((st.f_bavail as u64).saturating_mul(st.f_frsize as u64))
}

/// Resize the existing swapfile at `path` in place and re-activate it with `priority`.
pub(crate) fn resize_in_place(
    path: &Path,
    old_bytes: u64,
    new_mb: u64,
    priority: i32,
) -> Result<()> {
    let path_str = path.to_string_lossy().into_owned();
    let prio = priority.to_string();
    let file = open_nofollow(path)?;
    let new_bytes = new_mb
        .checked_mul(MIB)
        .ok_or_else(|| XzramError::Validation("swapfile size too large".into()))?;

    apply::deactivate_swap_path(&path_str)?;

    let attempt = set_len_allocated(&file, old_bytes, new_bytes)
        .and_then(|()| file.sync_all().map_err(Into::into))
        .and_then(|()| apply::run_command("mkswap", &[&path_str]).map(|_| ()))
        .and_then(|()| apply::run_command("swapon", &["-p", &prio, "--", &path_str]).map(|_| ()));

    if let Err(e) = attempt {
        let restored = restore_original(&file, &path_str, old_bytes, &prio);
        return Err(XzramError::Command(match restored {
            Ok(()) => format!(
                "{e}; restored the original {} MiB swapfile",
                old_bytes / MIB
            ),
            Err(r) => format!(
                "{e}; restoring the original swapfile also failed ({r}) — \
                 run `xzram swapfile resize {path_str} {}` once space is available",
                old_bytes / MIB
            ),
        }));
    }
    Ok(())
}

fn restore_original(file: &File, path_str: &str, old_bytes: u64, prio: &str) -> Result<()> {
    file.set_len(old_bytes)?;
    file.sync_all()?;
    apply::run_command("mkswap", &[path_str])?;
    apply::run_command("swapon", &["-p", prio, "--", path_str])?;
    Ok(())
}

fn open_nofollow(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(XzramError::Validation(format!(
            "{} is not a regular file",
            path.display()
        )));
    }
    Ok(file)
}

/// Shrink with truncate, or grow with real allocated blocks (swap can't have holes).
fn set_len_allocated(file: &File, old_bytes: u64, new_bytes: u64) -> Result<()> {
    if new_bytes <= old_bytes {
        file.set_len(new_bytes)?;
        return Ok(());
    }
    let len = libc::off_t::try_from(new_bytes)
        .map_err(|_| XzramError::Validation("swapfile size too large".into()))?;
    // SAFETY: valid open fd; fallocate does not touch user memory.
    let rc = unsafe { libc::fallocate(file.as_raw_fd(), 0, 0, len) };
    if rc == 0 {
        return Ok(());
    }
    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::EOPNOTSUPP) | Some(libc::ENOSYS) => zero_fill(file, old_bytes, new_bytes),
        _ => Err(err.into()),
    }
}

fn zero_fill(file: &File, from: u64, to: u64) -> Result<()> {
    let mut f = file;
    f.seek(SeekFrom::Start(from))?;
    let chunk = vec![0u8; MIB as usize];
    let mut left = to - from;
    while left > 0 {
        let n = left.min(MIB) as usize;
        f.write_all(&chunk[..n])?;
        left -= n as u64;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * MIB;

    #[test]
    fn staged_when_room_for_a_full_copy() {
        assert_eq!(
            choose_strategy(10 * GIB, 4 * GIB, 8 * 1024).unwrap(),
            ResizeStrategy::Staged
        );
    }

    #[test]
    fn in_place_when_only_the_delta_fits() {
        // 4 GiB -> 8 GiB with 5 GiB free: staged needs 8 GiB, in-place needs 4 GiB.
        assert_eq!(
            choose_strategy(5 * GIB, 4 * GIB, 8 * 1024).unwrap(),
            ResizeStrategy::InPlace
        );
        // Shrinking always fits in place.
        assert_eq!(
            choose_strategy(100 * MIB, 4 * GIB, 1024).unwrap(),
            ResizeStrategy::InPlace
        );
    }

    #[test]
    fn refuses_when_even_the_delta_does_not_fit() {
        let err = choose_strategy(GIB, 4 * GIB, 8 * 1024).unwrap_err();
        assert!(err.to_string().contains("left unchanged"), "{err}");
    }

    #[test]
    fn grow_and_shrink_keep_exact_length() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("swap");
        std::fs::write(&p, vec![1u8; MIB as usize]).unwrap();
        let f = open_nofollow(&p).unwrap();
        set_len_allocated(&f, MIB, 3 * MIB).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().len(), 3 * MIB);
        set_len_allocated(&f, 3 * MIB, 2 * MIB).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().len(), 2 * MIB);
        assert!(available_bytes(dir.path()).unwrap() > 0);
    }

    #[test]
    fn in_place_refuses_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("swap");
        std::fs::write(&p, b"x").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&p, &link).unwrap();
        assert!(open_nofollow(&link).is_err());
    }
}
