use std::path::Path;
use std::process::Stdio;
use std::sync::OnceLock;

use portable_pty::CommandBuilder;

use crate::config::Backend;
use crate::task::repo_id;

pub struct SessionParams<'a> {
    pub repo_root: &'a Path,
    pub task_name: &'a str,
    pub worktree_path: &'a Path,
    pub cols: u16,
    pub rows: u16,
    pub has_session: bool,
    pub permission_mode: &'a str,
}

impl Backend {
    /// Check that the backend's external dependency is available.
    pub fn validate(&self) -> anyhow::Result<()> {
        match self {
            Backend::BuiltIn => Ok(()),
            Backend::Tmux => {
                if is_tmux_available() {
                    Ok(())
                } else {
                    anyhow::bail!(
                        "backend = \"tmux\" is configured but tmux is not installed or not in PATH"
                    )
                }
            }
        }
    }

    /// Compute the session identifier for a task.
    /// Returns `None` for backends without session management.
    pub fn session_id(&self, repo_root: &Path, task_name: &str) -> Option<String> {
        match self {
            Backend::BuiltIn => None,
            Backend::Tmux => Some(tmux_session_name(repo_root, task_name)),
        }
    }

    /// Check whether a session is still alive.
    pub fn is_session_alive(&self, session_id: Option<&str>) -> bool {
        match self {
            Backend::BuiltIn => false,
            Backend::Tmux => session_id.is_some_and(tmux_session_exists),
        }
    }

    /// Detect an existing background session for a task on startup.
    /// Returns `Some(session_id)` if a live session exists.
    pub fn detect_running_session(&self, repo_root: &Path, task_name: &str) -> Option<String> {
        let id = self.session_id(repo_root, task_name)?;
        if tmux_session_exists(&id) {
            Some(id)
        } else {
            None
        }
    }

    /// Create a background session for the task if needed.
    /// Returns `Some(session_id)` if a session was created or already exists.
    pub fn create_session(&self, params: &SessionParams) -> anyhow::Result<Option<String>> {
        match self {
            Backend::BuiltIn => Ok(None),
            Backend::Tmux => {
                let session = tmux_session_name(params.repo_root, params.task_name);
                if !tmux_session_exists(&session) {
                    let cols_str = params.cols.to_string();
                    let rows_str = params.rows.to_string();
                    let wt_str = params.worktree_path.to_string_lossy().to_string();
                    let mut tmux_args: Vec<&str> = vec![
                        "new-session",
                        "-d",
                        "-s",
                        &session,
                        "-x",
                        &cols_str,
                        "-y",
                        &rows_str,
                        "-c",
                        &wt_str,
                    ];
                    let env_term = "TERM=xterm-256color";
                    tmux_args.extend(["-e", env_term]);
                    tmux_args.push("--");
                    tmux_args.push("claude");
                    if params.has_session {
                        tmux_args.push("--continue");
                    }
                    tmux_args.extend(["--permission-mode", params.permission_mode]);

                    let out = tmux_command().args(&tmux_args).output()?;
                    if !out.status.success() {
                        anyhow::bail!(
                            "tmux new-session failed: {}",
                            String::from_utf8_lossy(&out.stderr)
                        );
                    }
                }

                // Configure the dedicated tmux server for transparent operation:
                // - Hide status bar (copse renders its own UI)
                // - Disable prefix key so all keys pass through
                // - Set scrollback buffer size
                // - Remove all default key bindings
                // - Add back only scrollback controls (Ctrl-B/F for copy-mode)
                // Default copy-mode bindings (q/Escape to exit) survive unbind-key -a.
                // Idempotent — safe to run on every attach, not just new sessions.
                let _ = tmux_command()
                    .args([
                        "set-option",
                        "-g",
                        "status",
                        "off",
                        ";",
                        "set-option",
                        "-g",
                        "prefix",
                        "None",
                        ";",
                        "set-option",
                        "-g",
                        "history-limit",
                        "10000",
                        ";",
                        "unbind-key",
                        "-a",
                        ";",
                        "bind-key",
                        "-n",
                        "C-b",
                        "copy-mode",
                        "-u",
                        ";",
                        "bind-key",
                        "-n",
                        "C-f",
                        "copy-mode",
                        ";",
                        "bind-key",
                        "-T",
                        "copy-mode",
                        "C-b",
                        "send-keys",
                        "-X",
                        "page-up",
                        ";",
                        "bind-key",
                        "-T",
                        "copy-mode",
                        "C-f",
                        "send-keys",
                        "-X",
                        "page-down",
                    ])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .output();

                Ok(Some(session))
            }
        }
    }

    /// Build the command to run inside the PTY.
    pub fn build_pty_command(
        &self,
        session_id: Option<&str>,
        worktree_path: &Path,
        has_session: bool,
        permission_mode: &str,
    ) -> CommandBuilder {
        match (self, session_id) {
            (Backend::Tmux, Some(session)) => {
                let mut cmd = tmux_command_builder();
                let target = format!("={session}");
                cmd.args(["attach-session", "-t", &target]);
                cmd.env("TERM", "xterm-256color");
                cmd
            }
            _ => {
                let mut cmd = CommandBuilder::new("claude");
                if has_session {
                    cmd.arg("--continue");
                }
                cmd.args(["--permission-mode", permission_mode]);
                cmd.env("TERM", "xterm-256color");
                cmd.cwd(worktree_path);
                cmd
            }
        }
    }

