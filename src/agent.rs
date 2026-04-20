use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;

use crate::config::{Agent, Config};
use crate::process::CommandLogExt;

/// Short context note injected into the agent's system prompt / instructions
/// when `auto_commit` is enabled, so the agent knows its work is being
/// auto-committed after every response and can plan its own commits
/// accordingly. Kept terse on purpose: 2-4 sentences, ASCII-only so it
/// survives CLI argument passing under any locale.
///
/// Used for Claude Code (`Stop` hook) and Codex (`Stop` hook) — both fire
/// once per agent response.
const AUTO_COMMIT_HINT: &str = "\
copse auto-commit is active for this session. A Stop hook runs \
`git add -A && git diff --cached --quiet || git commit -m \"copse auto-commit\"` \
after every agent response. You do not need to run `git commit` yourself; \
any uncommitted changes at the end of your turn are packaged into a \
'copse auto-commit' commit automatically. You may still make your own \
commits with meaningful messages - auto-commit only fires if leftover \
changes remain after yours.";

/// Auto-commit hint tailored for Copilot CLI. Copilot CLI has no `Stop`
/// equivalent, so copse installs a `postToolUse` hook that fires after
/// **each tool invocation** — meaning a single agent turn can produce
/// several `copse auto-commit` commits. Telling the agent the true firing
/// cadence keeps its commit planning aligned with what will actually happen.
const AUTO_COMMIT_HINT_COPILOT: &str = "\
copse auto-commit is active for this session. A postToolUse hook runs \
`git add -A && git diff --cached --quiet || git commit -m \"copse auto-commit\"` \
after every tool invocation, so a single turn can produce multiple \
'copse auto-commit' commits. You do not need to run `git commit` yourself; \
uncommitted changes are packaged automatically. You may still make your \
own commits with meaningful messages - auto-commit only fires if leftover \
changes remain after yours.";

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
            Agent::CopilotCli => {
                if is_copilot_cli_available() {
                    Ok(())
                } else {
                    anyhow::bail!(
                        "agent = \"copilotcli\" is configured but copilot is not installed or not in PATH"
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
            Agent::CopilotCli => "copilot",
        }
    }

    /// Returns a human-readable name for UI display.
    pub fn display_name(&self) -> &'static str {
        match self {
            Agent::ClaudeCode => "claude",
            Agent::Codex => "codex",
            Agent::CopilotCli => "copilot",
        }
    }

    /// Returns an icon for UI display in the task list.
    pub fn icon(&self) -> &'static str {
        match self {
            Agent::ClaudeCode => "✽",
            Agent::Codex => "⬢",
            Agent::CopilotCli => "⛑",
        }
    }

    /// All agent variants, in display order.
    pub fn all() -> &'static [Agent] {
        &[Agent::ClaudeCode, Agent::Codex, Agent::CopilotCli]
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
                if config.auto_commit {
                    args.push("--append-system-prompt".to_string());
                    args.push(AUTO_COMMIT_HINT.to_string());
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
            Agent::CopilotCli => {
                let mut args = Vec::new();
                if has_session {
                    args.push("--continue".to_string());
                }
                if let Some(ref mode) = config.copilot_cli.mode {
                    args.push("--mode".to_string());
                    args.push(mode.clone());
                }
                if config.auto_permissions {
                    args.push("--allow-all-tools".to_string());
                    args.push("--allow-all-paths".to_string());
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
            Agent::CopilotCli => setup_copilot_cli_worktree(
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
            Agent::CopilotCli => has_copilot_processing_indicator(screen),
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
            .run_status()
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
            .run_status()
            .is_ok_and(|s| s.success())
    })
}

fn is_copilot_cli_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        std::process::Command::new("copilot")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .run_status()
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

    // Ensure the generated file is ignored via $GIT_DIR/info/exclude so that
    // `git add -A` from the auto-commit Stop hook does not pick it up. This
    // keeps the worktree free of any `.gitignore` diff.
    ensure_git_excluded(worktree_path, &["/.claude/settings.local.json"])?;

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

        // Write the auto-commit hint as a session-scoped instructions file and
        // point Codex at it via `model_instructions_file`. Codex resolves a
        // relative `model_instructions_file` against the directory of
        // `config.toml` (i.e. `.codex/`), so we store the sibling file name
        // without a `.codex/` prefix — otherwise Codex would look for
        // `.codex/.codex/copse-instructions.md`. If the user already set
        // `model_instructions_file` in their own config we respect it and skip
        // (so we do not silently clobber project-specific instructions).
        if auto_commit && !table.contains_key("model_instructions_file") {
            std::fs::write(codex_dir.join("copse-instructions.md"), AUTO_COMMIT_HINT)?;
            table.insert(
                "model_instructions_file".to_string(),
                toml::Value::String("copse-instructions.md".to_string()),
            );
        }

        let config_path = codex_dir.join("config.toml");
        std::fs::write(&config_path, toml::to_string_pretty(&table)?)?;
    }

    // Ensure generated files are ignored via $GIT_DIR/info/exclude so that
    // `git add -A` from the auto-commit Stop hook does not pick them up. This
    // keeps the worktree free of any `.gitignore` diff.
    ensure_git_excluded(
        worktree_path,
        &[
            "/.codex/hooks.json",
            "/.codex/config.toml",
            "/.codex/copse-instructions.md",
        ],
    )?;

    Ok(())
}

