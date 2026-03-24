mod app;
mod config;
mod diff;
mod event;
mod keybind;
mod task;
mod theme;
mod tui;
mod ui;

use std::path::PathBuf;

use app::App;
use clap::Parser;
use tui::Tui;

#[derive(Parser)]
#[command(name = "copse", version, about = "TUI for running Claude Code tasks in parallel using git worktrees")]
struct Cli {
    /// Generate the default config file at ~/.config/copse/default-config.toml
    #[arg(long)]
    init: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.init {
        return config::Config::init();
    }

    // Ignore SIGTSTP so Ctrl+Z inside copse doesn't suspend the process.
    unsafe {
        nix::sys::signal::signal(
            nix::sys::signal::Signal::SIGTSTP,
            nix::sys::signal::SigHandler::SigIgn,
        )?;
    }

    // Locate the repository root and compute worktree base directory.
    let repo_root = find_repo_root()?;
    let worktree_base_dir = task::worktree_base_dir(&repo_root)?;
    let config = config::Config::load()?;

    // Restore the terminal even on panic
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture,
        );
        default_hook(info);
    }));

    let mut tui = Tui::new()?;
    let event_tx = tui.event_sender();
    let mut app = App::new(repo_root.clone(), worktree_base_dir.clone(), config, event_tx);

    // Populate existing copse/* branches as Stopped tasks on startup
    let (cols, rows) = crossterm::terminal::size().unwrap_or((200, 50));
    for name in task::Task::list_existing(&repo_root) {
        app.tasks
            .push(task::Task::from_existing(name, &repo_root, &worktree_base_dir, rows, cols));
    }

    tui.spawn_input_reader();
    let run_result = tui.run(&mut app).await;

    // Kill all running Claude Code processes on every exit path.
    // Worktrees and branches are left intact for fast resume on next launch.
    for task in &mut app.tasks {
        let _ = task.kill();
    }

    run_result
}

fn find_repo_root() -> anyhow::Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Not in a git repository. copse must be run from within a git repository.");
    }

    let path = String::from_utf8(output.stdout)?.trim().to_string();
    Ok(PathBuf::from(path))
}

