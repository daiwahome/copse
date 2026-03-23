use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::config::Config;
use crate::event::{AppEvent, TaskId};

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Running,
    Stopped,
}

pub struct Task {
    pub id: TaskId,
    pub name: String,
    pub upstream: String,
    pub status: TaskStatus,
    /// Whether claude has been launched at least once for this task.
    /// Used to pass --continue on subsequent launches.
    pub has_run: bool,
    /// Cached number of commits ahead of upstream (None = not yet computed)
    pub commits_ahead: Option<usize>,
    /// Path to the git worktree for this task
    #[allow(dead_code)]
    pub worktree_path: PathBuf,
    /// Parses ANSI sequences and holds the screen buffer
    pub parser: Arc<Mutex<vt100::Parser>>,
    /// Write end of the PTY (sends keyboard input). None while Stopped.
    writer: Option<Box<dyn Write + Send>>,
    /// Background task that reads PTY output. None while Stopped.
    _reader_task: Option<JoinHandle<()>>,
    /// MasterPty used for resize operations. None while Stopped.
    master: Option<Box<dyn MasterPty + Send>>,
    /// Handle used to kill the child process. None while Stopped.
    killer: Option<Box<dyn portable_pty::ChildKiller + Send + Sync>>,
}

impl Task {
    /// Derive the worktree path for a given task name.
    /// Worktrees live at `<git_common_dir>/copse-worktrees/<name>`.
    /// Using the common .git dir (not repo_root/.git) ensures the path is
    /// stable even when copse runs from inside a worktree.
    pub fn worktree_path_for(git_common_dir: &Path, name: &str) -> PathBuf {
        git_common_dir.join("copse-worktrees").join(name)
    }

    /// Branch name used for a task: `copse/<name>`
    pub fn branch_name(name: &str) -> String {
        format!("copse/{name}")
    }

