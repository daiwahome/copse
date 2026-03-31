use std::path::Path;
use std::process::Stdio;
use std::sync::OnceLock;

use crate::config::{Agent, Config};

/// Result of `detect_waiting`: what the PTY screen currently indicates.
#[derive(Debug, PartialEq)]
pub enum WaitingDetection {
    /// Clear waiting signal: `❯` with cursor on its row, or "esc to cancel".
    Waiting,
    /// Clear running signal: spinner line or "esc to interrupt".
    Running,
    /// No recognisable indicator (e.g. completion list pushed ❯ out of scan range,
    /// or blank area between renders). Caller should maintain the current state.
    Unknown,
}

impl Agent {
    /// Check that the agent's external dependency is available.
    pub fn validate(&self) -> anyhow::Result<()> {
        match self {
            Agent::ClaudeCode => {
                if is_claude_available() {
                    Ok(())
                } else {
                    anyhow::bail!(
                        "agent = \"claudecode\" is configured but claude is not installed or not in PATH"
                    )
                }
            }
        }
    }

    /// Returns the CLI binary name for this agent.
    pub fn command_name(&self) -> &'static str {
        match self {
            Agent::ClaudeCode => "claude",
        }
    }

    /// Build CLI arguments for launching the agent.
    pub fn command_args(&self, has_session: bool, config: &Config) -> Vec<String> {
        match self {
            Agent::ClaudeCode => {
                let mut args = Vec::new();
                if has_session {
                    args.push("--continue".to_string());
                }
                args.push("--permission-mode".to_string());
                args.push(config.claude_code.permission_mode.clone());
                args
            }
        }
    }

    /// Write agent-specific configuration files into the worktree.
    pub fn setup_worktree(&self, worktree_path: &Path, config: &Config) -> anyhow::Result<()> {
        match self {
            Agent::ClaudeCode => setup_claude_code_worktree(
                worktree_path,
                config.auto_commit,
                config.auto_permissions,
            ),
        }
    }

    /// Pure pattern-matching logic for detecting whether the agent is waiting for input.
    /// Inspects the last few non-empty PTY lines (text + whether the cursor is on that row).
    ///
    /// Returns:
    /// - `Running`  -- spinner or "esc to interrupt" found
    /// - `Waiting`  -- "esc to cancel" or `❯` with cursor found (and no running signal)
    /// - `Unknown`  -- neither signal found; caller should keep current state unchanged
    pub fn detect_waiting(&self, lines: &[(String, bool)]) -> WaitingDetection {
        match self {
            Agent::ClaudeCode => detect_waiting_claude_code(lines),
        }
    }
}

// -- Private Claude Code helpers --

fn is_claude_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        std::process::Command::new("claude")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}

fn setup_claude_code_worktree(
    worktree_path: &Path,
    auto_commit: bool,
    auto_permissions: bool,
) -> anyhow::Result<()> {
    if !auto_commit && !auto_permissions {
        return Ok(());
    }

    let claude_dir = worktree_path.join(".claude");
    std::fs::create_dir_all(&claude_dir)?;

    let settings_path = claude_dir.join("settings.local.json");

    let template: serde_json::Value =
        serde_json::from_str(include_str!("templates/settings.local.json"))?;

    let mut settings = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)?;
        serde_json::from_str::<serde_json::Value>(&content)
            .unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Merge template keys into existing settings (existing keys take priority)
    if let (Some(target), Some(source)) = (settings.as_object_mut(), template.as_object()) {
        if auto_commit {
            if let Some(hooks) = source.get("hooks") {
                target.entry("hooks").or_insert_with(|| hooks.clone());
            }
        }
        if auto_permissions {
            if let Some(permissions) = source.get("permissions") {
                target
                    .entry("permissions")
                    .or_insert_with(|| permissions.clone());
            }
        }
    }

    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings)? + "\n",
    )?;

    Ok(())
}

/// Returns true if `line` looks like Claude Code's processing spinner.
///
/// Claude Code renders spinner frames as a Unicode dingbat/symbol character
/// followed by a verb ending in "ing...", e.g. `✢ Simmering...` or `✽ Thinking...`.
/// These characters fall in the Miscellaneous Symbols (U+2600-U+26FF) and
/// Dingbats (U+2700-U+27FF) Unicode blocks.
fn is_spinner_line(line: &str) -> bool {
    let mut chars = line.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let in_symbol_block = ('\u{2600}'..='\u{27FF}').contains(&first);
    if !in_symbol_block {
        return false;
    }
    let rest = chars.as_str().trim();
    rest.ends_with("ing\u{2026}") || rest.ends_with("ing...")
}