fn setup_copilot_cli_worktree(
    worktree_path: &Path,
    _repo_root: &Path,
    auto_commit: bool,
    _notification_command: Option<&str>,
) -> anyhow::Result<()> {
    // Copilot CLI loads hooks from any `.github/hooks/*.json` file. The
    // schema is documented at:
    //   https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/use-hooks
    //
    // Supported events: sessionStart, sessionEnd, userPromptSubmitted,
    // preToolUse, postToolUse, errorOccurred.
    //
    // We use `postToolUse` (fires after every tool invocation) for
    // auto-commit because copse's diff view relies on committed state:
    // uncommitted changes are invisible, so deferring commits to end-of-turn
    // would break the in-session code review workflow. `postToolUse` keeps
    // the diff view synchronized with the agent's progress, at the cost of
    // finer-grained commits than Claude Code's `Stop` produces (potentially
    // multiple commits per turn). The `git diff --cached --quiet` guard
    // short-circuits when there is nothing to commit, so read-only tool uses
    // do not produce empty commits.
    //
    // Multiple hook files in `.github/hooks/` are merged by Copilot CLI, so
    // we write our own file without touching any existing ones.
    //
    // `notification_command` is not supported: there is no hook event that
    // fires when Copilot CLI is waiting for user input.
    if !auto_commit {
        return Ok(());
    }

    let hooks_dir = worktree_path.join(".github").join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;

    let hooks = serde_json::json!({
        "version": 1,
        "hooks": {
            "postToolUse": [{
                "type": "command",
                "bash": "git add -A && git diff --cached --quiet || git commit -m 'copse auto-commit'",
                "cwd": ".",
                "timeoutSec": 30
            }]
        }
    });

    std::fs::write(
        hooks_dir.join("copse-hooks.json"),
        serde_json::to_string_pretty(&hooks)? + "\n",
    )?;

    // -- Auto-commit hint via .github/instructions/ --
    //
    // Copilot CLI auto-loads `.github/instructions/**/*.instructions.md` and
    // merges them with any top-level `AGENTS.md` / `CLAUDE.md` etc.
    // Reference:
    //   https://docs.github.com/en/copilot/customizing-copilot/adding-repository-custom-instructions-for-github-copilot
    //
    // This is safer than appending to the project's `AGENTS.md`: if that
    // file is tracked, the worktree already has the tracked copy (worktrees
    // share tracked files with HEAD) and overwriting it would produce a
    // modification that `info/exclude` cannot hide (exclude only applies to
    // untracked files), which would then be auto-committed on every turn.
    //
    // `copse.instructions.md` is a copse-specific filename, so collisions
    // with project-tracked files are effectively impossible. The `applyTo:
    // "**"` frontmatter marks this as a global instruction that applies
    // regardless of which file the agent is working on.
    let instructions_dir = worktree_path.join(".github").join("instructions");
    std::fs::create_dir_all(&instructions_dir)?;
    std::fs::write(
        instructions_dir.join("copse.instructions.md"),
        format!("---\napplyTo: \"**\"\n---\n{AUTO_COMMIT_HINT_COPILOT}\n"),
    )?;

    ensure_git_excluded(
        worktree_path,
        &[
            "/.github/hooks/copse-hooks.json",
            "/.github/instructions/copse.instructions.md",
        ],
    )?;

    Ok(())
}

const COPSE_MARKER: &str = "# managed by copse";

