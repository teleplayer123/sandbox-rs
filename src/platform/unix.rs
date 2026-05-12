use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::error::Result;

/// Set directory permissions to 0700 (owner read/write/execute only).
pub(super) fn restrict_to_owner(path: &Path) -> Result<()> {
    let perms = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

/// Fallback for Unix systems that are neither macOS nor Linux.
#[allow(dead_code)]
pub struct GenericUnix;

#[allow(dead_code)]
impl super::Platform for GenericUnix {
    fn name(&self) -> &'static str {
        "unix"
    }

    fn default_sandbox_dir(&self) -> Option<std::path::PathBuf> {
        dirs::data_dir().map(|d| d.join("sandbox-rs"))
    }

    fn secure_dir(&self, path: &Path) -> Result<()> {
        restrict_to_owner(path)
    }
}