/// The cursor check prevents echoed input (`❯ test` on a row the cursor has
/// left) from being mis-detected as a waiting prompt after Enter is pressed.
/// `Unknown` handles the case where a completion list (after typing `/`) pushes
/// the `❯` row out of the scan window: we neither accept nor reject waiting.
fn detect_waiting_claude_code(lines: &[(String, bool)]) -> WaitingDetection {
    let mut has_waiting_indicator = false;
    for (line, has_cursor) in lines {
        let lower = line.to_lowercase();
        if lower.contains("esc to interrupt") || lower.contains("ctrl+c to interrupt") {
            return WaitingDetection::Running;
        }
        if is_spinner_line(line) {
            return WaitingDetection::Running;
        }
        if !has_waiting_indicator
            && (lower.contains("esc to cancel") || (line.starts_with('❯') && *has_cursor))
        {
            has_waiting_indicator = true;
        }
    }
    if has_waiting_indicator {
        WaitingDetection::Waiting
    } else {
        WaitingDetection::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claudecode_command_name() {
        assert_eq!(Agent::ClaudeCode.command_name(), "claude");
    }

    #[test]
    fn claudecode_command_args_fresh() {
        let config = Config::default();
        let args = Agent::ClaudeCode.command_args(false, &config);
        assert_eq!(args, vec!["--permission-mode", "default"]);
    }

    #[test]
    fn claudecode_command_args_continue() {
        let mut config = Config::default();
        config.claude_code.permission_mode = "plan".to_string();
        let args = Agent::ClaudeCode.command_args(true, &config);
        assert_eq!(args, vec!["--continue", "--permission-mode", "plan"]);
    }

    /// Helper: build lines where none have the cursor (for busy/non-prompt tests).
    fn nc(strs: &[&str]) -> Vec<(String, bool)> {
        strs.iter().map(|s| (s.to_string(), false)).collect()
    }

    /// Helper: build lines where the first entry has the cursor.
    fn with_cursor(strs: &[&str]) -> Vec<(String, bool)> {
        strs.iter()
            .enumerate()
            .map(|(i, s)| (s.to_string(), i == 0))
            .collect()
    }

    #[test]
    fn busy_when_esc_to_interrupt() {
        let l = nc(&["Reading file...", "esc to interrupt"]);
        assert_eq!(
            Agent::ClaudeCode.detect_waiting(&l),
            WaitingDetection::Running
        );
    }

    #[test]
    fn busy_when_ctrl_c_to_interrupt() {
        let l = nc(&["Working...", "ctrl+c to interrupt"]);
        assert_eq!(
            Agent::ClaudeCode.detect_waiting(&l),
            WaitingDetection::Running
        );
    }

    #[test]
    fn busy_takes_priority_over_prompt() {
        let l = with_cursor(&["❯", "esc to interrupt"]);
        assert_eq!(
            Agent::ClaudeCode.detect_waiting(&l),
            WaitingDetection::Running
        );
    }

    #[test]
    fn busy_when_spinner() {
        let l = with_cursor(&["❯", "✢ Simmering\u{2026}"]);
        assert_eq!(
            Agent::ClaudeCode.detect_waiting(&l),
            WaitingDetection::Running
        );
    }

    #[test]
    fn busy_when_spinner_various_chars() {
        let l = nc(&["✽ Thinking\u{2026}"]);
        assert_eq!(
            Agent::ClaudeCode.detect_waiting(&l),
            WaitingDetection::Running
        );
    }

    #[test]
    fn waiting_when_esc_to_cancel() {
        let l = nc(&["Do you want to proceed?", "esc to cancel"]);
        assert_eq!(
            Agent::ClaudeCode.detect_waiting(&l),
            WaitingDetection::Waiting
        );
    }

    #[test]
    fn waiting_when_prompt() {
        let l = with_cursor(&["❯"]);
        assert_eq!(
            Agent::ClaudeCode.detect_waiting(&l),
            WaitingDetection::Waiting
        );
    }

    #[test]
    fn waiting_when_typing_at_prompt() {
        let l = with_cursor(&["❯ test"]);
        assert_eq!(
            Agent::ClaudeCode.detect_waiting(&l),
            WaitingDetection::Waiting
        );
    }

    #[test]
    fn unknown_when_prompt_without_cursor() {
        let l = nc(&["❯ test"]);
        assert_eq!(
            Agent::ClaudeCode.detect_waiting(&l),
            WaitingDetection::Unknown
        );
    }

    #[test]
    fn unknown_when_empty() {
        let l: Vec<(String, bool)> = vec![];
        assert_eq!(
            Agent::ClaudeCode.detect_waiting(&l),
            WaitingDetection::Unknown
        );
    }

    #[test]
    fn unknown_without_indicators() {
        let l = nc(&["some output", "more output"]);
        assert_eq!(
            Agent::ClaudeCode.detect_waiting(&l),
            WaitingDetection::Unknown
        );
    }

    #[test]
    fn unknown_slash_completion_list() {
        let l = nc(&["/review", "/chat", "/help", "/commit"]);
        assert_eq!(
            Agent::ClaudeCode.detect_waiting(&l),
            WaitingDetection::Unknown
        );
    }

    #[test]
    fn gt_no_longer_triggers_waiting() {
        let l = nc(&["> quoted text in output"]);
        assert_eq!(
            Agent::ClaudeCode.detect_waiting(&l),
            WaitingDetection::Unknown
        );
    }
}
