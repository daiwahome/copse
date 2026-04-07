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
                if config.claude_code.auto_mode {
                    args.push("--enable-auto-mode".to_string());
                }
                args
            }
        }
    }

    /// Write agent-specific configuration files into the worktree.
    /// Also inherits settings from the parent repository's `.claude/settings.local.json`.
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
}
