use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use crate::config::DiffFilter;
use crate::process::{self, CommandLogExt};

impl DiffFilter {
    /// Check that the diff filter's external dependency is available.
    pub fn validate(&self) -> anyhow::Result<()> {
        match self {
            DiffFilter::None => Ok(()),
            DiffFilter::Delta => {
                if is_delta_available() {
                    Ok(())
                } else {
                    anyhow::bail!(
                        "diff_filter = \"delta\" is configured but delta is not installed or not in PATH"
                    )
                }
            }
        }
    }

    /// Colorize raw diff text using the configured filter.
    /// Returns `None` if no colorization is configured or if the tool is unavailable.
    pub fn colorize(&self, raw_diff: &str) -> Option<String> {
        match self {
            DiffFilter::None => None,
            DiffFilter::Delta => colorize_with_delta(raw_diff),
        }
    }
}

// -- Private delta helpers (moved from diff.rs) --

/// Check if delta is available in PATH (cached after first call).
fn is_delta_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("delta")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .run_status()
            .is_ok_and(|s| s.success())
    })
}

/// Run raw diff text through `delta --color-only` for syntax highlighting.
/// Returns None if delta is not installed or fails.
fn colorize_with_delta(raw_diff: &str) -> Option<String> {
    if !is_delta_available() {
        return None;
    }

    // `--dark` suppresses delta's OSC 11 terminal background-color query.
    // Without it, delta writes `ESC]11;?ST` directly to /dev/tty and reads
    // back `ESC]11;rgb:…ST`, which copse (in raw mode) sees as keyboard input.
    //
    // Explicit styles are required because delta's defaults use "normal auto"
    // for minus (= terminal default fg, which is white) and "auto" backgrounds
    // that don't render well in a ratatui TUI. We want:
    //   - removed lines: red foreground (no background)
    //   - added lines:   green foreground (no background)
    //   - context lines: terminal default (no background)
    // File/hunk headers are colored by copse's own fallback renderer, so we
    // suppress delta's styling for those with "normal".
    let mut child = Command::new("delta")
        .args([
            "--no-gitconfig",
            "--color-only",
            "--dark",
            "--minus-style",
            "red",
            "--plus-style",
            "green",
            "--zero-style",
            "normal",
            "--file-style",
            "normal",
            "--hunk-header-style",
            "normal",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .run_spawn()
        .ok()?;

    let mut stdin = child.stdin.take().unwrap();
    let output = std::thread::scope(|s| {
        s.spawn(|| {
            let _ = stdin.write_all(raw_diff.as_bytes());
            drop(stdin);
        });
        child.wait_with_output()
    });

    let output = output.ok()?;
    process::log_exit_status("delta", output.status);
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_validate_always_ok() {
        assert!(DiffFilter::None.validate().is_ok());
    }

    #[test]
    fn none_colorize_returns_none() {
        assert!(DiffFilter::None.colorize("some diff").is_none());
    }
}
