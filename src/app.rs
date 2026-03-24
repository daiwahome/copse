use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    config::Config,
    event::{AppEvent, TaskId},
    task::Task,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    /// Full-screen task list
    Tasks,
    /// Agent view: all keystrokes forwarded to PTY.
    /// `full: false` — split view (list + agent side-by-side)
    /// `full: true`  — full-screen PTY
    /// Ctrl+] returns to Tasks. Shift+O toggles split/full.
    Agent { full: bool },
    /// Overlay dialog: entering a name for a new task
    NewTask { input: String },
    /// Overlay dialog: selecting the upstream branch for a new task
    NewTaskUpstream { name: String, branches: Vec<String>, selected: usize },
    /// Overlay dialog: confirming quit when running tasks exist
    ConfirmQuit,
    /// Overlay dialog: confirming Ctrl+K kill of the selected task
    ConfirmKill,
    /// Overlay dialog: confirming Shift+D delete of the selected task
    ConfirmDelete,
    /// Overlay dialog: confirming Shift+S sync from upstream
    ConfirmSync,
    /// Overlay dialog: Shift+M merge into upstream ([f]f / [s]quash)
    ConfirmMerge,
}

pub struct App {
    pub mode: Mode,
    pub tasks: Vec<Task>,
    pub selected_index: usize,
    pub focused_task: Option<TaskId>,
    pub should_quit: bool,
    pub repo_root: std::path::PathBuf,
    /// The common .git directory (shared across all worktrees).
    /// Used to place copse-worktrees/ in a stable location even when
    /// copse itself is running from inside a worktree.
    pub git_common_dir: std::path::PathBuf,
    pub config: Config,
    pub event_tx: tokio::sync::mpsc::Sender<AppEvent>,
    /// Error message to display in the status bar (cleared on next keypress)
    pub last_error: Option<String>,
}

impl App {
    pub fn new(
        repo_root: std::path::PathBuf,
        git_common_dir: std::path::PathBuf,
        config: Config,
        event_tx: tokio::sync::mpsc::Sender<AppEvent>,
    ) -> Self {
        Self {
            mode: Mode::Tasks,
            tasks: Vec::new(),
            selected_index: 0,
            focused_task: None,
            should_quit: false,
            repo_root,
            git_common_dir,
            config,
            event_tx,
            last_error: None,
        }
    }

    pub fn selected_task(&self) -> Option<&Task> {
        self.tasks.get(self.selected_index)
    }

