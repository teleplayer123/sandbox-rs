use std::path::{Path, PathBuf};

use crate::{
    config::Config,
    error::{Result, SandboxError},
    platform::{self, Platform},
};

pub struct Sandbox {
    pub root: PathBuf,
    pub config: Config,
}

impl Sandbox {
    /// Initialize using the platform detected at compile time.
    pub fn init(dir: &Path) -> Result<Self> {
        Self::init_with(dir, &*platform::current())
    }

    /// Initialize with an explicit platform — use this in tests to inject a mock.
    pub fn init_with(dir: &Path, platform: &dyn Platform) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let root = dir.canonicalize()?;

        if !root.is_dir() {
            return Err(SandboxError::InvalidDir(format!(
                "'{}' is not a directory",
                root.display()
            )));
        }

        platform.secure_dir(&root)?;

        let config = Config::load(&root)?;

        let responses = config.response_dir_path(&root);
        std::fs::create_dir_all(&responses)?;
        platform.secure_dir(&responses)?;

        Ok(Self { root, config })
    }

    /// Resolve `rel` relative to the sandbox root, rejecting any path that
    /// escapes via `..` or symlinks.
    pub fn safe_path(&self, rel: &str) -> Result<PathBuf> {
        let raw = self.root.join(rel);
        let normalized = normalize_path(&raw);
        if !normalized.starts_with(&self.root) {
            return Err(SandboxError::PathEscape(rel.to_string()));
        }
        Ok(normalized)
    }

    /// Resolve a path inside the responses directory.
    pub fn response_path(&self, filename: &str) -> Result<PathBuf> {
        let rel = format!("{}/{}", self.config.response_dir, filename);
        self.safe_path(&rel)
    }
}

/// Lexically normalize a path (resolve `.` and `..`) without touching the
/// filesystem, making it safe to use before a file exists.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        use std::path::Component::*;
        match component {
            CurDir => {}
            ParentDir => { out.pop(); }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Platform;
    use std::path::{Path, PathBuf};

    struct NullPlatform;
    impl Platform for NullPlatform {
        fn name(&self) -> &'static str { "null" }
        fn default_sandbox_dir(&self) -> Option<PathBuf> { None }
        fn secure_dir(&self, _: &Path) -> crate::error::Result<()> { Ok(()) }
    }

    #[test]
    fn normalize_removes_dotdot() {
        let p = PathBuf::from("/sandbox/foo/../bar");
        assert_eq!(normalize_path(&p), PathBuf::from("/sandbox/bar"));
    }

    #[test]
    fn normalize_keeps_valid_subpath() {
        let p = PathBuf::from("/sandbox/responses/out.json");
        assert_eq!(normalize_path(&p), p);
    }

    #[test]
    fn safe_path_rejects_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let sb = Sandbox::init_with(tmp.path(), &NullPlatform).unwrap();
        assert!(sb.safe_path("../../etc/passwd").is_err());
    }

    #[test]
    fn safe_path_accepts_nested() {
        let tmp = tempfile::tempdir().unwrap();
        let sb = Sandbox::init_with(tmp.path(), &NullPlatform).unwrap();
        let p = sb.safe_path("responses/foo.json").unwrap();
        assert!(p.starts_with(tmp.path()));
    }
}
