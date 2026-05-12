use std::path::{Path, PathBuf};

use crate::error::Result;

// ── Trait ────────────────────────────────────────────────────────────────────

pub trait Platform: Send + Sync {
    /// Short identifier used in diagnostics ("macos", "linux", "windows").
    fn name(&self) -> &'static str;

    /// OS-appropriate default root when the caller does not specify --dir.
    /// Returns None only if the home directory cannot be determined.
    fn default_sandbox_dir(&self) -> Option<PathBuf>;

    /// Tighten permissions on `path` so only the current user can access it.
    /// On Unix this is chmod 0700; on Windows this is a no-op (NTFS ACLs
    /// already scope access to the creating account by default).
    fn secure_dir(&self, path: &Path) -> Result<()>;
}

// ── OS dispatch ──────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(unix)]
mod unix;

/// Return a platform object for the current OS.
pub fn current() -> Box<dyn Platform> {
    #[cfg(target_os = "macos")]
    return Box::new(macos::MacOS);
    #[cfg(target_os = "linux")]
    return Box::new(linux::Linux);
    #[cfg(target_os = "windows")]
    return Box::new(windows::Windows);
    // Fallback for other Unix-like systems.
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "linux")))]
    return Box::new(unix::GenericUnix);
}

// ── Tests (RED → GREEN) ──────────────────────────────────────────────────────
//
// These tests were written *before* the platform structs existed to document
// the contract each implementation must satisfy.

#[cfg(test)]
mod tests {
    use super::*;

    // Every platform must return a non-empty name.
    #[test]
    fn platform_name_is_nonempty() {
        assert!(!current().name().is_empty());
    }

    // The default sandbox directory must be resolvable.
    #[test]
    fn default_dir_is_some() {
        assert!(
            current().default_sandbox_dir().is_some(),
            "HOME/USERPROFILE must be set in the test environment"
        );
    }

    // The default directory must be an absolute path.
    #[test]
    fn default_dir_is_absolute() {
        let dir = current().default_sandbox_dir().unwrap();
        assert!(dir.is_absolute(), "default sandbox dir must be absolute: {}", dir.display());
    }

    // macOS: dir should be inside ~/Library/Application Support
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_default_dir_under_application_support() {
        let dir = current().default_sandbox_dir().unwrap();
        assert!(
            dir.to_string_lossy().contains("Library/Application Support"),
            "expected ~/Library/Application Support, got {}",
            dir.display()
        );
    }

    // Linux: dir should be inside ~/.local/share (XDG_DATA_HOME)
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_default_dir_under_local_share() {
        let dir = current().default_sandbox_dir().unwrap();
        assert!(
            dir.to_string_lossy().contains(".local/share"),
            "expected ~/.local/share, got {}",
            dir.display()
        );
    }

    // Windows: dir should be inside %APPDATA%
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_default_dir_under_appdata() {
        let dir = current().default_sandbox_dir().unwrap();
        let s = dir.to_string_lossy().to_lowercase();
        assert!(s.contains("appdata"), "expected %APPDATA%, got {}", dir.display());
    }

    // Unix: secure_dir must apply 0700 permissions.
    #[cfg(unix)]
    #[test]
    fn secure_dir_sets_0700() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        current().secure_dir(tmp.path()).unwrap();
        let mode = std::fs::metadata(tmp.path()).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o700,
            "expected 0700, got {:o}",
            mode & 0o777
        );
    }

    // secure_dir must not return an error on an existing directory.
    #[test]
    fn secure_dir_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        current().secure_dir(tmp.path()).unwrap();
        current().secure_dir(tmp.path()).unwrap(); // second call must not fail
    }
}