/// Resolve the per-worktree `$GIT_DIR` for `worktree_path`.
///
/// - If `<worktree>/.git` is a directory, it IS the gitdir (main worktree).
/// - If it is a file, it contains `gitdir: <path>` pointing at the linked
///   worktree's gitdir (typically `<repo>/.git/worktrees/<name>`).
///
/// The resolved path is sanity-checked: a valid git directory always contains
/// a `HEAD` file. This prevents writing to arbitrary paths if the `.git` file
/// was hand-crafted or corrupted.
fn worktree_gitdir(worktree_path: &Path) -> anyhow::Result<PathBuf> {
    let git_path = worktree_path.join(".git");
    let meta = std::fs::metadata(&git_path)?;
    let resolved = if meta.is_dir() {
        git_path.clone()
    } else {
        let content = std::fs::read_to_string(&git_path)?;
        let pointer = content
            .lines()
            .find_map(|l| l.strip_prefix("gitdir:").map(str::trim))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    ".git at {} is not a valid gitdir pointer",
                    git_path.display()
                )
            })?;
        let pointer_path = Path::new(pointer);
        if pointer_path.is_absolute() {
            pointer_path.to_path_buf()
        } else {
            worktree_path.join(pointer_path)
        }
    };

    // Sanity check: a git directory (main or linked worktree) always has HEAD.
    // Refuse to write to a location that does not look like a git dir.
    if !resolved.join("HEAD").exists() {
        anyhow::bail!(
            "resolved gitdir {} does not look like a git directory (missing HEAD)",
            resolved.display()
        );
    }

    Ok(resolved)
}

/// Resolve `$GIT_COMMON_DIR` for `worktree_path`.
///
/// `info/exclude` lives in the **common** gitdir, not the per-worktree gitdir.
/// For a linked worktree, the per-worktree gitdir contains a `commondir` file
/// pointing back at the shared gitdir. For a main worktree, the gitdir itself
/// is already the common dir, so no `commondir` file exists.
///
/// Writing `info/exclude` to the per-worktree gitdir silently has no effect
/// on `git add` / `git status`, which is the bug this function fixes.
fn worktree_common_dir(worktree_path: &Path) -> anyhow::Result<PathBuf> {
    let gitdir = worktree_gitdir(worktree_path)?;
    let commondir_file = gitdir.join("commondir");
    if !commondir_file.exists() {
        return Ok(gitdir);
    }
    let content = std::fs::read_to_string(&commondir_file)?;
    let pointer = content.trim();
    let pointer_path = Path::new(pointer);
    if pointer_path.is_absolute() {
        Ok(pointer_path.to_path_buf())
    } else {
        // `commondir` is relative to the per-worktree gitdir.
        Ok(gitdir.join(pointer_path))
    }
}

/// Append gitignore-style `patterns` to the repository's
/// `$GIT_COMMON_DIR/info/exclude`.
///
/// Patterns already present are skipped (idempotent). `info/exclude` is a
/// local-only ignore list that lives inside `.git/` and is never committed or
/// surfaced in `git status` / `git diff`, so this lets copse hide the agent
/// configuration files it generates without modifying any tracked file.
/// Because `info/exclude` lives in the common gitdir, a single write covers
/// every linked worktree of the same repository.
fn ensure_git_excluded(worktree_path: &Path, patterns: &[&str]) -> anyhow::Result<()> {
    let common_dir = worktree_common_dir(worktree_path)?;
    let info_dir = common_dir.join("info");
    std::fs::create_dir_all(&info_dir)?;
    let exclude_path = info_dir.join("exclude");

    let existing = std::fs::read_to_string(&exclude_path).unwrap_or_default();
    let already_present: std::collections::HashSet<&str> =
        existing.lines().map(str::trim).collect();
    let missing: Vec<&str> = patterns
        .iter()
        .copied()
        .filter(|p| !already_present.contains(p.trim()))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }

    // Whether the copse marker is already present as a dedicated line. We
    // compare line-by-line (rather than via substring search) so that an
    // unrelated line happening to contain the marker text cannot confuse us.
    let marker_present = existing.lines().map(str::trim).any(|l| l == COPSE_MARKER);

    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    if !marker_present {
        content.push_str(COPSE_MARKER);
        content.push('\n');
    }
    for p in missing {
        content.push_str(p);
        content.push('\n');
    }
    std::fs::write(&exclude_path, content)?;
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

