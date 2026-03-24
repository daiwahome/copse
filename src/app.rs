use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    config::Config,
    diff::DiffState,
    event::{AppEvent, TaskId},
    task::Task,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pane {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum View {
    Tasks,
    Diff,
    Agent,
}

impl View {
    pub fn priority(self) -> u8 {
        match self {
            View::Tasks => 0,
            View::Diff => 1,
            View::Agent => 2,
        }
    }
}

pub enum ChildView {
    Diff(DiffState),
    Agent(TaskId),
}

impl ChildView {
    pub fn view(&self) -> View {
        match self {
            ChildView::Diff(_) => View::Diff,
            ChildView::Agent(_) => View::Agent,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Dialog {
    NewTask { input: String },
    NewTaskUpstream { name: String, branches: Vec<String>, selected: usize },
    ConfirmQuit,
    ConfirmKill,
    ConfirmDelete,
    ConfirmSync,
    ConfirmMerge,
    ChangeUpstream { branches: Vec<String>, selected: usize },
    DiffSearch { input: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ViewLayout {
    Single(View),
    Split(View, View),
    Fullscreen(View),
}

pub struct App {
    pub tasks: Vec<Task>,
    pub selected_index: usize,
    pub view_stack: Vec<ChildView>,
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
    /// Which view is fullscreen (None = split/auto layout)
    pub fullscreen: Option<View>,
    /// Which pane has focus in split view
    pub focus: Pane,
    /// Active dialog overlay (independent of layout)
    pub dialog: Option<Dialog>,
}

impl App {
    pub fn new(
        repo_root: std::path::PathBuf,
        git_common_dir: std::path::PathBuf,
        config: Config,
        event_tx: tokio::sync::mpsc::Sender<AppEvent>,
    ) -> Self {
        Self {
            tasks: Vec::new(),
            selected_index: 0,
            view_stack: Vec::new(),
            should_quit: false,
            repo_root,
            git_common_dir,
            config,
            event_tx,
            last_error: None,
            fullscreen: None,
            focus: Pane::Left,
            dialog: None,
        }
    }

    /// Compute the current layout from child view state.
    pub fn layout(&self) -> ViewLayout {
        if let Some(fs) = self.fullscreen {
            if fs == View::Tasks || self.has_view(fs) {
                return ViewLayout::Fullscreen(fs);
            }
        }
        let mut views: Vec<View> = vec![View::Tasks];
        views.extend(self.view_stack.iter().map(|v| v.view()));
        views.sort_by_key(|v| v.priority());
        let n = views.len();
        match n {
            1 => ViewLayout::Single(views[0]),
            _ => ViewLayout::Split(views[n - 2], views[n - 1]),
        }
    }

    /// Which view currently has focus.
    pub fn focused_view(&self) -> View {
        match self.layout() {
            ViewLayout::Fullscreen(v) | ViewLayout::Single(v) => v,
            ViewLayout::Split(left, _) if self.focus == Pane::Left => left,
            ViewLayout::Split(_, right) => right,
        }
    }

    pub fn selected_task(&self) -> Option<&Task> {
        self.tasks.get(self.selected_index)
    }

    pub fn diff_state(&self) -> Option<&DiffState> {
        self.view_stack.iter().find_map(|v| match v {
            ChildView::Diff(s) => Some(s),
            _ => None,
        })
    }

    pub fn diff_state_mut(&mut self) -> Option<&mut DiffState> {
        self.view_stack.iter_mut().find_map(|v| match v {
            ChildView::Diff(s) => Some(s),
            _ => None,
        })
    }

    pub fn focused_task_id(&self) -> Option<TaskId> {
        self.view_stack.iter().find_map(|v| match v {
            ChildView::Agent(id) => Some(*id),
            _ => None,
        })
    }

    pub fn has_view(&self, view: View) -> bool {
        self.view_stack.iter().any(|v| v.view() == view)
    }

    pub fn focused_task(&self) -> Option<&Task> {
        let id = self.focused_task_id()?;
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn focused_task_mut(&mut self) -> Option<&mut Task> {
        let id = self.focused_task_id()?;
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    fn push_diff(&mut self, state: DiffState) {
        self.view_stack.retain(|v| v.view() != View::Diff);
        self.view_stack.push(ChildView::Diff(state));
    }

    fn push_agent(&mut self, task_id: TaskId) {
        self.view_stack.retain(|v| v.view() != View::Agent);
        self.view_stack.push(ChildView::Agent(task_id));
    }

    fn update_diff(&mut self, state: DiffState) {
        if let Some(existing) = self.view_stack.iter_mut().find(|v| v.view() == View::Diff) {
            *existing = ChildView::Diff(state);
        } else {
            self.view_stack.push(ChildView::Diff(state));
        }
    }

    pub fn handle_event(&mut self, event: AppEvent) -> anyhow::Result<()> {
        match event {
            AppEvent::Key(key) => self.handle_key(key)?,
            AppEvent::TaskOutput(id) => {
                // Parser already updated; mark dirty for deferred status refresh.
                if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
                    task.waiting_status_dirty = true;
                }
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
                    self.push_agent(id);
                    self.focus = Pane::Right;
                    self.fullscreen = None;
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
                    task.waiting_for_input = false;
                    task.waiting_status_dirty = false;
                    task.commits_ahead = Task::compute_commits_ahead(
                        &self.repo_root, &task.name, &task.upstream,
                    );
                }
                if self.focused_task_id() == Some(id) {
                    self.close_agent();
                }
            }
            AppEvent::Resize { cols, rows } => {
                let content_rows = rows.saturating_sub(1);
                let agent_cols = match self.layout() {
                    ViewLayout::Split(_, View::Agent) => {
                        let list_width = (cols / 2).max(20);
                        cols.saturating_sub(list_width + 1)
                    }
                    _ => cols,
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
        // Dialog first
        if self.dialog.is_some() {
            return self.handle_dialog_key(key);
        }
        // Ctrl+W: toggle focus (split only)
        if key.code == KeyCode::Char('w') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if matches!(self.layout(), ViewLayout::Split(_, _)) {
                self.focus = match self.focus {
                    Pane::Left => Pane::Right,
                    Pane::Right => Pane::Left,
                };
            }
            return Ok(());
        }
        // Dispatch to focused view
        match self.focused_view() {
            View::Tasks => self.handle_tasks_key(key)?,
            View::Diff => self.handle_diff_key(key)?,
            View::Agent => self.handle_agent_key(key)?,
        }
        Ok(())
    }

    /// Refresh cached waiting status for tasks marked dirty.
    /// Called once before each draw to avoid redundant PTY scans.
    pub fn flush_waiting_status(&mut self) {
        for task in &mut self.tasks {
            if task.waiting_status_dirty {
                task.update_waiting_status();
                task.waiting_status_dirty = false;
            }
        }
    }

    /// Refresh commits_ahead for all tasks.
    pub fn refresh_commits_ahead(&mut self) {
        for task in &mut self.tasks {
            task.commits_ahead = Task::compute_commits_ahead(
                &self.repo_root, &task.name, &task.upstream,
            );
        }
    }

    fn handle_dialog_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        let dialog = self.dialog.take().unwrap();
        match dialog {
            Dialog::NewTask { input } => self.handle_new_task_dialog(key, input),
            Dialog::NewTaskUpstream { name, branches, selected } => self.handle_new_task_upstream_dialog(key, name, branches, selected),
            Dialog::ConfirmQuit => self.handle_confirm_quit_dialog(key),
            Dialog::ConfirmKill => self.handle_confirm_kill_dialog(key),
            Dialog::ConfirmDelete => self.handle_confirm_delete_dialog(key),
            Dialog::ConfirmSync => self.handle_confirm_sync_dialog(key),
            Dialog::ConfirmMerge => self.handle_confirm_merge_dialog(key),
            Dialog::ChangeUpstream { branches, selected } => self.handle_change_upstream_dialog(key, branches, selected),
            Dialog::DiffSearch { input } => self.handle_diff_search_dialog(key, input),
        }
    }

    fn handle_new_task_dialog(&mut self, key: KeyEvent, mut input: String) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Esc => {}
            KeyCode::Enter => {
                let name = input.trim().to_string();
                if name.is_empty() {
                } else if !Self::is_valid_task_name(&name) {
                    self.last_error = Some("Invalid task name (no spaces, .., or special chars)".to_string());
                    self.dialog = Some(Dialog::NewTask { input });
                } else if self.tasks.iter().any(|t| t.name == name) {
                    self.last_error = Some(format!("Task '{name}' already exists"));
                    self.dialog = Some(Dialog::NewTask { input });
                } else {
                    let branches = Task::list_upstream_candidates(&self.repo_root);
                    if branches.is_empty() {
                        self.last_error = Some("No eligible upstream branches found".to_string());
                    } else {
                        let current = self.get_current_branch().unwrap_or_default();
                        let selected = branches.iter().position(|b| b == &current).unwrap_or(0);
                        self.dialog = Some(Dialog::NewTaskUpstream { name, branches, selected });
                    }
                }
            }
            KeyCode::Backspace => { input.pop(); self.dialog = Some(Dialog::NewTask { input }); }
            KeyCode::Char(c) => { input.push(c); self.dialog = Some(Dialog::NewTask { input }); }
            _ => { self.dialog = Some(Dialog::NewTask { input }); }
        }
        Ok(())
    }

    fn handle_new_task_upstream_dialog(&mut self, key: KeyEvent, name: String, branches: Vec<String>, mut selected: usize) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Esc => {}
            KeyCode::Char('j') | KeyCode::Down => {
                if !branches.is_empty() { selected = (selected + 1) % branches.len(); }
                self.dialog = Some(Dialog::NewTaskUpstream { name, branches, selected });
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if !branches.is_empty() { selected = selected.checked_sub(1).unwrap_or(branches.len() - 1); }
                self.dialog = Some(Dialog::NewTaskUpstream { name, branches, selected });
            }
            KeyCode::Enter => {
                let upstream = branches[selected].clone();
                let (cols, rows) = crossterm::terminal::size().unwrap_or((200, 50));
                let content_rows = rows.saturating_sub(1);
                let task = Task::new_stopped(name, upstream, &self.git_common_dir, content_rows, cols);
                let _ = self.event_tx.try_send(AppEvent::TaskCreated(task));
            }
            _ => { self.dialog = Some(Dialog::NewTaskUpstream { name, branches, selected }); }
        }
        Ok(())
    }

    fn handle_confirm_quit_dialog(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => { self.should_quit = true; }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {}
            _ => { self.dialog = Some(Dialog::ConfirmQuit); }
        }
        Ok(())
    }

    fn handle_confirm_kill_dialog(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(task) = self.tasks.get_mut(self.selected_index) {
                    if let Err(e) = task.kill() {
                        self.last_error = Some(format!("Failed to kill task: {e}"));
                    }
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {}
            _ => { self.dialog = Some(Dialog::ConfirmKill); }
        }
        Ok(())
    }

    fn handle_confirm_delete_dialog(&mut self, key: KeyEvent) -> anyhow::Result<()> {
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
                        }).await;
                        match result {
                            Ok(Ok(())) => { let _ = event_tx.send(AppEvent::TaskDeleted(id)).await; }
                            Ok(Err(e)) => { let _ = event_tx.send(AppEvent::GitOpResult(Err(format!("Delete failed: {e}")))).await; }
                            Err(e) => { let _ = event_tx.send(AppEvent::GitOpResult(Err(format!("Delete task error: {e}")))).await; }
                        }
                    });
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {}
            _ => { self.dialog = Some(Dialog::ConfirmDelete); }
        }
        Ok(())
    }

    fn handle_confirm_sync_dialog(&mut self, key: KeyEvent) -> anyhow::Result<()> {
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
                        }).await;
                        match result {
                            Ok(Ok(())) => { let _ = event_tx.send(AppEvent::GitOpResult(Ok(()))).await; }
                            Ok(Err(e)) => { let _ = event_tx.send(AppEvent::GitOpResult(Err(format!("Sync failed: {e}")))).await; }
                            Err(e) => { let _ = event_tx.send(AppEvent::GitOpResult(Err(format!("Sync error: {e}")))).await; }
                        }
                    });
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {}
            _ => { self.dialog = Some(Dialog::ConfirmSync); }
        }
        Ok(())
    }

    fn handle_confirm_merge_dialog(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Char('f') => {
                let repo_root = self.repo_root.clone();
                if let Some(task) = self.tasks.get(self.selected_index) {
                    let name = task.name.clone();
                    let upstream = task.upstream.clone();
                    let event_tx = self.event_tx.clone();
                    tokio::spawn(async move {
                        let result = tokio::task::spawn_blocking(move || {
                            Task::merge_ff(&repo_root, &name, &upstream)
                        }).await;
                        match result {
                            Ok(Ok(())) => { let _ = event_tx.send(AppEvent::GitOpResult(Ok(()))).await; }
                            Ok(Err(e)) => { let _ = event_tx.send(AppEvent::GitOpResult(Err(format!("FF merge failed: {e}")))).await; }
                            Err(e) => { let _ = event_tx.send(AppEvent::GitOpResult(Err(format!("Merge error: {e}")))).await; }
                        }
                    });
                }
            }
            KeyCode::Char('s') => {
                if let Some(task) = self.tasks.get(self.selected_index) {
                    let name = task.name.clone();
                    let upstream = task.upstream.clone();
                    let _ = self.event_tx.try_send(AppEvent::SquashMerge { name, upstream });
                }
            }
            KeyCode::Esc => {}
            _ => { self.dialog = Some(Dialog::ConfirmMerge); }
        }
        Ok(())
    }

    fn handle_change_upstream_dialog(&mut self, key: KeyEvent, branches: Vec<String>, mut selected: usize) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Esc => {}
            KeyCode::Char('j') | KeyCode::Down => {
                if !branches.is_empty() { selected = (selected + 1) % branches.len(); }
                self.dialog = Some(Dialog::ChangeUpstream { branches, selected });
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if !branches.is_empty() { selected = selected.checked_sub(1).unwrap_or(branches.len() - 1); }
                self.dialog = Some(Dialog::ChangeUpstream { branches, selected });
            }
            KeyCode::Enter => {
                let new_upstream = branches[selected].clone();
                if let Some(task) = self.tasks.get_mut(self.selected_index) {
                    let name = task.name.clone();
                    match Task::set_upstream(&self.repo_root, &name, &new_upstream) {
                        Ok(()) => { task.upstream = new_upstream; self.refresh_commits_ahead(); }
                        Err(e) => { self.last_error = Some(format!("Failed to set upstream: {e}")); }
                    }
                }
            }
            _ => { self.dialog = Some(Dialog::ChangeUpstream { branches, selected }); }
        }
        Ok(())
    }

    fn handle_diff_search_dialog(&mut self, key: KeyEvent, mut input: String) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Esc => {}
            KeyCode::Enter => {
                let pattern = input.trim().to_string();
                if !pattern.is_empty() {
                    let page_height = crossterm::terminal::size()
                        .map(|(_, r)| r.saturating_sub(2) as usize)
                        .unwrap_or(20);
                    if let Some(state) = self.diff_state_mut() {
                        state.search_forward(&pattern);
                        state.ensure_cursor_visible(page_height);
                    }
                }
            }
            KeyCode::Backspace => { input.pop(); self.dialog = Some(Dialog::DiffSearch { input }); }
            KeyCode::Char(c) => { input.push(c); self.dialog = Some(Dialog::DiffSearch { input }); }
            _ => { self.dialog = Some(Dialog::DiffSearch { input }); }
        }
        Ok(())
    }

    fn handle_tasks_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        let in_split = !matches!(self.layout(), ViewLayout::Single(_));

        // Ctrl+K must be checked before the bare 'k' pattern below
        if key.code == KeyCode::Char('k') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if let Some(task) = self.tasks.get(self.selected_index) {
                if task.is_running() {
                    self.dialog = Some(Dialog::ConfirmKill);
                }
            }
            return Ok(());
        }

        // Ctrl+O: toggle fullscreen(Tasks)
        if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if in_split {
                self.toggle_fullscreen(View::Tasks);
            }
            return Ok(());
        }

        // Ctrl+Q: close child views
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if in_split {
                self.view_stack.clear();
                self.fullscreen = None;
            }
            return Ok(());
        }

        match key.code {
            // q/Q: in split context, close child views; in single, quit
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                if in_split {
                    self.view_stack.clear();
                    self.fullscreen = None;
                } else {
                    let has_running = self
                        .tasks
                        .iter()
                        .any(|t| t.status == crate::task::TaskStatus::Running);
                    if has_running {
                        self.dialog = Some(Dialog::ConfirmQuit);
                    } else {
                        self.should_quit = true;
                    }
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
            // Enter: open agent view or resume task
            KeyCode::Enter => {
                if let Some(task) = self.tasks.get(self.selected_index) {
                    match task.status {
                        crate::task::TaskStatus::Running => {
                            self.push_agent(task.id);
                            self.focus = Pane::Right;
                            self.fullscreen = None;
                            self.sync_pty_size();
                        }
                        crate::task::TaskStatus::Stopped => {
                            self.resume_task(self.selected_index);
                        }
                    }
                }
            }
            KeyCode::Char('n') => {
                self.dialog = Some(Dialog::NewTask {
                    input: String::new(),
                });
            }
            // Shift+M: merge into upstream (Stopped only)
            KeyCode::Char('M') => {
                if let Some(task) = self.tasks.get(self.selected_index) {
                    if task.is_stopped() {
                        self.dialog = Some(Dialog::ConfirmMerge);
                    } else {
                        self.last_error = Some("Stop the task before merging".to_string());
                    }
                }
            }
            // Shift+S: sync from upstream (Stopped only)
            KeyCode::Char('S') => {
                if let Some(task) = self.tasks.get(self.selected_index) {
                    if task.is_stopped() {
                        self.dialog = Some(Dialog::ConfirmSync);
                    } else {
                        self.last_error = Some("Stop the task before syncing".to_string());
                    }
                }
            }
            // Shift+U: change upstream (Stopped only)
            KeyCode::Char('U') => {
                if let Some(task) = self.tasks.get(self.selected_index) {
                    if !task.is_stopped() {
                        self.last_error = Some("Stop the task before changing upstream".to_string());
                    } else if !task.has_run {
                        self.last_error = Some("Launch the task at least once before changing upstream".to_string());
                    } else {
                        let branches = Task::list_upstream_candidates(&self.repo_root);
                        if branches.is_empty() {
                            self.last_error = Some("No eligible upstream branches found".to_string());
                        } else {
                            let current_upstream = &task.upstream;
                            let selected = branches.iter()
                                .position(|b| b == current_upstream)
                                .unwrap_or(0);
                            self.dialog = Some(Dialog::ChangeUpstream { branches, selected });
                        }
                    }
                }
            }
            // d: open diff view (when commits_ahead > 0)
            KeyCode::Char('d') => {
                if let Some(task) = self.tasks.get(self.selected_index) {
                    let ahead = task.commits_ahead.unwrap_or(0);
                    if ahead == 0 {
                        self.last_error = Some("No commits ahead of upstream".to_string());
                    } else {
                        match DiffState::from_task(
                            &self.repo_root,
                            &task.name,
                            &task.upstream,
                        ) {
                            Ok(state) => {
                                self.push_diff(state);
                                self.focus = Pane::Right;
                                self.fullscreen = None;
                            }
                            Err(e) => {
                                self.last_error = Some(format!("Failed to get diff: {e}"));
                            }
                        }
                    }
                }
            }
            // !: delete task (Stopped only, tig-style)
            KeyCode::Char('!') => {
                if let Some(task) = self.tasks.get(self.selected_index) {
                    if task.is_stopped() {
                        self.dialog = Some(Dialog::ConfirmDelete);
                    } else {
                        self.last_error = Some("Stop the task before deleting".to_string());
                    }
                }
            }
            // R: refresh commits ahead
            KeyCode::Char('R') => {
                self.refresh_commits_ahead();
                if self.has_view(View::Diff) {
                    self.update_diff_for_selected_task();
                }
            }
            // O: toggle fullscreen(Tasks) — only meaningful if child views exist
            KeyCode::Char('O') => {
                if in_split {
                    self.toggle_fullscreen(View::Tasks);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_agent_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        // Ctrl+O: toggle agent fullscreen
        if key.code == KeyCode::Char('o') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.toggle_fullscreen(View::Agent);
            return Ok(());
        }

        // Ctrl+Q: close agent view
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.close_agent();
            return Ok(());
        }

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
                if task.is_running() {
                    let _ = task.write_input(&bytes);
                }
            }
        }
        Ok(())
    }

    fn handle_diff_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        let page_height = crossterm::terminal::size()
            .map(|(_, r)| r.saturating_sub(2) as usize)
            .unwrap_or(20);

        // Ctrl key shortcuts
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('b') => {
                    if let Some(state) = self.diff_state_mut() {
                        state.page_up(page_height);
                        state.ensure_cursor_visible(page_height);
                    }
                    return Ok(());
                }
                KeyCode::Char('f') => {
                    if let Some(state) = self.diff_state_mut() {
                        state.page_down(page_height);
                        state.ensure_cursor_visible(page_height);
                    }
                    return Ok(());
                }
                KeyCode::Char('o') => {
                    self.toggle_fullscreen(View::Diff);
                    return Ok(());
                }
                KeyCode::Char('q') => {
                    self.close_diff();
                    return Ok(());
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.close_diff();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(state) = self.diff_state_mut() {
                    state.move_cursor_down();
                    state.ensure_cursor_visible(page_height);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(state) = self.diff_state_mut() {
                    state.move_cursor_up();
                    state.ensure_cursor_visible(page_height);
                }
            }
            // @: jump to next hunk (sets search pattern to ^@@, like tig)
            KeyCode::Char('@') => {
                if let Some(state) = self.diff_state_mut() {
                    state.search_forward("^@@");
                    state.ensure_cursor_visible(page_height);
                }
            }
            // /: enter search mode
            KeyCode::Char('/') => {
                self.dialog = Some(Dialog::DiffSearch {
                    input: String::new(),
                });
            }
            // n: next search match
            KeyCode::Char('n') => {
                if let Some(state) = self.diff_state_mut() {
                    state.search_next();
                    state.ensure_cursor_visible(page_height);
                }
            }
            // N: previous search match
            KeyCode::Char('N') => {
                if let Some(state) = self.diff_state_mut() {
                    state.search_prev();
                    state.ensure_cursor_visible(page_height);
                }
            }
            KeyCode::Char('O') => {
                self.toggle_fullscreen(View::Diff);
            }
            // R: refresh diff for current task
            KeyCode::Char('R') => {
                self.update_diff_for_selected_task();
            }
            _ => {}
        }
        Ok(())
    }

    /// Update diff_state for the currently selected task.
    /// If the task has no commits ahead, clears diff_state.
    fn update_diff_for_selected_task(&mut self) {
        if let Some(task) = self.tasks.get(self.selected_index) {
            let ahead = task.commits_ahead.unwrap_or(0);
            if ahead == 0 {
                self.close_diff();
            } else {
                match DiffState::from_task(&self.repo_root, &task.name, &task.upstream) {
                    Ok(state) => {
                        self.update_diff(state);
                    }
                    Err(e) => {
                        self.last_error = Some(format!("Failed to get diff: {e}"));
                        self.close_diff();
                    }
                }
            }
        }
    }

    /// Resize all PTYs to match the current layout's effective dimensions.
    fn sync_pty_size(&mut self) {
        let (cols, rows) = crossterm::terminal::size().unwrap_or((200, 50));
        let content_rows = rows.saturating_sub(1);
        let agent_cols = match self.layout() {
            ViewLayout::Split(_, View::Agent) => {
                let list_width = (cols / 2).max(20);
                cols.saturating_sub(list_width + 1)
            }
            _ => cols,
        };
        if let Some(task) = self.focused_task_mut() {
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

    // -- State transition helpers --

    fn close_diff(&mut self) {
        self.view_stack.retain(|v| v.view() != View::Diff);
        if self.fullscreen == Some(View::Diff) {
            self.fullscreen = None;
        }
    }

    fn close_agent(&mut self) {
        self.view_stack.retain(|v| v.view() != View::Agent);
        if self.fullscreen == Some(View::Agent) {
            self.fullscreen = None;
        }
    }

    fn toggle_fullscreen(&mut self, view: View) {
        if self.fullscreen == Some(view) {
            self.fullscreen = None;
        } else {
            self.fullscreen = Some(view);
        }
        if view == View::Agent {
            self.sync_pty_size();
        }
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
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
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