    /// Resize the backend's window/session.
    pub fn resize_session(&self, session_id: Option<&str>, cols: u16, rows: u16) {
        if let (Backend::Tmux, Some(session)) = (self, session_id) {
            let target = format!("={session}");
            let _ = tmux_command()
                .args([
                    "resize-window",
                    "-t",
                    &target,
                    "-x",
                    &cols.to_string(),
                    "-y",
                    &rows.to_string(),
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .output();
        }
    }

    /// Kill the backend session.
    pub fn kill_session(&self, session_id: Option<&str>) {
        if let (Backend::Tmux, Some(session)) = (self, session_id) {
            let target = format!("={session}");
            let _ = tmux_command()
                .args(["kill-session", "-t", &target])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .output();
        }
    }

    /// Whether the backend handles scrollback natively (e.g. tmux copy-mode).
    /// When true, copse skips its own scrollback and lets keys pass through to the PTY.
    pub fn handles_scrollback(&self) -> bool {
        match self {
            Backend::BuiltIn => false,
            Backend::Tmux => true,
        }
    }

    /// Whether this backend supports detached/background execution.
    pub fn supports_detach(&self) -> bool {
        match self {
            Backend::BuiltIn => false,
            Backend::Tmux => true,
        }
    }

    /// Whether to show a quit confirmation when tasks are running.
    pub fn needs_quit_confirmation(&self) -> bool {
        match self {
            Backend::BuiltIn => true,
            Backend::Tmux => false,
        }
    }
}

// -- Private tmux helpers --

/// Socket name for the copse-dedicated tmux server.
const TMUX_SOCKET: &str = "copse";

/// Build a `tmux` Command with `-L copse -f /dev/null` so copse uses its own
/// server and ignores the user's tmux configuration.
fn tmux_command() -> std::process::Command {
    let mut cmd = std::process::Command::new("tmux");
    cmd.args(["-L", TMUX_SOCKET, "-f", "/dev/null"]);
    cmd
}

fn tmux_command_builder() -> CommandBuilder {
    let mut cmd = CommandBuilder::new("tmux");
    cmd.args(["-L", TMUX_SOCKET, "-f", "/dev/null"]);
    cmd
}

fn is_tmux_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        std::process::Command::new("tmux")
            .arg("-V")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}

fn tmux_session_name(repo_root: &Path, task_name: &str) -> String {
    let repo = repo_id(repo_root);
    // tmux session names cannot contain '.' (invalid) or ':' (parsed as
    // session:window separator by -t), so replace both with '_'.
    let sanitized = repo.to_string_lossy().replace(['.', ':'], "_");
    format!("{sanitized}/{task_name}")
}

fn tmux_session_exists(session_name: &str) -> bool {
    let target = format!("={session_name}");
    tmux_command()
        .args(["has-session", "-t", &target])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn builtin_validate_always_ok() {
        assert!(Backend::BuiltIn.validate().is_ok());
    }

    #[test]
    fn builtin_session_id_is_none() {
        let root = PathBuf::from("/tmp/repo");
        assert_eq!(Backend::BuiltIn.session_id(&root, "task"), None);
    }

    #[test]
    fn builtin_session_never_alive() {
        assert!(!Backend::BuiltIn.is_session_alive(None));
        assert!(!Backend::BuiltIn.is_session_alive(Some("anything")));
    }

    #[test]
    fn builtin_detect_running_session_is_none() {
        let root = PathBuf::from("/tmp/repo");
        assert_eq!(Backend::BuiltIn.detect_running_session(&root, "task"), None);
    }

    #[test]
    fn builtin_create_session_is_none() {
        let root = PathBuf::from("/tmp/repo");
        let wt = PathBuf::from("/tmp/wt");
        let result = Backend::BuiltIn.create_session(&SessionParams {
            repo_root: &root,
            task_name: "task",
            worktree_path: &wt,
            cols: 80,
            rows: 24,
            has_session: false,
            permission_mode: "default",
        });
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn builtin_does_not_handle_scrollback() {
        assert!(!Backend::BuiltIn.handles_scrollback());
    }

    #[test]
    fn builtin_does_not_support_detach() {
        assert!(!Backend::BuiltIn.supports_detach());
    }

    #[test]
    fn builtin_needs_quit_confirmation() {
        assert!(Backend::BuiltIn.needs_quit_confirmation());
    }

    #[test]
    fn tmux_handles_scrollback() {
        assert!(Backend::Tmux.handles_scrollback());
    }

    #[test]
    fn tmux_supports_detach() {
        assert!(Backend::Tmux.supports_detach());
    }

    #[test]
    fn tmux_no_quit_confirmation() {
        assert!(!Backend::Tmux.needs_quit_confirmation());
    }

    #[test]
    fn tmux_session_name_sanitizes_dot_and_colon() {
        // Simulate repo_id returning a path with dots and colons
        let name = "github.com/owner/repo";
        let sanitized = name.replace(['.', ':'], "_");
        assert_eq!(sanitized, "github_com/owner/repo");
        let result = format!("{sanitized}/my-task");
        assert_eq!(result, "github_com/owner/repo/my-task");
    }

    #[test]
    fn tmux_session_name_preserves_slashes() {
        let name = "github.com/owner/repo";
        let sanitized = name.replace(['.', ':'], "_");
        assert!(sanitized.contains('/'));
    }
}