/// Detect whether Copilot CLI is actively processing.
///
/// Two mutually-exclusive indicators are relevant:
///
/// - **Idle prompt**: a line starting with `❯` containing `"Type @"` — the
///   agent is waiting for user input.
/// - **Processing spinner**: a line starting with `● ◉ ◎ ○` containing
///   `"Esc to cancel"`, e.g. `◉ Exploring codebase (Esc to cancel · 16.0 KiB)`.
///
/// Both can coexist in the scrollback (an old idle prompt above a fresh
/// spinner, or a stale spinner above a new idle prompt). The current state
/// is whichever indicator appears **lowest on the screen** (the most recent
/// output). We scan bottom-to-top and return on the first match — neither
/// indicator gets absolute priority over the other.
fn has_copilot_processing_indicator(screen: &vt100::Screen) -> bool {
    const COPILOT_SPINNERS: &[char] = &['●', '◉', '◎', '○'];

    let mut is_processing = false;
    scan_screen_rows(screen, |_r, trimmed| {
        // Idle prompt: agent is waiting for input.
        if trimmed.starts_with('❯') && trimmed.contains("Type @") {
            is_processing = false;
            return true; // stop — this is the most recent relevant row
        }
        // Spinner + "Esc to cancel": agent is actively processing.
        if let Some(first) = trimmed.chars().next() {
            if COPILOT_SPINNERS.contains(&first) && trimmed.contains("Esc to cancel") {
                is_processing = true;
                return true; // stop — this is the most recent relevant row
            }
        }
        false
    });
    is_processing
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

    #[test]
    fn claudecode_command_args_appends_auto_commit_hint() {
        let mut config = Config::default();
        config.auto_commit = true;
        let args = Agent::ClaudeCode.command_args(false, &config);

        // `--append-system-prompt` must be followed immediately by the hint.
        let pos = args
            .iter()
            .position(|a| a == "--append-system-prompt")
            .expect("--append-system-prompt should be present when auto_commit=true");
        assert_eq!(
            args.get(pos + 1).map(String::as_str),
            Some(AUTO_COMMIT_HINT)
        );
    }

    #[test]
    fn claudecode_command_args_omits_hint_when_auto_commit_off() {
        let config = Config::default();
        assert!(!config.auto_commit, "sanity: default is off");
        let args = Agent::ClaudeCode.command_args(false, &config);
        assert!(!args.iter().any(|a| a == "--append-system-prompt"));
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
            // Simulate a main-worktree `.git/` directory so that
            // `ensure_git_excluded` can resolve `$GIT_DIR` and write to
            // `info/exclude` during setup. `HEAD` is required by
            // `worktree_gitdir`'s sanity check.
            std::fs::create_dir_all(wt.join(".git/info")).unwrap();
            std::fs::write(wt.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
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

        fn read_info_exclude(&self) -> String {
            std::fs::read_to_string(self.wt.join(".git/info/exclude")).unwrap_or_default()
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
    fn codex_auto_commit_writes_instructions_file() {
        let f = WorktreeFixture::new();
        setup_codex_worktree(&f.wt, &f.repo, true, None).unwrap();

        // The hint file is created with the AUTO_COMMIT_HINT body.
        let hint = std::fs::read_to_string(f.wt.join(".codex/copse-instructions.md")).unwrap();
        assert_eq!(hint, AUTO_COMMIT_HINT);

        // config.toml points Codex at it via model_instructions_file. The
        // value is a bare filename (no `.codex/` prefix) because Codex resolves
        // it relative to the config.toml directory.
        let config_content = std::fs::read_to_string(f.wt.join(".codex/config.toml")).unwrap();
        let reparsed: toml::Table = config_content.parse().expect("valid TOML");
        assert_eq!(
            reparsed["model_instructions_file"].as_str(),
            Some("copse-instructions.md")
        );
    }

    #[test]
    fn codex_auto_commit_respects_user_model_instructions_file() {
        // If the user already configured model_instructions_file in their
        // repo-root .codex/config.toml, copse must not silently clobber it
        // and must not leave a stray copse-instructions.md behind.
        let f = WorktreeFixture::new();
        let parent_codex_dir = f.repo.join(".codex");
        std::fs::create_dir_all(&parent_codex_dir).unwrap();
        std::fs::write(
            parent_codex_dir.join("config.toml"),
            "model_instructions_file = \"custom.md\"\n",
        )
        .unwrap();

        setup_codex_worktree(&f.wt, &f.repo, true, None).unwrap();

        let config_content = std::fs::read_to_string(f.wt.join(".codex/config.toml")).unwrap();
        let reparsed: toml::Table = config_content.parse().expect("valid TOML");
        assert_eq!(
            reparsed["model_instructions_file"].as_str(),
            Some("custom.md"),
            "user's model_instructions_file must be preserved"
        );
        assert!(
            !f.wt.join(".codex/copse-instructions.md").exists(),
            "copse-instructions.md should not be written when user overrides the key"
        );
    }

    #[test]
    fn codex_notification_only_does_not_write_instructions_file() {
        // With only a notification command (and no auto_commit) copse still
        // writes config.toml, but no instructions file should appear.
        let f = WorktreeFixture::new();
        setup_codex_worktree(&f.wt, &f.repo, false, Some("echo hi")).unwrap();

        assert!(!f.wt.join(".codex/copse-instructions.md").exists());
        let config_content = std::fs::read_to_string(f.wt.join(".codex/config.toml")).unwrap();
        let reparsed: toml::Table = config_content.parse().expect("valid TOML");
        assert!(!reparsed.contains_key("model_instructions_file"));
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

    // -- ensure_git_excluded / worktree_gitdir tests --

    /// Helper: create a minimal main-worktree `.git/` directory with `HEAD`.
    fn init_fake_main_gitdir(wt: &Path) {
        std::fs::create_dir_all(wt.join(".git/info")).unwrap();
        std::fs::write(wt.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    }

    /// Helper: create a minimal linked-worktree setup. Returns the per-worktree
    /// gitdir path. `gitdir_value` is the raw text written after `gitdir: ` in
    /// the worktree's `.git` file (absolute or relative). Also creates the
    /// common gitdir at `<tmp>/.git/` (empty except for `HEAD`) and writes a
    /// `commondir` file in the per-worktree gitdir pointing at it — matching
    /// what `git worktree add` produces.
    fn init_fake_linked_worktree(
        tmp: &Path,
        name: &str,
        gitdir_value: &str,
        gitdir_abs: &Path,
    ) -> PathBuf {
        std::fs::create_dir_all(gitdir_abs).unwrap();
        std::fs::write(gitdir_abs.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        // Per-worktree gitdir has a `commondir` file pointing at the common
        // gitdir. We use an absolute path for determinism in tests.
        let common_gitdir = tmp.join(".git");
        std::fs::create_dir_all(&common_gitdir).unwrap();
        std::fs::write(common_gitdir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            gitdir_abs.join("commondir"),
            format!("{}\n", common_gitdir.display()),
        )
        .unwrap();
        let wt = tmp.join(name);
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {gitdir_value}\n")).unwrap();
        wt
    }

    #[test]
    fn ensure_git_excluded_writes_patterns() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("wt");
        init_fake_main_gitdir(&wt);

        ensure_git_excluded(&wt, &["/.codex/hooks.json", "/.codex/config.toml"]).unwrap();

        let content = std::fs::read_to_string(wt.join(".git/info/exclude")).unwrap();
        assert!(content.contains("# managed by copse"));
        assert!(content.contains("/.codex/hooks.json"));
        assert!(content.contains("/.codex/config.toml"));
    }

    #[test]
    fn ensure_git_excluded_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("wt");
        init_fake_main_gitdir(&wt);

        ensure_git_excluded(&wt, &["/.codex/hooks.json"]).unwrap();
        ensure_git_excluded(&wt, &["/.codex/hooks.json"]).unwrap();

        let content = std::fs::read_to_string(wt.join(".git/info/exclude")).unwrap();
        let occurrences = content.matches("/.codex/hooks.json").count();
        assert_eq!(occurrences, 1, "pattern should appear only once");
        let markers = content.matches("# managed by copse").count();
        assert_eq!(markers, 1, "marker should appear only once");
    }

    #[test]
    fn ensure_git_excluded_adds_new_pattern_without_duplicating_existing() {
        // First call registers A; second call registers both A and B. The
        // final file should have A exactly once and B once, with a single
        // marker line.
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("wt");
        init_fake_main_gitdir(&wt);

        ensure_git_excluded(&wt, &["/.codex/hooks.json"]).unwrap();
        ensure_git_excluded(&wt, &["/.codex/hooks.json", "/.claude/settings.local.json"]).unwrap();

        let content = std::fs::read_to_string(wt.join(".git/info/exclude")).unwrap();
        assert_eq!(content.matches("/.codex/hooks.json").count(), 1);
        assert_eq!(content.matches("/.claude/settings.local.json").count(), 1);
        assert_eq!(content.matches("# managed by copse").count(), 1);
    }

    #[test]
    fn ensure_git_excluded_preserves_existing_exclude_content() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("wt");
        init_fake_main_gitdir(&wt);
        std::fs::write(wt.join(".git/info/exclude"), "# user rule\nfoo.log\n").unwrap();

        ensure_git_excluded(&wt, &["/.codex/hooks.json"]).unwrap();

        let content = std::fs::read_to_string(wt.join(".git/info/exclude")).unwrap();
        assert!(content.starts_with("# user rule\nfoo.log\n"));
        assert!(content.contains("/.codex/hooks.json"));
    }

    #[test]
    fn ensure_git_excluded_does_not_false_match_marker_substring() {
        // A user-written line that merely contains the marker text as a
        // substring should NOT be treated as an existing marker. copse should
        // still emit its own dedicated marker line on first write.
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("wt");
        init_fake_main_gitdir(&wt);
        std::fs::write(wt.join(".git/info/exclude"), "foo # managed by copse bar\n").unwrap();

        ensure_git_excluded(&wt, &["/.codex/hooks.json"]).unwrap();

        let content = std::fs::read_to_string(wt.join(".git/info/exclude")).unwrap();
        // A dedicated marker line should now exist in addition to the pre-existing
        // user line. We assert by checking that a bare marker line is present.
        assert!(content.lines().any(|l| l.trim() == "# managed by copse"));
    }

    #[test]
    fn ensure_git_excluded_writes_to_common_dir_for_linked_worktree_absolute() {
        // `info/exclude` lives in the common gitdir, not the per-worktree
        // gitdir. A write into the per-worktree info/exclude would be silently
        // ignored by git — which is exactly the bug this test guards against.
        let tmp = tempfile::tempdir().unwrap();
        let per_worktree_gitdir = tmp.path().join(".git/worktrees/wt");
        let wt = init_fake_linked_worktree(
            tmp.path(),
            "wt",
            &per_worktree_gitdir.display().to_string(),
            &per_worktree_gitdir,
        );

        ensure_git_excluded(&wt, &["/.codex/hooks.json"]).unwrap();

        let common_exclude = tmp.path().join(".git/info/exclude");
        let content = std::fs::read_to_string(&common_exclude).unwrap();
        assert!(content.contains("/.codex/hooks.json"));
        // The per-worktree info/exclude must NOT be written (git would ignore it).
        assert!(!per_worktree_gitdir.join("info/exclude").exists());
    }

    #[test]
    fn ensure_git_excluded_writes_to_common_dir_for_linked_worktree_relative() {
        // git writes the `gitdir:` pointer as a relative path when the worktree
        // lives next to the repo (e.g. `gitdir: ../.git/worktrees/wt`). The
        // helper must resolve it against the worktree path, then find the
        // common dir via `commondir`.
        let tmp = tempfile::tempdir().unwrap();
        let per_worktree_gitdir = tmp.path().join(".git/worktrees/wt");
        let wt = init_fake_linked_worktree(
            tmp.path(),
            "wt",
            "../.git/worktrees/wt",
            &per_worktree_gitdir,
        );

        ensure_git_excluded(&wt, &["/.codex/hooks.json"]).unwrap();

        let common_exclude = tmp.path().join(".git/info/exclude");
        let content = std::fs::read_to_string(&common_exclude).unwrap();
        assert!(content.contains("/.codex/hooks.json"));
    }

    #[test]
    fn ensure_git_excluded_rejects_malformed_git_file() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        // A `.git` file without a `gitdir:` prefix is malformed.
        std::fs::write(wt.join(".git"), "not a gitdir pointer\n").unwrap();

        let err = ensure_git_excluded(&wt, &["/.codex/hooks.json"]).unwrap_err();
        assert!(
            err.to_string().contains("gitdir pointer"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn ensure_git_excluded_rejects_non_git_directory() {
        // A directory that has `.git/` but no `HEAD` is not a valid git dir.
        // Refuse to write to it rather than silently polluting an arbitrary
        // directory.
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(wt.join(".git")).unwrap();

        let err = ensure_git_excluded(&wt, &["/.codex/hooks.json"]).unwrap_err();
        assert!(
            err.to_string()
                .contains("does not look like a git directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn claude_setup_writes_info_exclude() {
        let f = WorktreeFixture::new();

        setup_claude_code_worktree(&f.wt, &f.repo, true, false, None).unwrap();

        let exclude = f.read_info_exclude();
        assert!(exclude.contains("/.claude/settings.local.json"));
    }

    #[test]
    fn codex_setup_writes_info_exclude() {
        let f = WorktreeFixture::new();

        setup_codex_worktree(&f.wt, &f.repo, true, None).unwrap();

        let exclude = f.read_info_exclude();
        assert!(exclude.contains("/.codex/hooks.json"));
        assert!(exclude.contains("/.codex/config.toml"));
        assert!(exclude.contains("/.codex/copse-instructions.md"));
    }

    #[test]
    fn setup_skips_info_exclude_when_nothing_written() {
        let f = WorktreeFixture::new();

        // No flags, no parent — setup returns early before writing anything.
        setup_claude_code_worktree(&f.wt, &f.repo, false, false, None).unwrap();
        setup_codex_worktree(&f.wt, &f.repo, false, None).unwrap();

        // info/exclude should not have been touched.
        assert!(!f.wt.join(".git/info/exclude").exists());
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

    // -- Agent icon / enumeration tests --

    #[test]
    fn agent_icons_are_distinct_per_variant() {
        assert_ne!(Agent::ClaudeCode.icon(), Agent::Codex.icon());
        assert_ne!(Agent::ClaudeCode.icon(), Agent::CopilotCli.icon());
        assert_ne!(Agent::Codex.icon(), Agent::CopilotCli.icon());
    }

    #[test]
    fn agent_all_contains_every_variant() {
        let all = Agent::all();
        assert!(all.contains(&Agent::ClaudeCode));
        assert!(all.contains(&Agent::Codex));
        assert!(all.contains(&Agent::CopilotCli));
        assert_eq!(all.len(), 3);
    }

    // -- Copilot CLI command tests --

    #[test]
    fn copilotcli_command_name() {
        assert_eq!(Agent::CopilotCli.command_name(), "copilot");
    }

    #[test]
    fn copilotcli_display_name() {
        assert_eq!(Agent::CopilotCli.display_name(), "copilot");
    }

    #[test]
    fn copilotcli_command_args_fresh() {
        let config = Config::default();
        let args = Agent::CopilotCli.command_args(false, &config);
        assert!(args.is_empty());
    }

    #[test]
    fn copilotcli_command_args_continue() {
        let config = Config::default();
        let args = Agent::CopilotCli.command_args(true, &config);
        assert_eq!(args, vec!["--continue"]);
    }

    #[test]
    fn copilotcli_command_args_with_mode() {
        let mut config = Config::default();
        config.copilot_cli.mode = Some("autopilot".to_string());
        let args = Agent::CopilotCli.command_args(false, &config);
        assert_eq!(args, vec!["--mode", "autopilot"]);
    }

    #[test]
    fn copilotcli_command_args_auto_permissions() {
        let mut config = Config::default();
        config.auto_permissions = true;
        let args = Agent::CopilotCli.command_args(false, &config);
        assert_eq!(args, vec!["--allow-all-tools", "--allow-all-paths"]);
    }

    #[test]
    fn copilotcli_command_args_continue_with_mode_and_permissions() {
        let mut config = Config::default();
        config.copilot_cli.mode = Some("plan".to_string());
        config.auto_permissions = true;
        let args = Agent::CopilotCli.command_args(true, &config);
        assert_eq!(
            args,
            vec![
                "--continue",
                "--mode",
                "plan",
                "--allow-all-tools",
                "--allow-all-paths"
            ]
        );
    }

    // -- Copilot CLI worktree setup tests --

    #[test]
    fn copilotcli_no_flags_no_parent_does_nothing() {
        let f = WorktreeFixture::new();
        setup_copilot_cli_worktree(&f.wt, &f.repo, false, None).unwrap();
        assert!(!f.wt.join(".github/hooks/copse-hooks.json").exists());
        assert!(!f
            .wt
            .join(".github/instructions/copse.instructions.md")
            .exists());
    }

    #[test]
    fn copilotcli_auto_commit_creates_hooks() {
        let f = WorktreeFixture::new();
        setup_copilot_cli_worktree(&f.wt, &f.repo, true, None).unwrap();

        let hooks_content =
            std::fs::read_to_string(f.wt.join(".github/hooks/copse-hooks.json")).unwrap();
        let hooks: serde_json::Value = serde_json::from_str(&hooks_content).unwrap();
        assert_eq!(hooks["version"], 1);
        let post = hooks["hooks"]["postToolUse"].as_array().unwrap();
        assert_eq!(post.len(), 1);
        assert_eq!(post[0]["type"], "command");
        assert!(post[0]["bash"]
            .as_str()
            .unwrap()
            .contains("copse auto-commit"));
    }

    #[test]
    fn copilotcli_auto_commit_writes_instructions_file() {
        let f = WorktreeFixture::new();
        setup_copilot_cli_worktree(&f.wt, &f.repo, true, None).unwrap();

        let content =
            std::fs::read_to_string(f.wt.join(".github/instructions/copse.instructions.md"))
                .unwrap();
        // Frontmatter: must start with `---`, declare `applyTo: "**"`, and
        // close with `---`. The auto-commit hint body follows.
        assert!(
            content.starts_with("---\n"),
            "instructions file must begin with frontmatter delimiter"
        );
        assert!(
            content.contains("applyTo: \"**\""),
            "frontmatter must declare applyTo: \"**\" so the instruction applies globally"
        );
        let (frontmatter, body) = content
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .expect("frontmatter must be closed with a `---` line");
        assert!(frontmatter.contains("applyTo: \"**\""));
        // Body must describe the actual Copilot CLI hook event (postToolUse,
        // per-tool) rather than the Claude/Codex Stop-hook wording, so the
        // agent's commit planning matches reality.
        assert!(body.contains(AUTO_COMMIT_HINT_COPILOT));
        assert!(body.contains("postToolUse"));
        assert!(!body.contains("Stop hook"));
    }

    #[test]
    fn copilotcli_auto_commit_does_not_touch_parent_agents_md() {
        // Regression test: the earlier implementation overwrote a tracked
        // AGENTS.md with `parent_content + hint`, which `info/exclude` cannot
        // hide (exclude only applies to untracked files). The auto-commit
        // hook would then commit that modification on every turn.
        let f = WorktreeFixture::new();
        std::fs::write(f.repo.join("AGENTS.md"), "# Project instructions\n").unwrap();

        setup_copilot_cli_worktree(&f.wt, &f.repo, true, None).unwrap();

        // The worktree's AGENTS.md must not be created by copse.
        assert!(!f.wt.join("AGENTS.md").exists());
        // The parent's AGENTS.md must be left untouched.
        let parent = std::fs::read_to_string(f.repo.join("AGENTS.md")).unwrap();
        assert_eq!(parent, "# Project instructions\n");
    }

    #[test]
    fn copilotcli_notification_only_does_nothing() {
        // notification_command is not supported for Copilot CLI.
        let f = WorktreeFixture::new();
        setup_copilot_cli_worktree(&f.wt, &f.repo, false, Some("echo notify")).unwrap();
        assert!(!f.wt.join(".github/hooks/copse-hooks.json").exists());
    }

    #[test]
    fn copilotcli_does_not_touch_parent_hook_files() {
        // Copilot CLI merges multiple hook files from .github/hooks/, so we
        // only write our own file and leave others alone.
        let f = WorktreeFixture::new();
        let parent_hooks_dir = f.repo.join(".github/hooks");
        std::fs::create_dir_all(&parent_hooks_dir).unwrap();
        std::fs::write(parent_hooks_dir.join("project.json"), "{}").unwrap();

        setup_copilot_cli_worktree(&f.wt, &f.repo, true, None).unwrap();

        assert!(f.wt.join(".github/hooks/copse-hooks.json").exists());
        assert!(!f.wt.join(".github/hooks/project.json").exists());
    }

    #[test]
    fn copilotcli_setup_writes_info_exclude() {
        let f = WorktreeFixture::new();
        setup_copilot_cli_worktree(&f.wt, &f.repo, true, None).unwrap();

        let exclude = f.read_info_exclude();
        assert!(exclude.contains("/.github/hooks/copse-hooks.json"));
        assert!(exclude.contains("/.github/instructions/copse.instructions.md"));
    }

    #[test]
    fn copilotcli_setup_skips_info_exclude_when_nothing_written() {
        let f = WorktreeFixture::new();
        setup_copilot_cli_worktree(&f.wt, &f.repo, false, None).unwrap();
        assert!(f.read_info_exclude().is_empty());
    }

    // -- Copilot CLI processing detection tests --

    #[test]
    fn copilotcli_processing_idle_prompt_returns_false() {
        let screen = make_screen(10, 80, &["❯ Type @ to mention files"]);
        assert!(!has_copilot_processing_indicator(&screen));
    }

    #[test]
    fn copilotcli_processing_spinner_returns_true() {
        let screen = make_screen(10, 80, &["◉ Exploring codebase (Esc to cancel · 16.0 KiB)"]);
        assert!(has_copilot_processing_indicator(&screen));
    }

    #[test]
    fn copilotcli_processing_newest_idle_prompt_below_spinner_wins() {
        // Stale spinner above a fresh idle prompt: decision is based on the
        // bottommost (most recent) indicator, so the agent is idle.
        let screen = make_screen(
            10,
            80,
            &[
                "◉ Exploring codebase (Esc to cancel · 16.0 KiB)\r\n",
                "❯ Type @ to mention files",
            ],
        );
        assert!(!has_copilot_processing_indicator(&screen));
    }

    #[test]
    fn copilotcli_processing_newest_spinner_below_idle_prompt_wins() {
        // Stale idle prompt above a fresh spinner: the spinner is the most
        // recent indicator, so the agent is processing. Regression test for
        // the bug where the idle prompt was given absolute priority.
        let screen = make_screen(
            10,
            80,
            &[
                "❯ Type @ to mention files\r\n",
                "◉ Exploring codebase (Esc to cancel · 16.0 KiB)",
            ],
        );
        assert!(has_copilot_processing_indicator(&screen));
    }

    #[test]
    fn copilotcli_processing_spinner_without_esc_cancel_returns_false() {
        // Static lines that start with a spinner character but are not the
        // active-processing indicator (no "Esc to cancel") must not match.
        let screen = make_screen(10, 80, &["● Environment loaded: 1 MCP server, 1 skill"]);
        assert!(!has_copilot_processing_indicator(&screen));
    }

    #[test]
    fn copilotcli_processing_empty_screen_returns_false() {
        let screen = make_screen(10, 80, &[]);
        assert!(!has_copilot_processing_indicator(&screen));
    }

    #[test]
    fn copilotcli_processing_plain_text_returns_false() {
        let screen = make_screen(10, 80, &["some output\r\nmore output"]);
        assert!(!has_copilot_processing_indicator(&screen));
    }
}
