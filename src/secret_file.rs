//! Atomic save / delete of secret-bearing files under a 0700 directory.
//!
//! Shared by [`crate::oauth::tokens`] and [`crate::api_key`]. On Unix files are
//! created with mode 0600 (atomically, via `OpenOptions::mode`) and the parent
//! directory is forced to 0700. Atomicity: write to a per-PID `<file>.<pid>.tmp`,
//! then `rename`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Atomically write `contents` to `path` with mode 0600, ensuring the parent
/// directory exists at mode 0700.
pub fn save(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir_0700(parent)?;
    }
    let tmp = path.with_extension(format!(
        "{}.{}.tmp",
        path.extension().and_then(|s| s.to_str()).unwrap_or(""),
        std::process::id()
    ));
    let cleanup = TmpFileGuard(tmp.clone());
    write_secret_file(&tmp, contents)?;
    fs::rename(&tmp, path)?;
    std::mem::forget(cleanup); // file moved, no longer needs deletion
    Ok(())
}

/// Remove a secret file. NotFound is success — `delete` is idempotent.
pub fn delete(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::Io(e)),
    }
}

/// Drop guard that deletes the temp file if `save` returns early before the
/// atomic rename. Prevents stale `.tmp` files after crashes / power loss.
struct TmpFileGuard(PathBuf);

impl Drop for TmpFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Create-and-write so the file's permissions are 0o600 from inception on
/// Unix — no window where the secret bytes are on disk with default umask
/// perms (0o644). On non-unix the std API doesn't expose a creation-time
/// mode, so we fall back to plain write.
fn write_secret_file(path: &Path, contents: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(contents)?;
        f.sync_all()?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let mut f = fs::File::create(path)?;
        f.write_all(contents)?;
        f.sync_all()?;
        Ok(())
    }
}

/// `create_dir_all` honors umask; for secret directories we want 0o700
/// regardless. No-op on non-unix.
fn ensure_dir_0700(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = fs::metadata(dir)?.permissions();
        if perm.mode() & 0o777 != 0o700 {
            perm.set_mode(0o700);
            fs::set_permissions(dir, perm)?;
        }
    }
    Ok(())
}
