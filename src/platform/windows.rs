use std::path::{Path, PathBuf};

use crate::error::Result;
use super::Platform;

pub struct Windows;

impl Platform for Windows {
    fn name(&self) -> &'static str {
        "windows"
    }

    /// %APPDATA%\sandbox-rs  (Roaming AppData)
    fn default_sandbox_dir(&self) -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("sandbox-rs"))
    }

    /// On Windows the directory is created with NTFS ACLs scoped to the current
    /// user by default. A full ACL lockdown would require the `windows` crate;
    /// this no-op is intentional and documented.
    fn secure_dir(&self, _path: &Path) -> Result<()> {
        Ok(())
    }
}
