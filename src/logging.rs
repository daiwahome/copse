use std::fs;
use std::path::PathBuf;

use etcetera::BaseStrategy;
use log::LevelFilter;
use simplelog::{ConfigBuilder, WriteLogger};

use crate::config::LogLevel;

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

/// Initialize file-based logging.
///
/// Log level is determined by (in priority order):
/// 1. `COPSE_LOG` environment variable
/// 2. `log_level` field in config
/// 3. Default: `info`
///
/// Logs are written to `~/.local/state/copse/copse.log`.
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

    let log_path = dir.join("copse.log");
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
}
