use std::path::{Path, PathBuf};

use crate::error::Result;
use super::{Platform, unix};

pub struct MacOS;

impl Platform for MacOS {
    fn name(&self) -> &'static str {
        "macos"
    }

    /// ~/Library/Application Support/sandbox-rs
    fn default_sandbox_dir(&self) -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("sandbox-rs"))
    }

    fn secure_dir(&self, path: &Path) -> Result<()> {
        unix::restrict_to_owner(path)
    }
}
