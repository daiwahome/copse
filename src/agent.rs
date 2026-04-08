use std::path::Path;
use std::process::Stdio;
use std::sync::OnceLock;

use crate::config::{Agent, Config};

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
            Agent::Codex => {
                if is_codex_available() {
                    Ok(())
                } else {
                    anyhow::bail!(
                        "agent = \"codex\" is configured but codex is not installed or not in PATH"
                    )
                }
            }
        }
    }

    /// Returns the CLI binary name for this agent.
    pub fn command_name(&self) -> &'static str {
        match self {
            Agent::ClaudeCode => "claude",
            Agent::Codex => "codex",
        }
    }

    /// Returns a human-readable name for UI display.
    pub fn display_name(&self) -> &'static str {
        match self {
            Agent::ClaudeCode => "claude",
            Agent::Codex => "codex",
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
                if config.claude_code.auto_mode {
                    args.push("--enable-auto-mode".to_string());
                }
                args
            }
            Agent::Codex => {
                let mut args = Vec::new();
                if has_session {
                    args.push("resume".to_string());
                    args.push("--last".to_string());
                }
                if let Some(ref sandbox) = config.codex.sandbox {
                    args.push("--sandbox".to_string());
                    args.push(sandbox.clone());
                }
                if let Some(ref approval) = config.codex.approval {
                    args.push("--ask-for-approval".to_string());
                    args.push(approval.clone());
                }
                if config.codex.search {
                    args.push("--search".to_string());
                }
                args
            }
        }
    }

    /// Write agent-specific configuration files into the worktree.
    pub fn setup_worktree(
        &self,
        worktree_path: &Path,
        repo_root: &Path,
        config: &Config,
    ) -> anyhow::Result<()> {
        match self {
            Agent::ClaudeCode => setup_claude_code_worktree(
                worktree_path,
                repo_root,
                config.auto_commit,
                config.auto_permissions,
                config.notification_command.as_deref(),
            ),
            Agent::Codex => setup_codex_worktree(
                worktree_path,
                repo_root,
                config.auto_commit,
                config.notification_command.as_deref(),
            ),
        }
    }

    /// Detect whether the agent is actively processing by examining the PTY screen.
    /// Returns `true` if the agent appears busy (not waiting for user input).
    pub fn is_processing(&self, screen: &vt100::Screen) -> bool {
        match self {
            Agent::ClaudeCode => has_active_spinner(screen),
            Agent::Codex => has_working_text(screen),
        }
    }
}

// -- Private helpers: binary availability --

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

fn is_codex_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        std::process::Command::new("codex")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}

// -- Private helpers: worktree setup --

fn setup_claude_code_worktree(
    worktree_path: &Path,
    repo_root: &Path,
    auto_commit: bool,
    auto_permissions: bool,
    notification_command: Option<&str>,
) -> anyhow::Result<()> {
    let parent_settings_path = repo_root.join(".claude").join("settings.local.json");
    let has_parent = parent_settings_path.exists();

    if !auto_commit && !auto_permissions && notification_command.is_none() && !has_parent {
        return Ok(());
    }

    let claude_dir = worktree_path.join(".claude");
    std::fs::create_dir_all(&claude_dir)?;

    let settings_path = claude_dir.join("settings.local.json");

    // Layer 1 (lowest priority): parent repository settings
    let mut settings = if has_parent {
        let content = std::fs::read_to_string(&parent_settings_path)?;
        serde_json::from_str::<serde_json::Value>(&content)
            .unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Layer 2: copse template (controlled by auto_commit / auto_permissions flags)
    let template: serde_json::Value =
        serde_json::from_str(include_str!("templates/settings.local.json"))?;
    if auto_commit {
        if let Some(hooks) = template.get("hooks") {
            merge_settings(
                &mut settings,
                &serde_json::json!({ "hooks": hooks.clone() }),
            );
        }
    }
    if auto_permissions {
        if let Some(permissions) = template.get("permissions") {
            let mut permissions = permissions.clone();
            // Restrict Edit / Write / NotebookEdit to worktree path
            if let Some(allow) = permissions.get_mut("allow").and_then(|v| v.as_array_mut()) {
                let worktree_prefix = worktree_path.to_string_lossy();
                for entry in allow.iter_mut() {
                    if let Some(s) = entry.as_str() {
                        let restricted = match s {
                            "Edit" => Some(format!("Edit({worktree_prefix}/**)")),
                            "Write" => Some(format!("Write({worktree_prefix}/**)")),
                            "NotebookEdit" => Some(format!("NotebookEdit({worktree_prefix}/**)")),
                            _ => None,
                        };
                        if let Some(r) = restricted {
                            *entry = serde_json::Value::String(r);
                        }
                    }
                }
            }
            merge_settings(
                &mut settings,
                &serde_json::json!({ "permissions": permissions }),
            );
            // Deny access to sensitive files outside the worktree.
            // Merged separately so that parent deny rules are preserved.
            if let Ok(home) = etcetera::home_dir() {
                let h = home.to_string_lossy();
                let deny = sensitive_path_deny_rules(&h);
                merge_settings(
                    &mut settings,
                    &serde_json::json!({ "permissions": { "deny": deny } }),
                );
            }
        }
    }
    if let Some(cmd) = notification_command {
        let notification_hook = serde_json::json!({
            "hooks": {
                "Notification": [{
                    "matcher": "",
                    "hooks": [{
                        "type": "command",
                        "command": cmd
                    }]
                }]
            }
        });
        merge_settings(&mut settings, &notification_hook);
    }

    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings)? + "\n",
    )?;

    Ok(())
}