    /// Ensure the worktree and its branch exist, creating them if necessary.
    /// If the branch already exists (resume case) the worktree is re-added on
    /// top of the existing branch; the branch itself is never recreated.
    /// For new branches, forks from `upstream` and sets it as the tracking branch.
    async fn ensure_worktree(repo_root: &Path, git_common_dir: &Path, name: &str, upstream: &str) -> anyhow::Result<PathBuf> {
        let worktree_path = Self::worktree_path_for(git_common_dir, name);
        let branch = Self::branch_name(name);
        let repo_root = repo_root.to_path_buf();
        let upstream = upstream.to_string();

        tokio::task::spawn_blocking(move || -> anyhow::Result<PathBuf> {
            // Check whether the branch already exists
            let branch_exists = std::process::Command::new("git")
                .args(["rev-parse", "--verify", &branch])
                .current_dir(&repo_root)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if branch_exists {
                // Check if the branch already has a worktree registered (possibly at a different path)
                let wt_list = std::process::Command::new("git")
                    .args(["worktree", "list", "--porcelain"])
                    .current_dir(&repo_root)
                    .output()
                    .ok();

                if let Some(out) = wt_list {
                    let listing = String::from_utf8_lossy(&out.stdout);
                    // Parse worktree list: find the worktree that has our branch checked out
                    let mut current_wt_path: Option<PathBuf> = None;
                    let mut current_branch = String::new();
                    for line in listing.lines() {
                        if let Some(p) = line.strip_prefix("worktree ") {
                            current_wt_path = Some(PathBuf::from(p));
                            current_branch = String::new();
                        } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
                            current_branch = b.to_string();
                        } else if line.is_empty() {
                            if current_branch == branch {
                                // This branch is already checked out in a worktree
                                if let Some(existing_path) = current_wt_path.take() {
                                    if existing_path.exists() {
                                        // Use the existing worktree path directly
                                        return Ok(existing_path);
                                    }
                                }
                            }
                            current_wt_path = None;
                            current_branch = String::new();
                        }
                    }
                }

                // Prune stale worktree metadata then re-add
                let _ = std::process::Command::new("git")
                    .args(["worktree", "prune"])
                    .current_dir(&repo_root)
                    .output();

                // Ensure the parent directory exists before git worktree add
                if let Some(parent) = worktree_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                if worktree_path.exists() {
                    // Directory exists but git doesn't know about it — add with --force
                    let out = std::process::Command::new("git")
                        .args(["worktree", "add", "--force", worktree_path.to_str().unwrap(), &branch])
                        .current_dir(&repo_root)
                        .output()?;
                    if !out.status.success() {
                        anyhow::bail!(
                            "git worktree add failed: {}",
                            String::from_utf8_lossy(&out.stderr)
                        );
                    }
                } else {
                    // Resume: re-attach existing branch to a fresh worktree
                    let out = std::process::Command::new("git")
                        .args(["worktree", "add", worktree_path.to_str().unwrap(), &branch])
                        .current_dir(&repo_root)
                        .output()?;
                    if !out.status.success() {
                        anyhow::bail!(
                            "git worktree add failed: {}",
                            String::from_utf8_lossy(&out.stderr)
                        );
                    }
                }
            } else {
                // New task: create branch + worktree together, forking from upstream.
                // Ensure the parent directory exists first (git won't create it).
                if let Some(parent) = worktree_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let out = std::process::Command::new("git")
                    .args([
                        "worktree",
                        "add",
                        "-b",
                        &branch,
                        worktree_path.to_str().unwrap(),
                        &upstream,
                    ])
                    .current_dir(&repo_root)
                    .output()?;
                if !out.status.success() {
                    anyhow::bail!(
                        "git worktree add -b failed: {}",
                        String::from_utf8_lossy(&out.stderr)
                    );
                }

                // Set upstream as the tracking branch
                let _ = std::process::Command::new("git")
                    .args(["branch", "--set-upstream-to", &upstream, &branch])
                    .current_dir(&repo_root)
                    .output();
            }

            Ok(worktree_path)
        })
        .await?
    }

    /// Write `.claude/settings.local.json` into the worktree based on config.
    /// Merges template keys into existing settings, preserving user customizations.
    fn setup_claude_settings(worktree_path: &Path, config: &Config) -> anyhow::Result<()> {
        if !config.auto_commit && !config.auto_permissions {
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
            if config.auto_commit {
                if let Some(hooks) = source.get("hooks") {
                    target.entry("hooks").or_insert_with(|| hooks.clone());
                }
            }
            if config.auto_permissions {
                if let Some(permissions) = source.get("permissions") {
                    target.entry("permissions").or_insert_with(|| permissions.clone());
                }
            }
        }

        std::fs::write(
            &settings_path,
            serde_json::to_string_pretty(&settings)? + "\n",
        )?;

        Ok(())
    }

    /// Read the upstream (tracking branch) for a task branch from git.
    pub fn load_upstream(repo_root: &Path, name: &str) -> Option<String> {
        let branch = Self::branch_name(name);
        let output = std::process::Command::new("git")
            .args([
                "for-each-ref",
                "--format=%(upstream:short)",
                &format!("refs/heads/{branch}"),
            ])
            .current_dir(repo_root)
            .output()
            .ok()?;
        if output.status.success() {
            let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !val.is_empty() {
                Some(val)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Count commits ahead of upstream: `git rev-list --count <upstream>..<branch>`
    pub fn compute_commits_ahead(repo_root: &Path, name: &str, upstream: &str) -> Option<usize> {
        let branch = Self::branch_name(name);
        let output = std::process::Command::new("git")
            .args(["rev-list", "--count", &format!("{upstream}..{branch}")])
            .current_dir(repo_root)
            .output()
            .ok()?;
        if output.status.success() {
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<usize>()
                .ok()
        } else {
            None
        }
    }

    /// List local branches eligible as upstream targets.
    /// Excludes `copse/*`, `main`, and `master` branches.
    pub fn list_upstream_candidates(repo_root: &Path) -> Vec<String> {
        let output = std::process::Command::new("git")
            .args(["branch", "--format=%(refname:short)"])
            .current_dir(repo_root)
            .output();

        match output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|line| {
                    let b = line.trim();
                    !b.is_empty()
                        && !b.starts_with("copse/")
                        && b != "main"
                        && b != "master"
                })
                .map(|s| s.trim().to_string())
                .collect(),
            _ => vec![],
        }
    }

    /// List task names that have a `copse/<name>` branch in the repository.
    /// These are displayed as Stopped tasks on startup and can be resumed.
    pub fn list_existing(repo_root: &Path) -> Vec<String> {
        let output = std::process::Command::new("git")
            .args(["branch", "--list", "copse/*", "--format=%(refname:short)"])
            .current_dir(repo_root)
            .output();

        match output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter_map(|line| {
                    line.strip_prefix("copse/").map(|name| name.to_string())
                })
                .collect(),
            _ => vec![],
        }
    }

    /// Construct a brand-new Task in Stopped state.
    /// No branch or worktree is created yet; that happens on first launch.
    pub fn new_stopped(name: String, upstream: String, git_common_dir: &Path, rows: u16, cols: u16) -> Self {
        Self::make_placeholder(name, upstream, false, None, git_common_dir, rows, cols)
    }

    /// Construct a placeholder Task in Stopped state for an existing branch,
    /// without spawning any process. Reads upstream from git tracking branch.
    pub fn from_existing(name: String, repo_root: &Path, git_common_dir: &Path, rows: u16, cols: u16) -> Self {
        let upstream = Self::load_upstream(repo_root, &name)
            .unwrap_or_else(|| "HEAD".to_string());
        let commits_ahead = Self::compute_commits_ahead(repo_root, &name, &upstream);
        Self::make_placeholder(name, upstream, true, commits_ahead, git_common_dir, rows, cols)
    }

    fn make_placeholder(name: String, upstream: String, has_run: bool, commits_ahead: Option<usize>, git_common_dir: &Path, rows: u16, cols: u16) -> Self {
        Task {
            id: uuid::Uuid::new_v4(),
            worktree_path: Self::worktree_path_for(git_common_dir, &name),
            name,
            upstream,
            status: TaskStatus::Stopped,
            has_run,
            commits_ahead,
            parser: Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0))),
            writer: None,
            _reader_task: None,
            master: None,
            killer: None,
        }
    }

    /// Spawn `claude` in the task's worktree inside a PTY.
    /// If `has_run` is true, passes `--continue` to resume the last session.
    /// The `id` parameter preserves the task's identity across restarts.
    pub async fn spawn(
        id: TaskId,
        name: String,
        upstream: String,
        has_run: bool,
        repo_root: PathBuf,
        git_common_dir: PathBuf,
        config: Config,
        rows: u16,
        cols: u16,
        event_tx: mpsc::Sender<AppEvent>,
    ) -> anyhow::Result<Self> {

        // Ensure the worktree (and branch) exist before launching claude
        let worktree_path = Self::ensure_worktree(&repo_root, &git_common_dir, &name, &upstream).await?;

        // Set up .claude/settings.local.json based on config
        let wp = worktree_path.clone();
        let permission_mode = config.permission_mode.clone();
        tokio::task::spawn_blocking(move || Self::setup_claude_settings(&wp, &config)).await??;

        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new("claude");
        if has_run {
            cmd.arg("--continue");
        }
        cmd.args(["--permission-mode", &permission_mode]);
        cmd.env("TERM", "xterm-256color");
        // Run claude inside the worktree directory so it picks up the branch
        cmd.cwd(&worktree_path);

        let child = pair.slave.spawn_command(cmd)?;
        // The slave side is no longer needed once the child is spawned
        drop(pair.slave);

        let writer = pair.master.take_writer()?;
        let mut reader = pair.master.try_clone_reader()?;
        // Obtain a ChildKiller for later termination
        let killer = child.clone_killer();

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0)));
        let parser_clone = Arc::clone(&parser);
        let event_tx_clone = event_tx.clone();

        let reader_task = tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        // Use blocking_send for exit so it is never dropped
                        let _ = event_tx_clone.blocking_send(AppEvent::TaskExited(id));
                        break;
                    }
                    Ok(n) => {
                        parser_clone
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .process(&buf[..n]);
                        // try_send to avoid blocking the PTY reader when the UI is slow;
                        // dropped redraws are harmless since the parser is already updated
                        let _ = event_tx_clone.try_send(AppEvent::TaskOutput(id));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                        // Transient signal interruption — retry the read
                        continue;
                    }
                    Err(_) => {
                        let _ = event_tx_clone.blocking_send(AppEvent::TaskExited(id));
                        break;
                    }
                }
            }
        });

        Ok(Task {
            id,
            name,
            upstream,
            status: TaskStatus::Running,
            has_run: true,
            commits_ahead: None,
            worktree_path,
            parser,
            writer: Some(writer),
            _reader_task: Some(reader_task),
            master: Some(pair.master),
            killer: Some(killer),
        })
    }

    /// Delete the task: remove the worktree and delete the branch.
    pub fn delete_task(repo_root: &Path, git_common_dir: &Path, name: &str) -> anyhow::Result<()> {
        let worktree_path = Self::worktree_path_for(git_common_dir, name);
        let branch = Self::branch_name(name);

        // Remove worktree
        let out = std::process::Command::new("git")
            .args(["worktree", "remove", "--force", worktree_path.to_str().unwrap_or("")])
            .current_dir(repo_root)
            .output()?;
        if !out.status.success() {
            // Prune and retry if stale
            let _ = std::process::Command::new("git")
                .args(["worktree", "prune"])
                .current_dir(repo_root)
                .output();
        }

        // Delete branch
        let out = std::process::Command::new("git")
            .args(["branch", "-D", &branch])
            .current_dir(repo_root)
            .output()?;
        if !out.status.success() {
            anyhow::bail!(
                "branch delete failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }

        Ok(())
    }

    /// Fast-forward merge: advance upstream to the task branch's HEAD.
    pub fn merge_ff(repo_root: &Path, name: &str, upstream: &str) -> anyhow::Result<()> {
        let branch = Self::branch_name(name);
        Self::advance_branch(repo_root, upstream, &branch)?;
        Ok(())
    }

    /// Sync task branch to upstream: reset --hard inside the worktree.
    pub fn sync_from_upstream(repo_root: &Path, git_common_dir: &Path, name: &str, upstream: &str) -> anyhow::Result<()> {
        let worktree_path = Self::worktree_path_for(git_common_dir, name);

        // Resolve upstream to a commit hash
        let out = std::process::Command::new("git")
            .args(["rev-parse", upstream])
            .current_dir(repo_root)
            .output()?;
        if !out.status.success() {
            anyhow::bail!(
                "Could not resolve upstream '{}': {}",
                upstream,
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let upstream_commit = String::from_utf8_lossy(&out.stdout).trim().to_string();

        // Reset inside the worktree
        let out = std::process::Command::new("git")
            .args(["reset", "--hard", &upstream_commit])
            .current_dir(&worktree_path)
            .output()?;
        if !out.status.success() {
            anyhow::bail!(
                "reset failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }

        Ok(())
    }

    /// Send keyboard input to the PTY
    pub fn write_input(&mut self, data: &[u8]) -> anyhow::Result<()> {
        if let Some(writer) = &mut self.writer {
            writer.write_all(data)?;
        }
        Ok(())
    }

    /// Notify the PTY of a terminal size change
    pub fn resize(&mut self, rows: u16, cols: u16) -> anyhow::Result<()> {
        // Resize via MasterPty; the PTY delivers SIGWINCH to the child automatically.
        // Only sync the parser size when the PTY resize succeeds to avoid divergence.
        if let Some(master) = &self.master {
            master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })?;
        }
        self.parser
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .screen_mut()
            .set_size(rows, cols);
        Ok(())
    }

    /// Find the worktree path where the given branch is checked out.
    pub fn find_branch_worktree(repo_root: &Path, branch: &str) -> anyhow::Result<PathBuf> {
        let target_ref = format!("refs/heads/{branch}");

        let output = std::process::Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(repo_root)
            .output()?;
        let listing = String::from_utf8_lossy(&output.stdout);

        let mut worktree_path: Option<PathBuf> = None;
        let mut current_path: Option<PathBuf> = None;

        for line in listing.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                current_path = Some(PathBuf::from(p));
            } else if let Some(b) = line.strip_prefix("branch ") {
                if b == target_ref {
                    worktree_path = current_path.take();
                }
            } else if line.is_empty() {
                current_path = None;
            }
        }

        worktree_path.ok_or_else(|| {
            anyhow::anyhow!("branch '{branch}' is not checked out in any worktree")
        })
    }

    /// Advance a branch to the given commit using `merge --ff-only`.
    /// The branch must be checked out in a worktree so that the working tree
    /// and index are updated atomically along with the ref.
    pub fn advance_branch(repo_root: &Path, branch: &str, commit: &str) -> anyhow::Result<()> {
        let wt_path = Self::find_branch_worktree(repo_root, branch)?;

        let out = std::process::Command::new("git")
            .args(["merge", "--ff-only", commit])
            .current_dir(&wt_path)
            .output()?;
        if !out.status.success() {
            anyhow::bail!(
                "merge --ff-only failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }

        Ok(())
    }

    /// Forcibly terminate the task
    pub fn kill(&mut self) -> anyhow::Result<()> {
        if let Some(killer) = &mut self.killer {
            killer.kill().map_err(|e| anyhow::anyhow!("kill failed: {e}"))?;
        }
        Ok(())
    }
}
