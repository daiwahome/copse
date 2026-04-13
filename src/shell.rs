use std::path::Path;
use std::time::Duration;

use crate::backend::is_tmux_available;
use crate::config::ShellMode;
use crate::process::{self, CommandLogExt};

impl ShellMode {
    /// Check that the shell mode's external dependency is available.
    pub fn validate(&self) -> anyhow::Result<()> {
        match self {
            ShellMode::Suspend => Ok(()),
            ShellMode::Tmux => {
                if is_tmux_available() {
                    Ok(())
                } else {
                    anyhow::bail!(
                        "shell_mode = \"tmux\" is configured but tmux is not installed or not in PATH"
                    )
                }
            }
        }
    }

    /// Whether this mode requires TUI suspend/resume.
    pub fn needs_suspend(&self) -> bool {
        matches!(self, ShellMode::Suspend)
    }

    /// Open a shell in the worktree directory.
    ///
    /// For `Suspend` mode, the caller must suspend the TUI before calling this
    /// and resume it after.
    pub fn open(&self, worktree_path: &Path) -> anyhow::Result<()> {
        match self {
            ShellMode::Tmux => {
                let status = std::process::Command::new("tmux")
                    .args(["new-window", "-c", &worktree_path.to_string_lossy()])
                    .run_status()?;
                if !status.success() {
                    anyhow::bail!("tmux new-window failed");
                }
                Ok(())
            }
            ShellMode::Suspend => {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

                let result = match std::process::Command::new(&shell)
                    .current_dir(worktree_path)
                    .stdin(std::process::Stdio::inherit())
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit())
                    .run_spawn()
                {
                    Ok(mut child) => {
                        std::thread::sleep(Duration::from_millis(50));
                        let _ = nix::sys::signal::kill(
                            nix::unistd::Pid::from_raw(child.id() as i32),
                            nix::sys::signal::Signal::SIGWINCH,
                        );
                        child.wait()
                    }
                    Err(e) => {
                        return Err(anyhow::anyhow!("Failed to spawn shell: {e}"));
                    }
                };

                match result {
                    Ok(status) => {
                        process::log_exit_status("shell", status);
                    }
                    Err(e) => {
                        return Err(anyhow::anyhow!("Shell failed: {e}"));
                    }
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suspend_validate_always_ok() {
        assert!(ShellMode::Suspend.validate().is_ok());
    }

    #[test]
    fn suspend_needs_suspend() {
        assert!(ShellMode::Suspend.needs_suspend());
    }

    #[test]
    fn tmux_does_not_need_suspend() {
        assert!(!ShellMode::Tmux.needs_suspend());
    }
}
