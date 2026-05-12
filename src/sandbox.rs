use std::path::{Path, PathBuf};

use crate::{
    config::Config,
    error::{Result, SandboxError},
    platform::{self, Platform},
};

pub struct Sandbox {
    pub root: PathBuf,
    pub config: Config,
    /// Canonicalized readonly roots used for read-path validation.
    readonly_roots: Vec<PathBuf>,
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

        log::debug!("sandbox root: {}", root.display());
        log::debug!("responses dir: {}", responses.display());

        let readonly_roots = canonicalize_readonly(&config.readonly_dirs);
        Ok(Self { root, config, readonly_roots })
    }

    /// Extend the in-memory readonly roots for this session without persisting.
    /// To persist, update `self.config.readonly_dirs` and call `self.config.save`.
    pub fn add_readonly_dirs(&mut self, dirs: &[PathBuf]) {
        for dir in dirs {
            if let Ok(canonical) = dir.canonicalize() {
                if !self.readonly_roots.contains(&canonical) {
                    log::debug!("readonly dir added: {}", canonical.display());
                    self.readonly_roots.push(canonical);
                }
            }
            if !self.config.readonly_dirs.contains(dir) {
                self.config.readonly_dirs.push(dir.clone());
            }
        }
    }

    // ── Write-safe path ───────────────────────────────────────────────────────

    /// Resolve `rel` relative to the sandbox root, rejecting any path that
    /// escapes via `..` or symlinks. For writes only.
    pub fn safe_path(&self, rel: &str) -> Result<PathBuf> {
        let normalized = normalize_path(&self.root.join(rel));
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

    // ── Read-safe path ────────────────────────────────────────────────────────

    /// Resolve `path` for a read operation.
    ///
    /// Relative paths are joined to the sandbox root. Absolute paths are
    /// accepted if they resolve inside the sandbox root **or** inside one of
    /// the configured readonly directories. Writes to the result path are
    /// never implicitly permitted.
    pub fn safe_read_path(&self, path: &str) -> Result<PathBuf> {
        let raw = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.root.join(path)
        };
        let normalized = normalize_path(&raw);
        if self.is_within_allowed_read(&normalized) {
            Ok(normalized)
        } else {
            Err(SandboxError::ReadRestricted(path.to_string()))
        }
    }

    fn is_within_allowed_read(&self, normalized: &Path) -> bool {
        if normalized.starts_with(&self.root) {
            return true;
        }
        self.readonly_roots.iter().any(|ro| normalized.starts_with(ro))
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

/// Canonicalize a list of readonly dirs, silently dropping any that don't exist.
fn canonicalize_readonly(dirs: &[PathBuf]) -> Vec<PathBuf> {
    dirs.iter().filter_map(|d| d.canonicalize().ok()).collect()
}

// ── Tests (RED written before implementation) ─────────────────────────────────

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

    fn make_sandbox() -> (tempfile::TempDir, Sandbox) {
        let tmp = tempfile::tempdir().unwrap();
        let sb = Sandbox::init_with(tmp.path(), &NullPlatform).unwrap();
        (tmp, sb)
    }

    // ── normalize_path ────────────────────────────────────────────────────────

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

    // ── safe_path (write) ─────────────────────────────────────────────────────

    #[test]
    fn safe_path_rejects_escape() {
        let (_tmp, sb) = make_sandbox();
        assert!(sb.safe_path("../../etc/passwd").is_err());
    }

    #[test]
    fn safe_path_accepts_nested() {
        let (_tmp, sb) = make_sandbox();
        let p = sb.safe_path("responses/foo.json").unwrap();
        assert!(p.starts_with(&sb.root));
    }

    // ── safe_read_path ────────────────────────────────────────────────────────

    #[test]
    fn read_path_accepts_relative_within_sandbox() {
        let (_tmp, sb) = make_sandbox();
        let p = sb.safe_read_path("responses/out.json").unwrap();
        assert!(p.starts_with(&sb.root));
    }

    #[test]
    fn read_path_rejects_absolute_outside_sandbox_without_readonly() {
        let (_tmp, sb) = make_sandbox();
        // /etc is outside the sandbox and no readonly dirs are configured.
        let result = sb.safe_read_path("/etc/hosts");
        assert!(
            matches!(result, Err(SandboxError::ReadRestricted(_))),
            "expected ReadRestricted, got {result:?}"
        );
    }

    #[test]
    fn read_path_accepts_absolute_within_readonly_dir() {
        let ro_dir = tempfile::tempdir().unwrap();
        let ro_file = ro_dir.path().join("data.json");
        std::fs::write(&ro_file, "{}").unwrap();

        let (_tmp, mut sb) = make_sandbox();
        sb.add_readonly_dirs(&[ro_dir.path().to_path_buf()]);

        let result = sb.safe_read_path(ro_file.to_str().unwrap());
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[test]
    fn read_path_rejects_dotdot_escape_from_readonly_dir() {
        let ro_dir = tempfile::tempdir().unwrap();
        let (_tmp, mut sb) = make_sandbox();
        sb.add_readonly_dirs(&[ro_dir.path().to_path_buf()]);

        // Attempt to traverse above the readonly dir.
        let escape = format!("{}/../../../etc/passwd", ro_dir.path().display());
        let result = sb.safe_read_path(&escape);
        assert!(
            matches!(result, Err(SandboxError::ReadRestricted(_))),
            "expected ReadRestricted, got {result:?}"
        );
    }

    #[test]
    fn add_readonly_dirs_is_idempotent() {
        let ro_dir = tempfile::tempdir().unwrap();
        let (_tmp, mut sb) = make_sandbox();
        sb.add_readonly_dirs(&[ro_dir.path().to_path_buf()]);
        let count_before = sb.readonly_roots.len();
        sb.add_readonly_dirs(&[ro_dir.path().to_path_buf()]);
        assert_eq!(sb.readonly_roots.len(), count_before);
    }

    #[test]
    fn add_readonly_dirs_updates_config() {
        let ro_dir = tempfile::tempdir().unwrap();
        let (_tmp, mut sb) = make_sandbox();
        sb.add_readonly_dirs(&[ro_dir.path().to_path_buf()]);
        assert!(sb.config.readonly_dirs.contains(&ro_dir.path().to_path_buf()));
    }
}
