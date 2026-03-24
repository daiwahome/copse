use std::{
    io::{self, Stdout},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use crossterm::{
    event::{
        self, DisableMouseCapture, Event, KeyEventKind, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;

use crate::{app::App, event::AppEvent, ui};

pub struct Tui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    event_tx: mpsc::Sender<AppEvent>,
    event_rx: mpsc::Receiver<AppEvent>,
    /// When true, the input reader pauses polling (used during $EDITOR)
    input_paused: Arc<AtomicBool>,
    /// Whether the kitty keyboard protocol was successfully enabled
    keyboard_enhancement_enabled: bool,
}

impl Tui {
    pub fn new() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(e) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(e.into());
        }
        let backend = CrosstermBackend::new(stdout);
        let terminal = match Terminal::new(backend) {
            Ok(t) => t,
            Err(e) => {
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                return Err(e.into());
            }
        };
        let keyboard_enhancement_enabled =
            crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false)
                && execute!(
                    io::stdout(),
                    PushKeyboardEnhancementFlags(
                        KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    )
                )
                .is_ok();
        let (event_tx, event_rx) = mpsc::channel(256);
        Ok(Self {
            terminal,
            event_tx,
            event_rx,
            input_paused: Arc::new(AtomicBool::new(false)),
            keyboard_enhancement_enabled,
        })
    }

    pub fn event_sender(&self) -> mpsc::Sender<AppEvent> {
        self.event_tx.clone()
    }

    /// Read keyboard and resize events in a background task
    pub fn spawn_input_reader(&self) {
        let tx = self.event_tx.clone();
        let paused = Arc::clone(&self.input_paused);
        tokio::task::spawn_blocking(move || loop {
            if tx.is_closed() {
                break;
            }
            // When paused (e.g. during $EDITOR), sleep instead of polling
            if paused.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            // 100ms timeout reduces idle CPU wakeups vs the previous 16ms
            match event::poll(Duration::from_millis(100)) {
                Ok(true) => {
                    // Re-check pause flag after poll returns — events that
                    // arrived while $EDITOR was active should be discarded
                    if paused.load(Ordering::Relaxed) {
                        continue;
                    }
                    match event::read() {
                        Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                            if tx.blocking_send(AppEvent::Key(key)).is_err() {
                                break;
                            }
                        }
                        Ok(Event::Resize(cols, rows)) => {
                            if tx.blocking_send(AppEvent::Resize { cols, rows }).is_err() {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    let _ = tx.blocking_send(AppEvent::FatalError(format!(
                        "Input poll error: {e}"
                    )));
                    break;
                }
            }
        });
    }

    /// Main event loop
    pub async fn run(&mut self, app: &mut App) -> anyhow::Result<()> {
        let mut refresh_interval = tokio::time::interval(Duration::from_secs(5));
        // Don't pile up ticks when the loop is busy
        refresh_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            self.terminal.draw(|frame| {
                ui::render(frame, app);
            })?;

            tokio::select! {
                event = self.event_rx.recv() => {
                    match event {
                        Some(AppEvent::SquashMerge { name, upstream }) => {
                            let result = self.execute_squash_merge(&app.repo_root, &app.git_common_dir, &name, &upstream);
                            if let Err(e) = result {
                                app.last_error = Some(format!("Squash merge failed: {e}"));
                            }
                            app.refresh_commits_ahead();
                        }
                        Some(event) => {
                            app.handle_event(event)?;
                        }
                        None => break,
                    }
                }
                _ = refresh_interval.tick() => {
                    app.refresh_commits_ahead();
                }
            }

            if app.should_quit {
                break;
            }
        }
        Ok(())
    }

    /// Leave alternate screen, squash-merge a task branch into upstream,
    /// then align the task branch to upstream.
    ///
    /// Builds a rebase -i style commit message template, runs
    /// `git merge --squash` + `git commit` (with $EDITOR) in the upstream
    /// worktree, then `git reset --hard` in the task worktree so both
    /// branches match.
    fn execute_squash_merge(
        &mut self,
        repo_root: &std::path::Path,
        git_common_dir: &std::path::Path,
        name: &str,
        upstream: &str,
    ) -> anyhow::Result<()> {
        use crate::task::Task;

        let branch = Task::branch_name(name);
        let task_wt = Task::worktree_path_for(&git_common_dir.to_path_buf(), name);
        let (upstream_wt, switched) = match Task::find_branch_worktree(repo_root, upstream) {
            Ok(wt) => (wt, false),
            Err(_) => {
                // Upstream not checked out anywhere; temporarily switch the task worktree
                Task::switch_branch(&task_wt, upstream)?;
                (task_wt.clone(), true)
            }
        };

        // Pause input reader so it doesn't consume stdin during $EDITOR
        self.input_paused.store(true, Ordering::Relaxed);

        // Leave alternate screen so $EDITOR can use the terminal
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;

        let result = (|| -> anyhow::Result<()> {
            // Build commit message template (rebase -i squash style)
            let log_output = std::process::Command::new("git")
                .args(["log", "--reverse", "--format=----%n%B", &format!("{upstream}..{branch}")])
                .current_dir(repo_root)
                .output();
            let raw = log_output
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();

            let commits: Vec<&str> = raw.split("----\n")
                .filter(|s| !s.trim().is_empty())
                .collect();
            let count = commits.len();

            let mut msg = format!("* This is a combination of {count} commits.\n");
            for (i, body) in commits.iter().enumerate() {
                let header = if i == 0 {
                    "* This is the 1st commit message:".to_string()
                } else {
                    format!("* This is the commit message #{}:", i + 1)
                };
                msg.push_str(&format!(
                    "\n{header}\n\n{}\n",
                    body.trim()
                ));
            }

            // Stage all task branch changes onto upstream
            let out = std::process::Command::new("git")
                .args(["merge", "--squash", &branch])
                .current_dir(&upstream_wt)
                .output()?;
            if !out.status.success() {
                anyhow::bail!("merge --squash failed: {}", String::from_utf8_lossy(&out.stderr));
            }

            // Write template and commit with $EDITOR
            let tmp = std::env::temp_dir().join(format!("copse-squash-msg-{}", std::process::id()));
            std::fs::write(&tmp, &msg)?;

            let commit_status = std::process::Command::new("git")
                .args(["commit", "-e", "-F", tmp.to_str().unwrap_or(""), "--cleanup=strip"])
                .current_dir(&upstream_wt)
                .stdin(std::process::Stdio::inherit())
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .status();

            let _ = std::fs::remove_file(&tmp);

            if !commit_status.map(|s| s.success()).unwrap_or(false) {
                // Commit was aborted — clean up staged changes in upstream worktree
                let _ = std::process::Command::new("git")
                    .args(["reset", "--hard"])
                    .current_dir(&upstream_wt)
                    .output();
                anyhow::bail!("commit aborted");
            }

            // Align task branch to upstream
            let out = std::process::Command::new("git")
                .args(["reset", "--hard", upstream])
                .current_dir(&task_wt)
                .output()?;
            if !out.status.success() {
                anyhow::bail!("reset --hard failed: {}", String::from_utf8_lossy(&out.stderr));
            }

            Ok(())
        })();

        // Always switch back to task branch if we borrowed the task worktree
        if switched {
            let _ = Task::switch_branch(&task_wt, &branch);
        }

        // Return to alternate screen and resume input reader
        enable_raw_mode()?;
        execute!(self.terminal.backend_mut(), EnterAlternateScreen)?;
        if self.keyboard_enhancement_enabled {
            let _ = execute!(
                self.terminal.backend_mut(),
                PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                )
            );
        }
        self.terminal.clear()?;
        self.input_paused.store(false, Ordering::Relaxed);

        result
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        // Always restore the terminal, even on panic
        if self.keyboard_enhancement_enabled {
            let _ = execute!(self.terminal.backend_mut(), PopKeyboardEnhancementFlags);
        }
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
    }
}
