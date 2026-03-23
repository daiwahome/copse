use std::{
    io::{self, Stdout},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use crossterm::{
    event::{self, DisableMouseCapture, Event, KeyEventKind},
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
        let (event_tx, event_rx) = mpsc::channel(256);
        Ok(Self {
            terminal,
            event_tx,
            event_rx,
            input_paused: Arc::new(AtomicBool::new(false)),
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
        loop {
            self.terminal.draw(|frame| {
                ui::render(frame, app);
            })?;

            match self.event_rx.recv().await {
                Some(AppEvent::SquashMerge { name, upstream }) => {
                    // Squash merge needs $EDITOR — leave alternate screen temporarily
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

            if app.should_quit {
                break;
            }
        }
        Ok(())
    }

    /// Leave alternate screen, squash commits in the task worktree, then
    /// fast-forward upstream via update-ref.
    ///
    /// Approach: `git reset --soft <upstream>` collapses all task commits into
    /// the staging area, then `git commit` creates a single squash commit on
    /// top of upstream. Finally `update-ref` advances the upstream branch.
    fn execute_squash_merge(
        &mut self,
        repo_root: &std::path::Path,
        git_common_dir: &std::path::Path,
        name: &str,
        upstream: &str,
    ) -> anyhow::Result<()> {
        use crate::task::Task;

        let branch = Task::branch_name(name);
        let worktree_path = Task::worktree_path_for(
            &git_common_dir.to_path_buf(),
            name,
        );

        // Pause input reader so it doesn't consume stdin during $EDITOR
        self.input_paused.store(true, Ordering::Relaxed);

        // Leave alternate screen so $EDITOR can use the terminal
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;

        let result = (|| -> anyhow::Result<()> {
            // Build commit message template (rebase -i squash style with *)
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

            // Reset task branch onto upstream (all changes become staged)
            let out = std::process::Command::new("git")
                .args(["reset", "--soft", upstream])
                .current_dir(&worktree_path)
                .output()?;
            if !out.status.success() {
                anyhow::bail!("reset --soft failed: {}", String::from_utf8_lossy(&out.stderr));
            }

            // Write template and commit with $EDITOR
            let tmp = std::env::temp_dir().join(format!("copse-squash-msg-{}", std::process::id()));
            std::fs::write(&tmp, &msg)?;

            let commit_status = std::process::Command::new("git")
                .args(["commit", "-e", "-F", tmp.to_str().unwrap_or(""), "--cleanup=strip"])
                .current_dir(&worktree_path)
                .stdin(std::process::Stdio::inherit())
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .status();

            let _ = std::fs::remove_file(&tmp);

            if !commit_status.map(|s| s.success()).unwrap_or(false) {
                // Commit was aborted — restore the branch to its original position
                let recovery = std::process::Command::new("git")
                    .args(["reset", "--soft", &branch])
                    .current_dir(&worktree_path)
                    .output();
                match recovery {
                    Ok(out) if out.status.success() => {
                        anyhow::bail!("commit aborted");
                    }
                    _ => {
                        anyhow::bail!(
                            "commit aborted and recovery failed — run `git reset --soft {}` \
                             in the task worktree to restore the branch",
                            branch
                        );
                    }
                }
            }

            // Fast-forward upstream to the new squash commit
            let head = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&worktree_path)
                .output()?;
            let commit = String::from_utf8_lossy(&head.stdout).trim().to_string();

            let out = std::process::Command::new("git")
                .args(["update-ref", &format!("refs/heads/{upstream}"), &commit])
                .current_dir(repo_root)
                .output()?;
            if !out.status.success() {
                anyhow::bail!("update-ref failed: {}", String::from_utf8_lossy(&out.stderr));
            }

            Ok(())
        })();

        // Return to alternate screen and resume input reader
        enable_raw_mode()?;
        execute!(self.terminal.backend_mut(), EnterAlternateScreen)?;
        self.terminal.clear()?;
        self.input_paused.store(false, Ordering::Relaxed);

        result
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        // Always restore the terminal, even on panic
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
    }
}