    pub fn focused_task(&self) -> Option<&Task> {
        let id = self.focused_task?;
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn focused_task_mut(&mut self) -> Option<&mut Task> {
        let id = self.focused_task?;
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    pub fn handle_event(&mut self, event: AppEvent) -> anyhow::Result<()> {
        match event {
            AppEvent::Key(key) => self.handle_key(key)?,
            AppEvent::TaskOutput(_id) => {
                // Parser already updated; the event loop will redraw.
            }
            AppEvent::TaskCreated(task) => {
                // New task added as Stopped — worktree created on first launch
                self.selected_index = self.tasks.len();
                self.tasks.push(task);
            }
            AppEvent::TaskResumed { id, result } => match result {
                Ok(task) => {
                    // Find the placeholder task by ID and replace it
                    if let Some(pos) = self.tasks.iter().position(|t| t.id == id) {
                        self.tasks[pos] = task;
                        self.selected_index = pos;
                    } else {
                        self.selected_index = self.tasks.len();
                        self.tasks.push(task);
                    }
                    self.focused_task = Some(id);
                    self.mode = Mode::Agent { full: false };
                    self.sync_pty_size();
                }
                Err(e) => {
                    self.last_error = Some(e);
                }
            },
            AppEvent::TaskExited(id) => {
                // Mark as Stopped but keep the task in the list
                if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
                    task.status = crate::task::TaskStatus::Stopped;
                    task.commits_ahead = Task::compute_commits_ahead(
                        &self.repo_root, &task.name, &task.upstream,
                    );
                }
                // When the focused task exits while in agent view, close the
                // view and return to Tasks so the user can act immediately.
                if self.focused_task == Some(id) {
                    self.focused_task = None;
                    if matches!(self.mode, Mode::Agent { .. }) {
                        self.mode = Mode::Tasks;
                    }
                }
            }
            AppEvent::Resize { cols, rows } => {
                // Reserve 1 row for the status bar so the PTY size matches the
                // visible area and full-screen TUI programs render correctly.
                let content_rows = rows.saturating_sub(1);
                // In split view the agent pane is narrower than the full terminal.
                // Use the same list_width formula as ui/mod.rs to compute agent cols.
                let agent_cols = if matches!(self.mode, Mode::Agent { full: false, .. }) {
                    let list_width = (cols / 2).max(20);
                    cols.saturating_sub(list_width + 1) // +1 for divider
                } else {
                    cols
                };
                for task in &mut self.tasks {
                    let _ = task.resize(content_rows, agent_cols);
                }
            }
            AppEvent::TaskDeleted(id) => {
                if let Some(pos) = self.tasks.iter().position(|t| t.id == id) {
                    self.tasks.remove(pos);
                    if self.selected_index >= self.tasks.len() && !self.tasks.is_empty() {
                        self.selected_index = self.tasks.len() - 1;
                    }
                }
                self.refresh_commits_ahead();
            }
            AppEvent::GitOpResult(result) => match result {
                Ok(()) => {
                    self.refresh_commits_ahead();
                }
                Err(msg) => {
                    self.last_error = Some(msg);
                }
            },
            AppEvent::SquashMerge { .. } => {
                // Handled directly by Tui::run (needs alternate screen exit)
            }
            AppEvent::FatalError(msg) => {
                self.last_error = Some(msg);
                self.should_quit = true;
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        self.last_error = None;
        match &self.mode {
            Mode::Tasks => self.handle_tasks_key(key)?,
            Mode::Agent { .. } => self.handle_agent_key(key)?,
            Mode::NewTask { .. } => self.handle_new_task_key(key)?,
            Mode::NewTaskUpstream { .. } => self.handle_new_task_upstream_key(key)?,
            Mode::ConfirmQuit => self.handle_confirm_quit_key(key)?,
            Mode::ConfirmKill => self.handle_confirm_kill_key(key)?,
            Mode::ConfirmDelete => self.handle_confirm_delete_key(key)?,
            Mode::ConfirmSync => self.handle_confirm_sync_key(key)?,
            Mode::ConfirmMerge => self.handle_confirm_merge_key(key)?,
        }
        Ok(())
    }

    /// Refresh commits_ahead for all tasks.
    pub fn refresh_commits_ahead(&mut self) {
        for task in &mut self.tasks {
            task.commits_ahead = Task::compute_commits_ahead(
                &self.repo_root, &task.name, &task.upstream,
            );
        }
    }

    fn handle_tasks_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        // Ctrl+R: refresh
        if key.code == KeyCode::Char('r') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.refresh_commits_ahead();
            return Ok(());
        }
        // Ctrl+K must be checked before the bare 'k' pattern below
        if key.code == KeyCode::Char('k') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if let Some(task) = self.tasks.get(self.selected_index) {
                if task.status == crate::task::TaskStatus::Running {
                    self.mode = Mode::ConfirmKill;
                }
            }
            return Ok(());
        }

        match key.code {
            // q: confirm if running tasks exist, otherwise quit immediately
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                let has_running = self
                    .tasks
                    .iter()
                    .any(|t| t.status == crate::task::TaskStatus::Running);
                if has_running {
                    self.mode = Mode::ConfirmQuit;
                } else {
                    self.should_quit = true;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.tasks.is_empty() {
                    self.selected_index = (self.selected_index + 1).min(self.tasks.len() - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if !self.tasks.is_empty() {
                    self.selected_index = self.selected_index.saturating_sub(1);
                }
            }
            // Enter: open agent view (split), like selecting a commit in tig
            KeyCode::Enter => {
                if let Some(task) = self.tasks.get(self.selected_index) {
                    match task.status {
                        crate::task::TaskStatus::Running => {
                            self.focused_task = Some(task.id);
                            self.mode = Mode::Agent { full: false };
                            self.sync_pty_size();
                        }
                        crate::task::TaskStatus::Stopped => {
                            self.resume_task(self.selected_index);
                        }
                    }
                }
            }
            KeyCode::Char('n') => {
                self.mode = Mode::NewTask {
                    input: String::new(),
                };
            }
            // Shift+M: merge into upstream (Stopped only)
            KeyCode::Char('M') => {
                if let Some(task) = self.tasks.get(self.selected_index) {
                    if task.status == crate::task::TaskStatus::Stopped {
                        self.mode = Mode::ConfirmMerge;
                    } else {
                        self.last_error = Some("Stop the task before merging".to_string());
                    }
                }
            }
            // Shift+S: sync from upstream (Stopped only)
            KeyCode::Char('S') => {
                if let Some(task) = self.tasks.get(self.selected_index) {
                    if task.status == crate::task::TaskStatus::Stopped {
                        self.mode = Mode::ConfirmSync;
                    } else {
                        self.last_error = Some("Stop the task before syncing".to_string());
                    }
                }
            }
            // !: delete task (Stopped only, tig-style)
            KeyCode::Char('!') => {
                if let Some(task) = self.tasks.get(self.selected_index) {
                    if task.status == crate::task::TaskStatus::Stopped {
                        self.mode = Mode::ConfirmDelete;
                    } else {
                        self.last_error = Some("Stop the task before deleting".to_string());
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_agent_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        let Mode::Agent { full } = self.mode else {
            return Ok(());
        };

        // Ctrl+]: in fullscreen → return to split, in split → return to Tasks
        // crossterm maps 0x1D (Ctrl+]) → KeyCode::Char('5') + CONTROL
        if key.code == KeyCode::Char('5') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if full {
                self.mode = Mode::Agent { full: false };
                self.sync_pty_size();
            } else {
                self.focused_task = None;
                self.mode = Mode::Tasks;
            }
            return Ok(());
        }

        // TODO: Shift+O (maximize) is disabled because it captures 'O' keypresses
        // meant for the PTY (e.g. vim's O command). Need a key that doesn't conflict
        // with claude code's key bindings. See docs/en/design-decisions.md.

        // Ctrl+B: scroll up one page, Ctrl+F: scroll down one page
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('b') => {
                    if let Some(task) = self.focused_task_mut() {
                        let page = crossterm::terminal::size()
                            .map(|(_, r)| r.saturating_sub(2) as usize)
                            .unwrap_or(20);
                        let new_offset = task.scroll_offset.saturating_add(page);
                        let mut screen = task.parser.lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .screen().clone();
                        screen.set_scrollback(new_offset);
                        task.scroll_offset = screen.scrollback();
                    }
                    return Ok(());
                }
                KeyCode::Char('f') => {
                    if let Some(task) = self.focused_task_mut() {
                        let page = crossterm::terminal::size()
                            .map(|(_, r)| r.saturating_sub(2) as usize)
                            .unwrap_or(20);
                        task.scroll_offset = task.scroll_offset.saturating_sub(page);
                    }
                    return Ok(());
                }
                _ => {}
            }
        }

        // Everything else: reset scroll and forward to PTY
        let bytes = key_to_bytes(key);
        if !bytes.is_empty() {
            if let Some(task) = self.focused_task_mut() {
                task.scroll_offset = 0;
                if task.status == crate::task::TaskStatus::Running {
                    let _ = task.write_input(&bytes);
                }
            }
        }
        Ok(())
    }

    /// Resize all PTYs to match the current mode's effective dimensions.
    /// In split view the agent pane is narrower than the full terminal;
    /// call this whenever the mode changes between split and full-screen.
    fn sync_pty_size(&mut self) {
        let (cols, rows) = crossterm::terminal::size().unwrap_or((200, 50));
        let content_rows = rows.saturating_sub(1); // reserve 1 row for status bar
        let agent_cols = if matches!(self.mode, Mode::Agent { full: false, .. }) {
            let list_width = (cols / 2).max(20);
            cols.saturating_sub(list_width + 1)
        } else {
            cols
        };
        for task in &mut self.tasks {
            let _ = task.resize(content_rows, agent_cols);
        }
    }

    fn resume_task(&mut self, index: usize) {
        let Some(task) = self.tasks.get(index) else {
            return;
        };
        let id = task.id;
        let name = task.name.clone();
        let upstream = task.upstream.clone();
        let has_run = task.has_run;
        let tx = self.event_tx.clone();
        let repo_root = self.repo_root.clone();
        let git_common_dir = self.git_common_dir.clone();
        let config = self.config.clone();
        let (cols, rows) = crossterm::terminal::size().unwrap_or((200, 50));
        let content_rows = rows.saturating_sub(1); // reserve 1 row for status bar
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let result = Task::spawn(id, name, upstream, has_run, repo_root, git_common_dir, config, content_rows, cols, tx)
                .await
                .map_err(|e| format!("Failed to resume task: {e}"));
            let _ = event_tx
                .send(crate::event::AppEvent::TaskResumed { id, result })
                .await;
        });
    }

    fn handle_confirm_kill_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(task) = self.tasks.get_mut(self.selected_index) {
                    if let Err(e) = task.kill() {
                        self.last_error = Some(format!("Failed to kill task: {e}"));
                    }
                    // task stays in the list as Stopped (TaskExited event will arrive)
                }
                self.mode = Mode::Tasks;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Tasks;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_confirm_merge_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Char('f') => {
                // Fast-forward merge
                let repo_root = self.repo_root.clone();
                if let Some(task) = self.tasks.get(self.selected_index) {
                    let name = task.name.clone();
                    let upstream = task.upstream.clone();
                    let event_tx = self.event_tx.clone();
                    tokio::spawn(async move {
                        let result = tokio::task::spawn_blocking(move || {
                            Task::merge_ff(&repo_root, &name, &upstream)
                        })
                        .await;
                        match result {
                            Ok(Ok(())) => {
                                let _ = event_tx.send(AppEvent::GitOpResult(Ok(()))).await;
                            }
                            Ok(Err(e)) => {
                                let _ = event_tx
                                    .send(AppEvent::GitOpResult(Err(format!("FF merge failed: {e}"))))
                                    .await;
                            }
                            Err(e) => {
                                let _ = event_tx
                                    .send(AppEvent::GitOpResult(Err(format!("Merge error: {e}"))))
                                    .await;
                            }
                        }
                    });
                }
                self.mode = Mode::Tasks;
            }
            KeyCode::Char('s') => {
                // Squash merge — needs alternate screen exit for $EDITOR
                if let Some(task) = self.tasks.get(self.selected_index) {
                    let name = task.name.clone();
                    let upstream = task.upstream.clone();
                    let tx = self.event_tx.clone();
                    let _ = tx.try_send(AppEvent::SquashMerge { name, upstream });
                }
                self.mode = Mode::Tasks;
            }
            KeyCode::Esc => {
                self.mode = Mode::Tasks;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_confirm_sync_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let repo_root = self.repo_root.clone();
                let git_common_dir = self.git_common_dir.clone();
                if let Some(task) = self.tasks.get(self.selected_index) {
                    let name = task.name.clone();
                    let upstream = task.upstream.clone();
                    let event_tx = self.event_tx.clone();
                    tokio::spawn(async move {
                        let result = tokio::task::spawn_blocking(move || {
                            Task::sync_from_upstream(&repo_root, &git_common_dir, &name, &upstream)
                        })
                        .await;
                        match result {
                            Ok(Ok(())) => {
                                let _ = event_tx.send(AppEvent::GitOpResult(Ok(()))).await;
                            }
                            Ok(Err(e)) => {
                                let _ = event_tx
                                    .send(AppEvent::GitOpResult(Err(format!("Sync failed: {e}"))))
                                    .await;
                            }
                            Err(e) => {
                                let _ = event_tx
                                    .send(AppEvent::GitOpResult(Err(format!("Sync error: {e}"))))
                                    .await;
                            }
                        }
                    });
                }
                self.mode = Mode::Tasks;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Tasks;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_confirm_delete_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let repo_root = self.repo_root.clone();
                let git_common_dir = self.git_common_dir.clone();
                if let Some(task) = self.tasks.get(self.selected_index) {
                    let id = task.id;
                    let name = task.name.clone();
                    let event_tx = self.event_tx.clone();
                    tokio::spawn(async move {
                        let result = tokio::task::spawn_blocking(move || {
                            Task::delete_task(&repo_root, &git_common_dir, &name)
                        })
                        .await;
                        match result {
                            Ok(Ok(())) => {
                                let _ = event_tx.send(AppEvent::TaskDeleted(id)).await;
                            }
                            Ok(Err(e)) => {
                                let _ = event_tx
                                    .send(AppEvent::GitOpResult(Err(format!(
                                        "Delete failed: {e}"
                                    ))))
                                    .await;
                            }
                            Err(e) => {
                                let _ = event_tx
                                    .send(AppEvent::GitOpResult(Err(format!(
                                        "Delete task error: {e}"
                                    ))))
                                    .await;
                            }
                        }
                    });
                }
                self.mode = Mode::Tasks;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Tasks;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_confirm_quit_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.should_quit = true;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Tasks;
            }
            _ => {}
        }
        Ok(())
    }

    fn get_current_branch(&self) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&self.repo_root)
            .output()
            .ok()?;
        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if branch.is_empty() || branch == "HEAD" {
                None
            } else {
                Some(branch)
            }
        } else {
            None
        }
    }

    /// Check if a task name is valid as a git branch name suffix.
    fn is_valid_task_name(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        // Reject names that would cause problems in git branch names or paths
        if name == "." || name == ".." || name.starts_with('-') {
            return false;
        }
        // Only allow characters safe for git branch names
        name.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'
        })
    }

    fn handle_new_task_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        let Mode::NewTask { input } = &mut self.mode else {
            return Ok(());
        };
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Tasks;
            }
            KeyCode::Enter => {
                let name = input.trim().to_string();
                if name.is_empty() {
                    self.mode = Mode::Tasks;
                } else if !Self::is_valid_task_name(&name) {
                    self.last_error = Some("Invalid task name (no spaces, .., or special chars)".to_string());
                    // Stay in NewTask mode so user can fix the name
                } else if self.tasks.iter().any(|t| t.name == name) {
                    self.last_error = Some(format!("Task '{name}' already exists"));
                } else {
                    let branches = Task::list_upstream_candidates(&self.repo_root);
                    if branches.is_empty() {
                        self.last_error = Some("No eligible upstream branches found".to_string());
                        self.mode = Mode::Tasks;
                    } else {
                        // Pre-select the current branch if it's in the list
                        let current = self.get_current_branch().unwrap_or_default();
                        let selected = branches.iter()
                            .position(|b| b == &current)
                            .unwrap_or(0);
                        self.mode = Mode::NewTaskUpstream {
                            name,
                            branches,
                            selected,
                        };
                    }
                }
            }
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Char(c) => {
                input.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_new_task_upstream_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        let Mode::NewTaskUpstream { name, branches, selected } = &mut self.mode else {
            return Ok(());
        };
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Tasks;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !branches.is_empty() {
                    *selected = (*selected + 1) % branches.len();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if !branches.is_empty() {
                    *selected = selected.checked_sub(1).unwrap_or(branches.len() - 1);
                }
            }
            KeyCode::Enter => {
                let upstream = branches[*selected].clone();
                let name = name.clone();
                self.mode = Mode::Tasks;
                let (cols, rows) = crossterm::terminal::size().unwrap_or((200, 50));
                let content_rows = rows.saturating_sub(1);
                let task = Task::new_stopped(name, upstream, &self.git_common_dir, content_rows, cols);
                let tx = self.event_tx.clone();
                let _ = tx.try_send(crate::event::AppEvent::TaskCreated(task));
            }
            _ => {}
        }
        Ok(())
    }
}

fn key_to_bytes(key: KeyEvent) -> Vec<u8> {
    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let lower = c.to_ascii_lowercase();
                if lower.is_ascii_alphabetic() {
                    // Ctrl+a..z → 0x01..0x1a
                    vec![lower as u8 - b'a' + 1]
                } else {
                    // crossterm maps 0x1C..=0x1F → Char('4'..='7') + CONTROL
                    // so we reverse that mapping here for PTY forwarding
                    match c {
                        '4' => vec![0x1c], // Ctrl+\ (FS)
                        '5' => vec![0x1d], // Ctrl+] (GS) — also used as back key
                        '6' => vec![0x1e], // Ctrl+^ (RS)
                        '7' => vec![0x1f], // Ctrl+_ (US)
                        _ => vec![],
                    }
                }
            } else {
                let mut buf = [0u8; 4];
                c.encode_utf8(&mut buf).as_bytes().to_vec()
            }
        }
        KeyCode::Enter => b"\r".to_vec(),
        KeyCode::Backspace => b"\x7f".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Tab => b"\t".to_vec(),
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Esc => b"\x1b".to_vec(),
        KeyCode::F(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => vec![],
        },
        _ => vec![],
    }
}
