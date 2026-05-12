use std::path::Path;

use crate::{
    config::LogConfig,
    error::{Result, SandboxError},
    sandbox::normalize_path,
};

/// Initialize the global file logger from `config` and the sandbox root.
///
/// - When `config.enabled` is false this is a complete no-op; no file is
///   created and the global logger is not touched.
/// - The log file path is validated to stay within the sandbox root so a
///   crafted `sandbox.toml` cannot write logs outside the jail.
/// - If the global logger is already set (e.g., two commands in the same
///   process, or a test harness), the second call is silently ignored.
pub fn init(config: &LogConfig, sandbox_root: &Path) -> Result<()> {
    if !config.enabled {
        return Ok(());
    }

    let log_path = resolve_log_path(config, sandbox_root)?;

    let dispatch = fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {:<5} {}] {}",
                chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(config.level_filter())
        .chain(fern::log_file(&log_path).map_err(SandboxError::Io)?);

    // Ignore SetLoggerError — logger already registered; treat as no-op.
    let _ = dispatch.apply();
    Ok(())
}

/// Resolve and validate the log file path within the sandbox root.
fn resolve_log_path(config: &LogConfig, sandbox_root: &Path) -> Result<std::path::PathBuf> {
    let raw = sandbox_root.join(&config.file);
    let normalized = normalize_path(&raw);
    if !normalized.starts_with(sandbox_root) {
        return Err(SandboxError::PathEscape(config.file.clone()));
    }
    Ok(normalized)
}

// ── Tests (RED written before implementation) ─────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LogConfig;

    // ── LogConfig data ────────────────────────────────────────────────────────

    #[test]
    fn default_log_config_is_enabled() {
        let cfg = LogConfig::default();
        assert!(cfg.enabled);
    }

    #[test]
    fn default_log_level_is_info() {
        let cfg = LogConfig::default();
        assert_eq!(cfg.level_filter(), log::LevelFilter::Info);
    }

    #[test]
    fn log_level_parses_all_variants() {
        for (s, expected) in [
            ("error", log::LevelFilter::Error),
            ("warn",  log::LevelFilter::Warn),
            ("info",  log::LevelFilter::Info),
            ("debug", log::LevelFilter::Debug),
            ("trace", log::LevelFilter::Trace),
        ] {
            let cfg = LogConfig { level: s.to_string(), ..LogConfig::default() };
            assert_eq!(cfg.level_filter(), expected, "failed for level '{s}'");
        }
    }

    #[test]
    fn unknown_log_level_falls_back_to_info() {
        let cfg = LogConfig { level: "nonsense".to_string(), ..LogConfig::default() };
        assert_eq!(cfg.level_filter(), log::LevelFilter::Info);
    }

    #[test]
    fn log_config_toml_round_trip() {
        let original = LogConfig::default();
        let serialized = toml::to_string(&original).unwrap();
        let decoded: LogConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(decoded.enabled, original.enabled);
        assert_eq!(decoded.file,    original.file);
        assert_eq!(decoded.level,   original.level);
    }

    // ── init() behaviour ─────────────────────────────────────────────────────

    #[test]
    fn init_disabled_does_not_create_file() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = LogConfig { enabled: false, ..LogConfig::default() };
        init(&cfg, tmp.path()).unwrap();
        assert!(!tmp.path().join(&cfg.file).exists());
    }

    #[test]
    fn init_disabled_returns_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = LogConfig { enabled: false, ..LogConfig::default() };
        // Must not error even when called multiple times.
        assert!(init(&cfg, tmp.path()).is_ok());
        assert!(init(&cfg, tmp.path()).is_ok());
    }

    // ── Path safety ───────────────────────────────────────────────────────────

    #[test]
    fn path_escape_in_log_file_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = LogConfig {
            enabled: true,
            file: "../../escape.log".to_string(),
            ..LogConfig::default()
        };
        let result = resolve_log_path(&cfg, tmp.path());
        assert!(
            matches!(result, Err(SandboxError::PathEscape(_))),
            "expected PathEscape, got {result:?}"
        );
    }

    #[test]
    fn valid_log_path_resolves_inside_sandbox() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = LogConfig::default();
        let path = resolve_log_path(&cfg, tmp.path()).unwrap();
        assert!(path.starts_with(tmp.path()));
    }
}