/// Returns deny rules for sensitive paths that should not be accessible to the agent.
fn sensitive_path_deny_rules(home: &str) -> Vec<String> {
    vec![
        // Credentials & keys
        format!("Read({home}/.ssh/**)"),
        format!("Read({home}/.gnupg/**)"),
        format!("Read({home}/.aws/**)"),
        format!("Read({home}/.config/gcloud/**)"),
        format!("Read({home}/.azure/**)"),
        // Secrets files
        "Read(**/.env)".to_string(),
        "Read(**/.env.*)".to_string(),
        "Read(**/*.pem)".to_string(),
        "Read(**/*.key)".to_string(),
        // Shell history
        format!("Read({home}/.bash_history)"),
        format!("Read({home}/.zsh_history)"),
        // Auth tokens & configs
        format!("Read({home}/.netrc)"),
        format!("Read({home}/.docker/config.json)"),
        format!("Read({home}/.kube/config)"),
        format!("Read({home}/.npmrc)"),
    ]
}

/// Deep-merge `overlay` into `base`.
/// - Objects: recurse, overlay keys win for scalars.
/// - Arrays: concatenate and deduplicate (by value equality).
/// - Scalars: overlay replaces base.
/// - Mismatched types (e.g. Object vs Array): overlay replaces base entirely.
fn merge_settings(base: &mut serde_json::Value, overlay: &serde_json::Value) {
    match (base.as_object_mut(), overlay.as_object()) {
        (Some(base_map), Some(overlay_map)) => {
            for (key, overlay_val) in overlay_map {
                match base_map.get_mut(key) {
                    Some(base_val) => merge_settings(base_val, overlay_val),
                    None => {
                        base_map.insert(key.clone(), overlay_val.clone());
                    }
                }
            }
        }
        _ => match (base.as_array_mut(), overlay.as_array()) {
            (Some(base_arr), Some(overlay_arr)) => {
                for item in overlay_arr {
                    if !base_arr.contains(item) {
                        base_arr.push(item.clone());
                    }
                }
            }
            _ => {
                *base = overlay.clone();
            }
        },
    }
}

