use std::fs;
use std::path::{Path, PathBuf};

use etcetera::BaseStrategy;
use log::LevelFilter;
use simplelog::{ConfigBuilder, WriteLogger};
use time::OffsetDateTime;

use crate::config::LogLevel;

const MAX_LOG_FILES: usize = 10;

fn log_dir() -> anyhow::Result<PathBuf> {
    let strategy = etcetera::base_strategy::Xdg::new()
        .map_err(|e| anyhow::anyhow!("Failed to determine state directory: {e}"))?;
    let state_dir = strategy.state_dir().unwrap_or_else(|| strategy.data_dir());
    Ok(state_dir.join("copse"))
}

fn to_level_filter(level: &LogLevel) -> LevelFilter {
    match level {
        LogLevel::Trace => LevelFilter::Trace,
        LogLevel::Debug => LevelFilter::Debug,
        LogLevel::Info => LevelFilter::Info,
        LogLevel::Warn => LevelFilter::Warn,
        LogLevel::Error => LevelFilter::Error,
        LogLevel::Off => LevelFilter::Off,
    }
}

fn parse_env_level(s: &str) -> Option<LevelFilter> {
    match s.to_lowercase().as_str() {
        "trace" => Some(LevelFilter::Trace),
        "debug" => Some(LevelFilter::Debug),
        "info" => Some(LevelFilter::Info),
        "warn" => Some(LevelFilter::Warn),
        "error" => Some(LevelFilter::Error),
        "off" => Some(LevelFilter::Off),
        _ => None,
    }
}

fn generate_log_filename() -> String {
    let now = OffsetDateTime::now_utc();
    format!(
        "copse-{:04}-{:02}-{:02}T{:02}-{:02}-{:02}.log",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    )
}

fn cleanup_old_logs(dir: &Path, max_files: usize) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    let mut log_files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "log")
                && path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.starts_with("copse-"))
        })
        .collect();

    log_files.sort();

    if log_files.len() > max_files {
        let to_remove = log_files.len() - max_files;
        for path in &log_files[..to_remove] {
            let _ = fs::remove_file(path);
        }
    }
}

/// Initialize file-based logging.
///
/// Log level is determined by (in priority order):
/// 1. `COPSE_LOG` environment variable
/// 2. `log_level` field in config
/// 3. Default: `info`
///
/// Logs are written to `~/.local/state/copse/copse-<TIMESTAMP>.log`
/// where `<TIMESTAMP>` is a UTC timestamp in `YYYY-MM-DDTHH-MM-SS` format.
/// On startup, old log files are cleaned up, keeping the most recent 10.
pub fn init(config_level: &LogLevel) -> anyhow::Result<()> {
    let level = match std::env::var("COPSE_LOG") {
        Ok(v) => parse_env_level(&v).ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid COPSE_LOG value \"{v}\". Valid values: trace, debug, info, warn, error, off"
            )
        })?,
        Err(_) => to_level_filter(config_level),
    };

    if level == LevelFilter::Off {
        return Ok(());
    }

    let dir = log_dir()?;
    fs::create_dir_all(&dir)
        .map_err(|e| anyhow::anyhow!("Failed to create log directory {}: {e}", dir.display()))?;

    // Reserve one slot for the new log file about to be created.
    cleanup_old_logs(&dir, MAX_LOG_FILES - 1);

    let log_path = dir.join(generate_log_filename());
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| anyhow::anyhow!("Failed to open log file {}: {e}", log_path.display()))?;

    let config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .set_target_level(LevelFilter::Off)
        .build();

    WriteLogger::init(level, config, file)
        .map_err(|e| anyhow::anyhow!("Failed to initialize logger: {e}"))?;

    log::info!("Logging initialized (level: {level})");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_level_filter_all_variants() {
        assert_eq!(to_level_filter(&LogLevel::Trace), LevelFilter::Trace);
        assert_eq!(to_level_filter(&LogLevel::Debug), LevelFilter::Debug);
        assert_eq!(to_level_filter(&LogLevel::Info), LevelFilter::Info);
        assert_eq!(to_level_filter(&LogLevel::Warn), LevelFilter::Warn);
        assert_eq!(to_level_filter(&LogLevel::Error), LevelFilter::Error);
        assert_eq!(to_level_filter(&LogLevel::Off), LevelFilter::Off);
    }

    #[test]
    fn parse_env_level_valid() {
        assert_eq!(parse_env_level("trace"), Some(LevelFilter::Trace));
        assert_eq!(parse_env_level("DEBUG"), Some(LevelFilter::Debug));
        assert_eq!(parse_env_level("Info"), Some(LevelFilter::Info));
        assert_eq!(parse_env_level("WARN"), Some(LevelFilter::Warn));
        assert_eq!(parse_env_level("error"), Some(LevelFilter::Error));
        assert_eq!(parse_env_level("off"), Some(LevelFilter::Off));
    }

    #[test]
    fn parse_env_level_invalid() {
        assert_eq!(parse_env_level("verbose"), None);
        assert_eq!(parse_env_level(""), None);
        assert_eq!(parse_env_level("42"), None);
    }

    #[test]
    fn log_dir_ends_with_copse() {
        let dir = log_dir().unwrap();
        assert_eq!(dir.file_name().unwrap(), "copse");
    }

    #[test]
    fn generate_log_filename_format() {
        let name = generate_log_filename();
        assert!(name.starts_with("copse-"));
        assert!(name.ends_with(".log"));
        assert_eq!(name.len(), "copse-2026-03-31T10-30-00.log".len());
        let timestamp = &name["copse-".len()..name.len() - ".log".len()];
        assert!(timestamp
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == 'T'));
    }

    #[test]
    fn cleanup_old_logs_keeps_max_files() {
        let dir = tempfile::tempdir().unwrap();
        let names = [
            "copse-2026-01-01T00-00-00.log",
            "copse-2026-01-02T00-00-00.log",
            "copse-2026-01-03T00-00-00.log",
            "copse-2026-01-04T00-00-00.log",
            "copse-2026-01-05T00-00-00.log",
            "copse-2026-01-06T00-00-00.log",
            "copse-2026-01-07T00-00-00.log",
        ];
        for name in &names {
            fs::File::create(dir.path().join(name)).unwrap();
        }
        fs::File::create(dir.path().join("other.log")).unwrap();

        cleanup_old_logs(dir.path(), 3);

        let mut remaining: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        remaining.sort();

        assert_eq!(remaining.len(), 4);
        assert!(remaining.contains(&"copse-2026-01-05T00-00-00.log".to_string()));
        assert!(remaining.contains(&"copse-2026-01-06T00-00-00.log".to_string()));
        assert!(remaining.contains(&"copse-2026-01-07T00-00-00.log".to_string()));
        assert!(remaining.contains(&"other.log".to_string()));
    }

    #[test]
    fn cleanup_old_logs_no_op_when_under_limit() {
        let dir = tempfile::tempdir().unwrap();
        let names = [
            "copse-2026-01-01T00-00-00.log",
            "copse-2026-01-02T00-00-00.log",
        ];
        for name in &names {
            fs::File::create(dir.path().join(name)).unwrap();
        }

        cleanup_old_logs(dir.path(), 5);

        let count = fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(count, 2);
    }

    #[test]
    fn cleanup_old_logs_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        cleanup_old_logs(dir.path(), 5);
    }

    #[test]
    fn cleanup_old_logs_nonexistent_dir() {
        cleanup_old_logs(Path::new("/tmp/nonexistent-copse-test-dir"), 5);
    }
}
