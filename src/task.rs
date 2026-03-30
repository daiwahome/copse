use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use portable_pty::{MasterPty, NativePtySystem, PtySize, PtySystem};
use tokio::{sync::mpsc, task::JoinHandle};

use etcetera::BaseStrategy;

use crate::config::{Backend, Config};
use crate::event::{AppEvent, TaskId};

const SCROLLBACK_LEN: usize = 10_000;

pub struct SpawnParams {
    pub id: TaskId,
    pub name: String,
    pub upstream: Option<String>,
    pub has_session: bool,
    pub repo_root: PathBuf,
    pub worktree_base_dir: PathBuf,
    pub config: Config,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Running,
    Stopped,
    Deleting,
}

pub struct Task {
    pub id: TaskId,
    pub name: String,
    pub upstream: Option<String>,
    pub status: TaskStatus,
    /// Whether a continuable Claude session exists for this task.
    /// When true, `--continue` is passed on the next launch.
    pub has_session: bool,
    /// Cached number of commits ahead of upstream (None = not yet computed)
    pub commits_ahead: Option<usize>,
    /// Whether the upstream branch currently exists in the repo
    pub upstream_exists: bool,
    /// Path to the git worktree for this task
    #[allow(dead_code)]
    pub worktree_path: PathBuf,
    /// Parses ANSI sequences and holds the screen buffer
    pub parser: Arc<Mutex<vt100::Parser>>,
    /// Cached: whether Claude appears to be waiting for user input
    pub waiting_for_input: bool,
    /// True when PTY output has arrived since last `update_waiting_status` call
    pub waiting_status_dirty: bool,
    /// Set when Enter is pressed; cleared when a non-waiting screen state is seen or
    /// after 300 ms. Prevents flipping back to "waiting" before Claude has had time
    /// to start processing (handles echo + response arriving in the same PTY batch).
    pub last_enter_at: Option<std::time::Instant>,
    /// Set when detect_waiting first returns true after the grace period.
    /// "Waiting" is only accepted after 100 ms of consistent detection.
    /// Resets to None when a non-waiting state is observed.
    pub first_waiting_at: Option<std::time::Instant>,
    /// Scrollback offset for the agent view (0 = live view)
    pub scroll_offset: usize,
    /// Backend used for this task
    pub backend: Backend,
    /// Backend session identifier (e.g. tmux session name). None for BuiltIn.
    pub session_id: Option<String>,
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
    pub fn is_running(&self) -> bool {
        self.status == TaskStatus::Running
    }

    pub fn is_stopped(&self) -> bool {
        self.status == TaskStatus::Stopped
    }

    /// Whether this task has a PTY attached (i.e. copse is connected to the process).
    /// A tmux-backed task can be Running but not attached (background execution).
    pub fn is_attached(&self) -> bool {
        self.writer.is_some()
    }