fn setup_codex_worktree(
    worktree_path: &Path,
    repo_root: &Path,
    auto_commit: bool,
    notification_command: Option<&str>,
) -> anyhow::Result<()> {
    let parent_hooks_path = repo_root.join(".codex").join("hooks.json");
    let parent_config_path = repo_root.join(".codex").join("config.toml");
    let has_parent_hooks = parent_hooks_path.exists();
    let has_parent_config = parent_config_path.exists();

    if !auto_commit && notification_command.is_none() && !has_parent_hooks && !has_parent_config {
        return Ok(());
    }

    let codex_dir = worktree_path.join(".codex");
    std::fs::create_dir_all(&codex_dir)?;

    // -- hooks.json --
    if auto_commit || has_parent_hooks {
        let mut hooks = if has_parent_hooks {
            let content = std::fs::read_to_string(&parent_hooks_path)?;
            serde_json::from_str::<serde_json::Value>(&content)
                .unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        if auto_commit {
            let stop_hook = serde_json::json!({
                "hooks": {
                    "Stop": [{
                        "matcher": "",
                        "hooks": [{
                            "type": "command",
                            "command": "bash -c 'git add -A && git diff --cached --quiet || git commit -m \"copse auto-commit\"'"
                        }]
                    }]
                }
            });
            merge_settings(&mut hooks, &stop_hook);
        }

        std::fs::write(
            codex_dir.join("hooks.json"),
            serde_json::to_string_pretty(&hooks)? + "\n",
        )?;
    }

    // -- config.toml --
    // Parse the parent config as a TOML table and merge copse settings
    // structurally to avoid duplicate section keys (e.g. [features]).
    let needs_config = auto_commit || notification_command.is_some();
    if needs_config || has_parent_config {
        let mut table: toml::Table = if has_parent_config {
            let content = std::fs::read_to_string(&parent_config_path)?;
            content.parse().unwrap_or_default()
        } else {
            toml::Table::new()
        };

        if auto_commit {
            let features = table
                .entry("features")
                .or_insert_with(|| toml::Value::Table(toml::Table::new()))
                .as_table_mut();
            if let Some(features) = features {
                features.insert("codex_hooks".to_string(), toml::Value::Boolean(true));
            }
        }

        if let Some(cmd) = notification_command {
            let notify_array = toml::Value::Array(vec![
                toml::Value::String("bash".to_string()),
                toml::Value::String("-c".to_string()),
                toml::Value::String(cmd.to_string()),
            ]);
            table.insert("notify".to_string(), notify_array);
        }

        let config_path = codex_dir.join("config.toml");
        std::fs::write(&config_path, toml::to_string_pretty(&table)?)?;
    }

    Ok(())
}

// -- Private helpers: PTY screen detection --

/// Scan non-empty rows of the PTY screen bottom-to-top (up to 50 rows).
/// Calls `test_row` with the row index and trimmed text for each non-empty row.
/// Returns `true` as soon as `test_row` returns `true`.
fn scan_screen_rows(screen: &vt100::Screen, mut test_row: impl FnMut(u16, &str) -> bool) -> bool {
    let rows = screen.size().0 as usize;
    let cols = screen.size().1 as usize;
    let mut scanned = 0;
    for r in (0..rows).rev() {
        let row_text: String = (0..cols)
            .filter_map(|c| {
                screen
                    .cell(r as u16, c as u16)
                    .map(|cell| cell.contents().to_string())
            })
            .collect();
        let trimmed = row_text.trim();
        if trimmed.is_empty() {
            continue;
        }
        if test_row(r as u16, trimmed) {
            return true;
        }
        scanned += 1;
        if scanned >= 50 {
            break;
        }
    }
    false
}

/// Claude Code spinner characters used for animation frames.
const SPINNER_CHARS: &[char] = &['✢', '✳', '✶', '✻', '✽', '·'];

/// Scan the PTY screen for a colored spinner character, indicating active processing.
///
/// Returns `true` when a spinner character with a non-default foreground color
/// is found in the visible rows (scanned bottom-to-top, up to 50 non-empty rows).
fn has_active_spinner(screen: &vt100::Screen) -> bool {
    let cols = screen.size().1 as usize;
    scan_screen_rows(screen, |r, trimmed| {
        let Some(first_char) = trimmed.chars().next() else {
            return false;
        };
        if !SPINNER_CHARS.contains(&first_char) {
            return false;
        }
        for c in 0..cols {
            if let Some(cell) = screen.cell(r, c as u16) {
                if let Some(ch) = cell.contents().chars().next() {
                    if SPINNER_CHARS.contains(&ch) {
                        return !matches!(cell.fgcolor(), vt100::Color::Default);
                    }
                }
            }
        }
        false
    })
}

/// Detect whether the Codex CLI is actively processing.
///
/// Codex displays `• Working (Xs • esc to interrupt)` while processing.
/// The text pattern is specific enough to avoid most false positives.
/// No-color terminals are supported — color is not required.
///
/// Scans bottom-to-top, up to 50 non-empty rows.
fn has_working_text(screen: &vt100::Screen) -> bool {
    scan_screen_rows(screen, |_r, trimmed| {
        trimmed.contains("Working (") && trimmed.contains("esc to interrupt")
    })
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

    #[test]
    fn claudecode_command_args_auto_mode() {
        let mut config = Config::default();
        config.claude_code.auto_mode = true;
        let args = Agent::ClaudeCode.command_args(false, &config);
        assert_eq!(
            args,
            vec!["--permission-mode", "default", "--enable-auto-mode"]
        );
    }

    #[test]
    fn claudecode_command_args_continue_auto_mode() {
        let mut config = Config::default();
        config.claude_code.auto_mode = true;
        let args = Agent::ClaudeCode.command_args(true, &config);
        assert_eq!(
            args,
            vec![
                "--continue",
                "--permission-mode",
                "default",
                "--enable-auto-mode"
            ]
        );
    }

    // -- merge_settings tests --

    #[test]
    fn merge_scalars_overlay_wins() {
        let mut base = serde_json::json!({"key": "base"});
        merge_settings(&mut base, &serde_json::json!({"key": "overlay"}));
        assert_eq!(base["key"], "overlay");
    }

    #[test]
    fn merge_objects_recursively() {
        let mut base = serde_json::json!({"a": {"x": 1}, "b": 2});
        merge_settings(&mut base, &serde_json::json!({"a": {"y": 2}, "c": 3}));
        assert_eq!(
            base,
            serde_json::json!({"a": {"x": 1, "y": 2}, "b": 2, "c": 3})
        );
    }

    #[test]
    fn merge_arrays_deduplicates() {
        let mut base = serde_json::json!(["a", "b"]);
        merge_settings(&mut base, &serde_json::json!(["b", "c"]));
        assert_eq!(base, serde_json::json!(["a", "b", "c"]));
    }

    #[test]
    fn merge_nested_arrays_in_objects() {
        let mut base = serde_json::json!({"permissions": {"allow": ["Bash(git *)", "Edit"]}});
        let overlay = serde_json::json!({"permissions": {"allow": ["Edit", "Bash(cargo *)"]}});
        merge_settings(&mut base, &overlay);
        assert_eq!(
            base,
            serde_json::json!({"permissions": {"allow": ["Bash(git *)", "Edit", "Bash(cargo *)"]}})
        );
    }

    #[test]
    fn merge_hooks_concatenates_arrays() {
        let mut base = serde_json::json!({
            "hooks": {"Stop": [{"type": "command", "command": "echo parent"}]}
        });
        let overlay = serde_json::json!({
            "hooks": {"Stop": [{"type": "command", "command": "echo copse"}]}
        });
        merge_settings(&mut base, &overlay);
        let stop = base["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
    }

    #[test]
    fn merge_adds_missing_keys() {
        let mut base = serde_json::json!({});
        merge_settings(
            &mut base,
            &serde_json::json!({"permissions": {"allow": ["Edit"]}}),
        );
        assert_eq!(
            base,
            serde_json::json!({"permissions": {"allow": ["Edit"]}})
        );
    }

    // -- setup_claude_code_worktree tests --

    struct WorktreeFixture {
        _tmp: tempfile::TempDir,
        repo: std::path::PathBuf,
        wt: std::path::PathBuf,
    }

    impl WorktreeFixture {
        fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let repo = tmp.path().join("repo");
            let wt = tmp.path().join("wt");
            std::fs::create_dir_all(&repo).unwrap();
            std::fs::create_dir_all(&wt).unwrap();
            Self {
                _tmp: tmp,
                repo,
                wt,
            }
        }

        fn set_parent_settings(&self, value: &serde_json::Value) {
            let path = self.repo.join(".claude/settings.local.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, serde_json::to_string_pretty(value).unwrap()).unwrap();
        }

        fn read_worktree_settings(&self) -> serde_json::Value {
            let content =
                std::fs::read_to_string(self.wt.join(".claude/settings.local.json")).unwrap();
            serde_json::from_str(&content).unwrap()
        }
    }

    #[test]
    fn inherits_parent_settings() {
        let f = WorktreeFixture::new();
        f.set_parent_settings(&serde_json::json!({
            "permissions": {"allow": ["Bash(cargo *)"]}
        }));

        setup_claude_code_worktree(&f.wt, &f.repo, false, false, None).unwrap();

        let result = f.read_worktree_settings();
        assert!(result["permissions"]["allow"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("Bash(cargo *)")));
    }

    #[test]
    fn template_merges_on_top_of_parent() {
        let f = WorktreeFixture::new();
        f.set_parent_settings(&serde_json::json!({
            "permissions": {"allow": ["Bash(cargo *)"]}
        }));

        setup_claude_code_worktree(&f.wt, &f.repo, false, true, None).unwrap();

        let allow = f.read_worktree_settings()["permissions"]["allow"]
            .as_array()
            .unwrap()
            .clone();
        let wt_str = f.wt.to_string_lossy().to_string();
        assert!(allow.contains(&serde_json::json!("Bash(cargo *)")));
        assert!(allow.contains(&serde_json::json!(format!("Edit({wt_str}/**)"))));
        assert!(allow.contains(&serde_json::json!(format!("Write({wt_str}/**)"))));
        assert!(allow.contains(&serde_json::json!(format!("NotebookEdit({wt_str}/**)"))));
        // Unrestricted versions should NOT be present
        assert!(!allow.contains(&serde_json::json!("Edit")));
        assert!(!allow.contains(&serde_json::json!("Write")));
        assert!(!allow.contains(&serde_json::json!("NotebookEdit")));
    }

    #[test]
    fn auto_permissions_adds_deny_rules() {
        let f = WorktreeFixture::new();

        setup_claude_code_worktree(&f.wt, &f.repo, false, true, None).unwrap();

        let deny = f.read_worktree_settings()["permissions"]["deny"]
            .as_array()
            .unwrap()
            .clone();
        let home = etcetera::home_dir().unwrap();
        let h = home.to_string_lossy();
        assert!(deny.contains(&serde_json::json!(format!("Read({h}/.ssh/**)"))));
        assert!(deny.contains(&serde_json::json!(format!("Read({h}/.aws/**)"))));
        assert!(deny.contains(&serde_json::json!("Read(**/.env)")));
        assert!(deny.contains(&serde_json::json!("Read(**/.env.*)")));
        assert!(deny.contains(&serde_json::json!("Read(**/*.pem)")));
        assert!(deny.contains(&serde_json::json!(format!("Read({h}/.kube/config)"))));
    }

    #[test]
    fn deny_rules_merge_with_parent_deny() {
        let f = WorktreeFixture::new();
        f.set_parent_settings(&serde_json::json!({
            "permissions": {"deny": ["Bash(rm *)"]}
        }));

        setup_claude_code_worktree(&f.wt, &f.repo, false, true, None).unwrap();

        let deny = f.read_worktree_settings()["permissions"]["deny"]
            .as_array()
            .unwrap()
            .clone();
        // Parent deny rule is preserved
        assert!(deny.contains(&serde_json::json!("Bash(rm *)")));
        // Sensitive path deny rules are also present
        let home = etcetera::home_dir().unwrap();
        let h = home.to_string_lossy();
        assert!(deny.contains(&serde_json::json!(format!("Read({h}/.ssh/**)"))));
    }

    #[test]
    fn no_parent_no_flags_does_nothing() {
        let f = WorktreeFixture::new();

        setup_claude_code_worktree(&f.wt, &f.repo, false, false, None).unwrap();

        assert!(!f.wt.join(".claude/settings.local.json").exists());
    }

    #[test]
    fn no_parent_with_flags_uses_template() {
        let f = WorktreeFixture::new();

        setup_claude_code_worktree(&f.wt, &f.repo, true, true, None).unwrap();

        let result = f.read_worktree_settings();
        assert!(result.get("hooks").is_some());
        assert!(result.get("permissions").is_some());
    }

    #[test]
    fn notification_command_adds_hook() {
        let f = WorktreeFixture::new();

        setup_claude_code_worktree(&f.wt, &f.repo, false, false, Some("printf '\\a'")).unwrap();

        let result = f.read_worktree_settings();
        let hooks = result["hooks"]["Notification"].as_array().unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0]["hooks"][0]["command"], "printf '\\a'");
    }

    // -- Codex command tests --

    #[test]
    fn codex_command_name() {
        assert_eq!(Agent::Codex.command_name(), "codex");
    }

    #[test]
    fn codex_display_name() {
        assert_eq!(Agent::Codex.display_name(), "codex");
    }

    #[test]
    fn codex_command_args_fresh_defaults() {
        let config = Config::default();
        let args = Agent::Codex.command_args(false, &config);
        // Default: search = true, sandbox/approval = None
        assert_eq!(args, vec!["--search"]);
    }

    #[test]
    fn codex_command_args_with_sandbox() {
        let mut config = Config::default();
        config.codex.sandbox = Some("workspace-write".to_string());
        config.codex.search = false;
        let args = Agent::Codex.command_args(false, &config);
        assert_eq!(args, vec!["--sandbox", "workspace-write"]);
    }

    #[test]
    fn codex_command_args_with_both() {
        let mut config = Config::default();
        config.codex.sandbox = Some("workspace-write".to_string());
        config.codex.approval = Some("on-request".to_string());
        config.codex.search = false;
        let args = Agent::Codex.command_args(false, &config);
        assert_eq!(
            args,
            vec![
                "--sandbox",
                "workspace-write",
                "--ask-for-approval",
                "on-request"
            ]
        );
    }

    #[test]
    fn codex_command_args_resume() {
        let mut config = Config::default();
        config.codex.sandbox = Some("workspace-write".to_string());
        let args = Agent::Codex.command_args(true, &config);
        assert_eq!(&args[0..2], &["resume", "--last"]);
        assert!(args.contains(&"--sandbox".to_string()));
    }

    #[test]
    fn codex_command_args_search_disabled() {
        let mut config = Config::default();
        config.codex.search = false;
        let args = Agent::Codex.command_args(false, &config);
        assert!(args.is_empty());
    }

    // -- Codex worktree setup tests --

    #[test]
    fn codex_no_flags_no_parent_does_nothing() {
        let f = WorktreeFixture::new();
        setup_codex_worktree(&f.wt, &f.repo, false, None).unwrap();
        assert!(!f.wt.join(".codex/hooks.json").exists());
        assert!(!f.wt.join(".codex/config.toml").exists());
    }

    #[test]
    fn codex_auto_commit_creates_hooks_and_config() {
        let f = WorktreeFixture::new();
        setup_codex_worktree(&f.wt, &f.repo, true, None).unwrap();

        // hooks.json should have a Stop hook
        let hooks_content = std::fs::read_to_string(f.wt.join(".codex/hooks.json")).unwrap();
        let hooks: serde_json::Value = serde_json::from_str(&hooks_content).unwrap();
        let stop = hooks["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        assert!(stop[0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("copse auto-commit"));

        // config.toml should enable codex_hooks feature
        let config_content = std::fs::read_to_string(f.wt.join(".codex/config.toml")).unwrap();
        assert!(config_content.contains("codex_hooks = true"));
    }

    #[test]
    fn codex_notification_command_creates_config() {
        let f = WorktreeFixture::new();
        setup_codex_worktree(&f.wt, &f.repo, false, Some("echo hello")).unwrap();

        let config_content = std::fs::read_to_string(f.wt.join(".codex/config.toml")).unwrap();
        assert!(config_content.contains("notify = "));
        assert!(config_content.contains("echo hello"));
    }

    #[test]
    fn codex_inherits_parent_hooks() {
        let f = WorktreeFixture::new();
        let parent_hooks_dir = f.repo.join(".codex");
        std::fs::create_dir_all(&parent_hooks_dir).unwrap();
        std::fs::write(
            parent_hooks_dir.join("hooks.json"),
            r#"{"hooks":{"Stop":[{"matcher":"","hooks":[{"type":"command","command":"echo parent"}]}]}}"#,
        )
        .unwrap();

        setup_codex_worktree(&f.wt, &f.repo, true, None).unwrap();

        let hooks_content = std::fs::read_to_string(f.wt.join(".codex/hooks.json")).unwrap();
        let hooks: serde_json::Value = serde_json::from_str(&hooks_content).unwrap();
        let stop = hooks["hooks"]["Stop"].as_array().unwrap();
        // Parent hook + copse auto-commit hook
        assert_eq!(stop.len(), 2);
    }

    #[test]
    fn codex_preserves_parent_config() {
        let f = WorktreeFixture::new();
        let parent_codex_dir = f.repo.join(".codex");
        std::fs::create_dir_all(&parent_codex_dir).unwrap();
        std::fs::write(
            parent_codex_dir.join("config.toml"),
            "model = \"gpt-5-codex\"\n\n[features]\nweb_search = true\n",
        )
        .unwrap();

        setup_codex_worktree(&f.wt, &f.repo, true, Some("echo hi")).unwrap();

        let config_content = std::fs::read_to_string(f.wt.join(".codex/config.toml")).unwrap();
        // Re-parse to verify valid TOML with no duplicate sections
        let reparsed: toml::Table = config_content.parse().expect("valid TOML");
        // Parent settings preserved
        assert_eq!(reparsed["model"].as_str(), Some("gpt-5-codex"));
        // [features] merged, not duplicated
        assert_eq!(
            reparsed["features"]["web_search"],
            toml::Value::Boolean(true)
        );
        assert_eq!(
            reparsed["features"]["codex_hooks"],
            toml::Value::Boolean(true)
        );
        // notify set
        let notify = reparsed["notify"].as_array().unwrap();
        assert_eq!(notify[2].as_str(), Some("echo hi"));
    }

    #[test]
    fn codex_parent_config_only_no_flags() {
        let f = WorktreeFixture::new();
        let parent_codex_dir = f.repo.join(".codex");
        std::fs::create_dir_all(&parent_codex_dir).unwrap();
        std::fs::write(
            parent_codex_dir.join("config.toml"),
            "model = \"gpt-5-codex\"\n",
        )
        .unwrap();

        setup_codex_worktree(&f.wt, &f.repo, false, None).unwrap();

        let config_content = std::fs::read_to_string(f.wt.join(".codex/config.toml")).unwrap();
        // Parent settings preserved even when no copse flags are set
        assert!(config_content.contains("model = \"gpt-5-codex\""));
    }

    // -- Spinner / screen detection tests (moved from task.rs) --

    fn make_screen(rows: u16, cols: u16, lines: &[&str]) -> vt100::Screen {
        let mut parser = vt100::Parser::new(rows, cols, 0);
        for line in lines {
            parser.process(line.as_bytes());
        }
        parser.screen().clone()
    }

    fn colored(text: &str) -> String {
        // ANSI: set foreground to color index 174
        format!("\x1b[38;5;174m{text}\x1b[0m")
    }

    #[test]
    fn spinner_detected_when_colored() {
        let line = colored("✢ Lollygagging…");
        let screen = make_screen(10, 40, &[&line]);
        assert!(has_active_spinner(&screen));
    }

    #[test]
    fn spinner_not_detected_when_default_color() {
        let screen = make_screen(10, 40, &["✻ Worked for 55s"]);
        assert!(!has_active_spinner(&screen));
    }

    #[test]
    fn no_spinner_on_empty_screen() {
        let screen = make_screen(10, 40, &[]);
        assert!(!has_active_spinner(&screen));
    }

    #[test]
    fn no_spinner_on_plain_text() {
        let screen = make_screen(10, 40, &["some output\r\nmore output\r\n"]);
        assert!(!has_active_spinner(&screen));
    }

    #[test]
    fn middle_dot_spinner_detected() {
        let line = colored("· Thinking…");
        let screen = make_screen(10, 40, &[&line]);
        assert!(has_active_spinner(&screen));
    }

    #[test]
    fn prompt_not_detected_as_spinner() {
        let screen = make_screen(10, 40, &["❯ hello"]);
        assert!(!has_active_spinner(&screen));
    }

    #[test]
    fn working_text_detected_with_color() {
        let line = colored("• Working (3s • esc to interrupt)");
        let screen = make_screen(10, 60, &[&line]);
        assert!(has_working_text(&screen));
    }

    #[test]
    fn working_text_detected_without_color() {
        // No-color terminals must also be detected
        let screen = make_screen(10, 60, &["• Working (3s • esc to interrupt)"]);
        assert!(has_working_text(&screen));
    }

    #[test]
    fn working_text_not_on_empty_screen() {
        let screen = make_screen(10, 40, &[]);
        assert!(!has_working_text(&screen));
    }

    #[test]
    fn working_text_not_on_plain_text() {
        let screen = make_screen(10, 40, &["some output\r\nmore output\r\n"]);
        assert!(!has_working_text(&screen));
    }

    #[test]
    fn working_text_not_on_partial_match() {
        // "Working" alone without the full pattern should not match
        let screen = make_screen(10, 40, &["Working on the feature"]);
        assert!(!has_working_text(&screen));
    }

    #[test]
    fn working_text_not_on_code_output() {
        let screen = make_screen(10, 60, &["println!(\"Working (hard)\");"]);
        assert!(!has_working_text(&screen));
    }
}
