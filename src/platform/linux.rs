use std::path::{Path, PathBuf};

use crate::error::Result;
use super::{Platform, unix};

pub struct Linux;

impl Platform for Linux {
    fn name(&self) -> &'static str {
        "linux"
    }

    /// ~/.local/share/sandbox-rs  (XDG_DATA_HOME or its default)
    fn default_sandbox_dir(&self) -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("sandbox-rs"))
    }

    fn secure_dir(&self, path: &Path) -> Result<()> {
        unix::restrict_to_owner(path)
    }
}