    /// Re-scan PTY screen and update the cached `waiting_for_input` flag.
    pub fn update_waiting_status(&mut self) {
        if !self.is_running() {
            self.waiting_for_input = false;
            return;
        }
        let screen = self
            .parser
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .screen()
            .clone();
        let rows = screen.size().0 as usize;
        let cols = screen.size().1 as usize;
        let cursor_row = screen.cursor_position().0;

        let mut lines: Vec<(String, bool)> = Vec::new();
        for r in (0..rows).rev() {
            let row_text: String = (0..cols)
                .filter_map(|c| {
                    screen
                        .cell(r as u16, c as u16)
                        .map(|cell| cell.contents().to_string())
                })
                .collect();
            let trimmed = row_text.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }
            let has_cursor = r as u16 == cursor_row;
            lines.push((trimmed, has_cursor));
            if lines.len() >= 10 {
                break;
            }
        }
        // Clear the expired grace-period timer regardless of detection result.
        if self
            .last_enter_at
            .is_some_and(|t| t.elapsed() >= std::time::Duration::from_millis(300))
        {
            self.last_enter_at = None;
        }
        match detect_waiting(&lines) {
            WaitingDetection::Waiting => {
                // Guard: don't flip back to "waiting" within 300 ms of Enter.
                let grace_expired = self.last_enter_at.is_none();
                if grace_expired {
                    // Hysteresis: require 100 ms of consistent detection before accepting.
                    let first = self
                        .first_waiting_at
                        .get_or_insert_with(std::time::Instant::now);
                    if first.elapsed() >= std::time::Duration::from_millis(100) {
                        self.waiting_for_input = true;
                        self.first_waiting_at = None;
                    }
                }
            }
            WaitingDetection::Running => {
                // Clear running signal: definitely not waiting.
                self.first_waiting_at = None;
                self.waiting_for_input = false;
            }
            WaitingDetection::Unknown => {
                // No recognisable signal (e.g. completion list pushed ❯ out of scan
                // window, or background PTY render between clear states).
                // Maintain waiting_for_input unchanged.
                // Reset hysteresis so a brief ❯ render during processing cannot
                // accumulate through Unknown states to falsely accept "waiting".
                self.first_waiting_at = None;
            }
        }
    }

    /// Derive the worktree path for a given task name.
    pub fn worktree_path_for(worktree_base_dir: &Path, name: &str) -> PathBuf {
        worktree_base_dir.join(name)
    }

    /// Path to the session marker file for a given task name.
    /// Placed in `worktree_base_dir` (outside the worktree) to avoid git tracking.
    fn session_marker_path_for(worktree_base_dir: &Path, name: &str) -> PathBuf {
        worktree_base_dir.join(format!("{name}.has-session"))
    }

    fn session_marker_path(&self) -> PathBuf {
        let base = self.worktree_path.parent().unwrap_or(&self.worktree_path);
        Self::session_marker_path_for(base, &self.name)
    }

    /// Branch name used for a task: `copse/<name>`
    pub fn branch_name(name: &str) -> String {
        format!("copse/{name}")
    }

    /// Ensure the worktree and its branch exist, creating them if necessary.
    /// If the branch already exists (resume case) the worktree is re-added on
    /// top of the existing branch; the branch itself is never recreated.
    /// For new branches, forks from `upstream` and sets it as the tracking branch.
    async fn ensure_worktree(
        repo_root: &Path,
        worktree_base_dir: &Path,
        name: &str,
        upstream: &str,
    ) -> anyhow::Result<PathBuf> {
        let worktree_path = Self::worktree_path_for(worktree_base_dir, name);
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
                        .args([
                            "worktree",
                            "add",
                            "--force",
                            worktree_path.to_str().unwrap(),
                            &branch,
                        ])
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

    /// Check whether the given upstream branch exists in the repo.
    pub fn check_upstream_exists(repo_root: &Path, upstream: &str) -> bool {
        std::process::Command::new("git")
            .args(["rev-parse", "--verify", &format!("refs/heads/{upstream}")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .current_dir(repo_root)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
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
                    !b.is_empty() && !b.starts_with("copse/") && b != "main" && b != "master"
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
                .filter_map(|line| line.strip_prefix("copse/").map(|name| name.to_string()))
                .collect(),
            _ => vec![],
        }
    }

    /// Construct a brand-new Task in Stopped state.
    /// No branch or worktree is created yet; that happens on first launch.
    pub fn new_stopped(
        name: String,
        upstream: String,
        worktree_base_dir: &Path,
        backend: Backend,
        rows: u16,
        cols: u16,
    ) -> Self {
        Task {
            id: uuid::Uuid::new_v4(),
            worktree_path: Self::worktree_path_for(worktree_base_dir, &name),
            name,
            upstream: Some(upstream),
            status: TaskStatus::Stopped,
            has_session: false,
            commits_ahead: None,
            upstream_exists: true,
            parser: Arc::new(Mutex::new(vt100::Parser::new(rows, cols, SCROLLBACK_LEN))),
            waiting_for_input: true,
            waiting_status_dirty: false,
            last_enter_at: None,
            first_waiting_at: None,
            scroll_offset: 0,
            backend,
            session_id: None,
            writer: None,
            _reader_task: None,
            master: None,
            killer: None,
        }
    }

    /// Construct a Task in Stopped state for an existing branch,
    /// without spawning any process. Reads upstream from git tracking branch.
    pub fn from_existing(
        name: String,
        repo_root: &Path,
        worktree_base_dir: &Path,
        backend: Backend,
        rows: u16,
        cols: u16,
    ) -> Self {
        let upstream = Self::load_upstream(repo_root, &name);
        let has_session = Self::session_marker_path_for(worktree_base_dir, &name).exists();
        let commits_ahead = upstream
            .as_ref()
            .and_then(|u| Self::compute_commits_ahead(repo_root, &name, u));
        let upstream_exists = upstream
            .as_ref()
            .is_some_and(|u| commits_ahead.is_some() || Self::check_upstream_exists(repo_root, u));
        Task {
            id: uuid::Uuid::new_v4(),
            worktree_path: Self::worktree_path_for(worktree_base_dir, &name),
            name,
            upstream,
            status: TaskStatus::Stopped,
            has_session,
            commits_ahead,
            upstream_exists,
            parser: Arc::new(Mutex::new(vt100::Parser::new(rows, cols, SCROLLBACK_LEN))),
            waiting_for_input: true,
            waiting_status_dirty: false,
            last_enter_at: None,
            first_waiting_at: None,
            scroll_offset: 0,
            backend,
            session_id: None,
            writer: None,
            _reader_task: None,
            master: None,
            killer: None,
        }
    }

    /// Spawn `claude` in the task's worktree inside a PTY.
    /// If `has_session` is true, passes `--continue` to resume the last session.
    /// The `id` parameter preserves the task's identity across restarts.
    pub async fn spawn(
        params: SpawnParams,
        rows: u16,
        cols: u16,
        event_tx: mpsc::Sender<AppEvent>,
    ) -> anyhow::Result<Self> {
        let SpawnParams {
            id,
            name,
            upstream,
            has_session,
            repo_root,
            worktree_base_dir,
            config,
        } = params;

        // Ensure the worktree (and branch) exist before launching claude
        let worktree_path = Self::ensure_worktree(
            &repo_root,
            &worktree_base_dir,
            &name,
            upstream.as_deref().unwrap_or(""),
        )
        .await?;

        // Set up .claude/settings.local.json based on config
        let wp = worktree_path.clone();
        let backend = config.backend.clone();
        let permission_mode = config.permission_mode.clone();
        tokio::task::spawn_blocking(move || Self::setup_claude_settings(&wp, &config)).await??;

        let session_id = backend.create_session(&crate::backend::SessionParams {
            repo_root: &repo_root,
            task_name: &name,
            worktree_path: &worktree_path,
            cols,
            rows,
            has_session,
            permission_mode: &permission_mode,
        })?;

        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let cmd = backend.build_pty_command(
            session_id.as_deref(),
            &worktree_path,
            has_session,
            &permission_mode,
        );

        let child = pair.slave.spawn_command(cmd)?;
        // The slave side is no longer needed once the child is spawned
        drop(pair.slave);

        let writer = pair.master.take_writer()?;
        let mut reader = pair.master.try_clone_reader()?;
        // Obtain a ChildKiller for later termination
        let killer = child.clone_killer();

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, SCROLLBACK_LEN)));
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
            has_session: false,
            commits_ahead: None,
            upstream_exists: true,
            worktree_path,
            parser,
            waiting_for_input: true,
            waiting_status_dirty: false,
            last_enter_at: None,
            first_waiting_at: None,
            scroll_offset: 0,
            backend,
            session_id,
            writer: Some(writer),
            _reader_task: Some(reader_task),
            master: Some(pair.master),
            killer: Some(killer),
        })
    }

    /// Delete the task: remove the worktree and delete the branch.
    pub fn delete_task(
        repo_root: &Path,
        worktree_base_dir: &Path,
        name: &str,
        backend: &Backend,
    ) -> anyhow::Result<()> {
        // Kill any running backend session for this task
        backend.kill_session(backend.session_id(repo_root, name).as_deref());

        let worktree_path = Self::worktree_path_for(worktree_base_dir, name);
        let branch = Self::branch_name(name);

        // Remove worktree
        let out = std::process::Command::new("git")
            .args([
                "worktree",
                "remove",
                "--force",
                worktree_path.to_str().unwrap_or(""),
            ])
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

        // Clean up session marker
        let _ = std::fs::remove_file(Self::session_marker_path_for(worktree_base_dir, name));

        Ok(())
    }

    /// Fast-forward merge: advance upstream to the task branch's HEAD.
    pub fn merge_ff(repo_root: &Path, name: &str, upstream: &str) -> anyhow::Result<()> {
        let branch = Self::branch_name(name);
        Self::advance_branch(repo_root, upstream, &branch)?;
        Ok(())
    }

    /// Sync task branch to upstream: reset --hard inside the worktree.
    pub fn sync_from_upstream(
        repo_root: &Path,
        worktree_base_dir: &Path,
        name: &str,
        upstream: &str,
    ) -> anyhow::Result<()> {
        let worktree_path = Self::worktree_path_for(worktree_base_dir, name);

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
            anyhow::bail!("reset failed: {}", String::from_utf8_lossy(&out.stderr));
        }

        Ok(())
    }

    /// Send keyboard input to the PTY
    pub fn write_input(&mut self, data: &[u8]) -> anyhow::Result<()> {
        if let Some(writer) = &mut self.writer {
            writer.write_all(data)?;
            if !self.has_session {
                self.has_session = true;
                let _ = std::fs::write(self.session_marker_path(), "");
            }
            // Optimistically clear the waiting flag when Enter is submitted so the
            // list view transitions from "waiting" to "running" immediately.
            // The cursor moves away from the ❯ row after Enter, so the next
            // update_waiting_status scan will not falsely re-detect "waiting"
            // from the echoed input line.
            if data.contains(&b'\r') {
                self.waiting_for_input = false;
                self.last_enter_at = Some(std::time::Instant::now());
                self.first_waiting_at = None;
            }
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
        self.backend
            .resize_session(self.session_id.as_deref(), cols, rows);
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

        worktree_path
            .ok_or_else(|| anyhow::anyhow!("branch '{branch}' is not checked out in any worktree"))
    }

    /// Advance a branch to the given commit.
    /// If the branch is checked out in a worktree, uses `merge --ff-only` to
    /// update the working tree and index atomically.
    /// Otherwise, moves the branch ref directly with `branch -f`.
    pub fn advance_branch(repo_root: &Path, branch: &str, commit: &str) -> anyhow::Result<()> {
        match Self::find_branch_worktree(repo_root, branch) {
            Ok(wt_path) => {
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
            }
            Err(_) => {
                let out = std::process::Command::new("git")
                    .args(["branch", "-f", branch, commit])
                    .current_dir(repo_root)
                    .output()?;
                if !out.status.success() {
                    anyhow::bail!("branch -f failed: {}", String::from_utf8_lossy(&out.stderr));
                }
            }
        }

        Ok(())
    }

    /// Switch the branch checked out in a worktree.
    pub fn switch_branch(worktree: &Path, branch: &str) -> anyhow::Result<()> {
        let out = std::process::Command::new("git")
            .args(["switch", branch])
            .current_dir(worktree)
            .output()?;
        if !out.status.success() {
            anyhow::bail!("switch failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    }

    /// Change the upstream (tracking branch) for a task branch.
    pub fn set_upstream(repo_root: &Path, name: &str, upstream: &str) -> anyhow::Result<()> {
        let branch = Self::branch_name(name);
        let out = std::process::Command::new("git")
            .args(["branch", "--set-upstream-to", upstream, &branch])
            .current_dir(repo_root)
            .output()?;
        if !out.status.success() {
            anyhow::bail!(
                "set-upstream-to failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Ok(())
    }

    /// Detach from the PTY without killing the backend session.
    /// The Claude process continues running in the background.
    /// Kills the PTY child (e.g. `tmux attach-session`) so the reader task
    /// gets EOF and exits cleanly. This does NOT kill the backend session.
    pub fn detach(&mut self) {
        if let Some(killer) = &mut self.killer {
            let _ = killer.kill();
        }
        self.writer = None;
        self._reader_task = None;
        self.master = None;
        self.killer = None;
    }

    /// Forcibly terminate the task
    pub fn kill(&mut self) -> anyhow::Result<()> {
        self.backend.kill_session(self.session_id.as_deref());
        if let Some(killer) = &mut self.killer {
            killer
                .kill()
                .map_err(|e| anyhow::anyhow!("kill failed: {e}"))?;
        }
        Ok(())
    }
}

/// Compute the worktree base directory for a repository.
/// Uses XDG data directory with a ghq-style repo identifier.
/// Example: `~/.local/share/copse/worktrees/github.com/owner/repo`
pub fn worktree_base_dir(repo_root: &Path) -> anyhow::Result<PathBuf> {
    let strategy = etcetera::base_strategy::Xdg::new()
        .map_err(|e| anyhow::anyhow!("Failed to determine XDG data directory: {e}"))?;
    let data_dir = strategy.data_dir();
    Ok(data_dir
        .join("copse")
        .join("worktrees")
        .join(repo_id(repo_root)))
}

/// Derive a ghq-style repository identifier from the origin remote URL.
/// Falls back to the directory name of `repo_root` if no origin remote is configured.
pub(crate) fn repo_id(repo_root: &Path) -> PathBuf {
    if let Some(url) = origin_url(repo_root) {
        if let Some(parsed) = parse_remote_url(&url) {
            return parsed;
        }
    }
    // Fallback: use the directory name
    PathBuf::from(
        repo_root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".to_string()),
    )
}

fn origin_url(repo_root: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if output.status.success() {
        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !url.is_empty() {
            Some(url)
        } else {
            None
        }
    } else {
        None
    }
}

/// Parse a git remote URL into a ghq-style path (e.g. `github.com/owner/repo`).
/// Supports SSH (`git@host:owner/repo.git`) and HTTPS (`https://host/owner/repo.git`).
fn parse_remote_url(url: &str) -> Option<PathBuf> {
    let path_str = if let Some(rest) = url.strip_prefix("git@") {
        // SSH: git@github.com:owner/repo.git
        rest.replacen(':', "/", 1)
    } else if url.starts_with("https://") || url.starts_with("http://") || url.starts_with("ssh://")
    {
        // HTTPS/SSH: https://github.com/owner/repo.git
        let after_scheme = url.split("://").nth(1)?;
        // Strip user@ prefix (e.g. ssh://git@github.com/...)
        match after_scheme.split_once('@') {
            Some((_, rest)) => rest.to_string(),
            None => after_scheme.to_string(),
        }
    } else {
        return None;
    };
    // Strip trailing .git
    let path_str = path_str.strip_suffix(".git").unwrap_or(&path_str);
    // Strip trailing /
    let path_str = path_str.strip_suffix('/').unwrap_or(path_str);
    if path_str.is_empty() {
        return None;
    }
    Some(PathBuf::from(path_str))
}

/// Returns true if `line` looks like Claude Code's processing spinner.
///
/// Claude Code renders spinner frames as a Unicode dingbat/symbol character
/// followed by a verb ending in "ing…", e.g. `✢ Simmering…` or `✽ Thinking…`.
/// These characters fall in the Miscellaneous Symbols (U+2600–U+26FF) and
/// Dingbats (U+2700–U+27FF) Unicode blocks.
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

/// Result of `detect_waiting`: what the PTY screen currently indicates.
#[derive(Debug, PartialEq)]
enum WaitingDetection {
    /// Clear waiting signal: `❯` with cursor on its row, or "esc to cancel".
    Waiting,
    /// Clear running signal: spinner line or "esc to interrupt".
    Running,
    /// No recognisable indicator (e.g. completion list pushed ❯ out of scan range,
    /// or blank area between renders). Caller should maintain the current state.
    Unknown,
}

/// Pure pattern-matching logic for detecting whether Claude is waiting for input.
/// Inspects the last few non-empty PTY lines (text + whether the cursor is on that row).
///
/// Returns:
/// - `Running`  — spinner or "esc to interrupt" found
/// - `Waiting`  — "esc to cancel" or `❯` with cursor found (and no running signal)
/// - `Unknown`  — neither signal found; caller should keep current state unchanged
///
/// The cursor check prevents echoed input (`❯ test` on a row the cursor has
/// left) from being mis-detected as a waiting prompt after Enter is pressed.
/// `Unknown` handles the case where a completion list (after typing `/`) pushes
/// the `❯` row out of the scan window: we neither accept nor reject waiting.
fn detect_waiting(lines: &[(String, bool)]) -> WaitingDetection {
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
        assert_eq!(detect_waiting(&l), WaitingDetection::Running);
    }

    #[test]
    fn busy_when_ctrl_c_to_interrupt() {
        let l = nc(&["Working...", "ctrl+c to interrupt"]);
        assert_eq!(detect_waiting(&l), WaitingDetection::Running);
    }

    #[test]
    fn busy_takes_priority_over_prompt() {
        let l = with_cursor(&["❯", "esc to interrupt"]);
        assert_eq!(detect_waiting(&l), WaitingDetection::Running);
    }

    #[test]
    fn busy_when_spinner() {
        let l = with_cursor(&["❯", "✢ Simmering\u{2026}"]);
        assert_eq!(detect_waiting(&l), WaitingDetection::Running);
    }

    #[test]
    fn busy_when_spinner_various_chars() {
        let l = nc(&["✽ Thinking\u{2026}"]);
        assert_eq!(detect_waiting(&l), WaitingDetection::Running);
    }

    #[test]
    fn waiting_when_esc_to_cancel() {
        let l = nc(&["Do you want to proceed?", "esc to cancel"]);
        assert_eq!(detect_waiting(&l), WaitingDetection::Waiting);
    }

    #[test]
    fn waiting_when_prompt() {
        // cursor is on the ❯ row
        let l = with_cursor(&["❯"]);
        assert_eq!(detect_waiting(&l), WaitingDetection::Waiting);
    }

    #[test]
    fn waiting_when_typing_at_prompt() {
        // user is mid-input: ❯ line with typed text, cursor still on that row
        let l = with_cursor(&["❯ test"]);
        assert_eq!(detect_waiting(&l), WaitingDetection::Waiting);
    }

    #[test]
    fn unknown_when_prompt_without_cursor() {
        // echoed ❯ line after Enter: cursor has moved away → unknown (maintain state)
        let l = nc(&["❯ test"]);
        assert_eq!(detect_waiting(&l), WaitingDetection::Unknown);
    }

    #[test]
    fn unknown_when_empty() {
        let l: Vec<(String, bool)> = vec![];
        assert_eq!(detect_waiting(&l), WaitingDetection::Unknown);
    }

    #[test]
    fn unknown_without_indicators() {
        // completion list items or plain output: no clear signal
        let l = nc(&["some output", "more output"]);
        assert_eq!(detect_waiting(&l), WaitingDetection::Unknown);
    }

    #[test]
    fn unknown_slash_completion_list() {
        // `/` completion pushed ❯ out of scan range; only completion items visible
        let l = nc(&["/review", "/chat", "/help", "/commit"]);
        assert_eq!(detect_waiting(&l), WaitingDetection::Unknown);
    }

    #[test]
    fn gt_no_longer_triggers_waiting() {
        let l = nc(&["> quoted text in output"]);
        assert_eq!(detect_waiting(&l), WaitingDetection::Unknown);
    }

    #[test]
    fn parse_ssh_remote_url() {
        assert_eq!(
            parse_remote_url("git@github.com:daiwahome/copse.git"),
            Some(PathBuf::from("github.com/daiwahome/copse"))
        );
    }

    #[test]
    fn parse_https_remote_url() {
        assert_eq!(
            parse_remote_url("https://github.com/daiwahome/copse.git"),
            Some(PathBuf::from("github.com/daiwahome/copse"))
        );
    }

    #[test]
    fn parse_https_remote_url_without_dot_git() {
        assert_eq!(
            parse_remote_url("https://github.com/daiwahome/copse"),
            Some(PathBuf::from("github.com/daiwahome/copse"))
        );
    }

    #[test]
    fn parse_ssh_scheme_remote_url() {
        assert_eq!(
            parse_remote_url("ssh://git@github.com/daiwahome/copse.git"),
            Some(PathBuf::from("github.com/daiwahome/copse"))
        );
    }

    #[test]
    fn parse_invalid_remote_url() {
        assert_eq!(parse_remote_url("not-a-url"), None);
    }
}
