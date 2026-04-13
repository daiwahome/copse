//! Command-logging extension trait for `std::process::Command`.
//!
//! Each helper logs the command line at `debug` before execution and logs
//! non-zero exits / spawn failures at `warn`, so `COPSE_LOG=debug` yields
//! enough information to diagnose why an external command (git, tmux, delta,
//! shell) misbehaved without ad-hoc instrumentation.
//!
//! **Security note:** `format_cmd` logs the full argv including all arguments.
//! Do not pass secrets (tokens, credentials, private URLs) on the command
//! line — they will end up in the log file. Use environment variables or
//! stdin for secrets instead.

use std::fmt::Write as _;
use std::io;
use std::process::{Child, Command, ExitStatus, Output};

/// Extension trait adding logged-execution helpers to `std::process::Command`.
///
/// Allows keeping the existing builder chain style:
/// ```ignore
/// Command::new("git").args(["rev-parse", "HEAD"]).current_dir(&root).run_output()?;
/// ```
pub trait CommandLogExt {
    /// Like `Command::status()`, but logs the invocation and non-zero exits.
    fn run_status(&mut self) -> io::Result<ExitStatus>;

    /// Like [`run_status`](Self::run_status) but treats non-zero exits as
    /// normal and does not warn. Use this for *probe* commands where a
    /// non-zero exit is expected (e.g. `tmux has-session`, which exits 1
    /// when the session does not exist). Spawn failures (binary missing)
    /// are still warned.
    fn run_status_quiet(&mut self) -> io::Result<ExitStatus>;

    /// Like `Command::output()`, but logs the invocation, non-zero exits, and
    /// (on failure) the last few lines of stderr.
    fn run_output(&mut self) -> io::Result<Output>;

    /// Like `Command::spawn()`, but logs the invocation and spawn failures.
    /// Non-zero exits after wait are the caller's responsibility — pair with
    /// [`log_exit_status`] after `child.wait()`.
    fn run_spawn(&mut self) -> io::Result<Child>;
}

impl CommandLogExt for Command {
    fn run_status(&mut self) -> io::Result<ExitStatus> {
        log_exec(self);
        match self.status() {
            Ok(status) => {
                if !status.success() {
                    log::warn!(
                        "non-zero exit ({}): {}",
                        format_exit_code(status),
                        format_cmd(self)
                    );
                }
                Ok(status)
            }
            Err(e) => {
                log::warn!("spawn failed: {}: {e}", format_cmd(self));
                Err(e)
            }
        }
    }

    fn run_status_quiet(&mut self) -> io::Result<ExitStatus> {
        log_exec(self);
        match self.status() {
            Ok(status) => Ok(status),
            Err(e) => {
                log::warn!("spawn failed: {}: {e}", format_cmd(self));
                Err(e)
            }
        }
    }

    fn run_output(&mut self) -> io::Result<Output> {
        log_exec(self);
        match self.output() {
            Ok(out) => {
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let tail = stderr_tail(&stderr);
                    log::warn!(
                        "non-zero exit ({}): {} stderr=[{tail}]",
                        format_exit_code(out.status),
                        format_cmd(self)
                    );
                }
                Ok(out)
            }
            Err(e) => {
                log::warn!("spawn failed: {}: {e}", format_cmd(self));
                Err(e)
            }
        }
    }

    fn run_spawn(&mut self) -> io::Result<Child> {
        log_exec(self);
        match self.spawn() {
            Ok(child) => Ok(child),
            Err(e) => {
                log::warn!("spawn failed: {}: {e}", format_cmd(self));
                Err(e)
            }
        }
    }
}

/// Log the command line at `debug` level, skipping `format_cmd` entirely when
/// the `debug` target is disabled (the common case: default log level is
/// `info`).
fn log_exec(cmd: &Command) {
    if log::log_enabled!(log::Level::Debug) {
        log::debug!("exec: {}", format_cmd(cmd));
    }
}

/// Log a non-zero exit status at `warn` level. Used by callers that use
/// [`CommandLogExt::run_spawn`] and later wait on the child.
pub fn log_exit_status(label: &str, status: ExitStatus) {
    if !status.success() {
        log::warn!("non-zero exit ({}): {label}", format_exit_code(status));
    }
}

/// Format a `Command` as "program arg1 arg2 ... (cwd=/path)" for logging.
fn format_cmd(cmd: &Command) -> String {
    let mut s = cmd.get_program().to_string_lossy().to_string();
    for arg in cmd.get_args() {
        let _ = write!(s, " {}", arg.to_string_lossy());
    }
    if let Some(cwd) = cmd.get_current_dir() {
        let _ = write!(s, " (cwd={})", cwd.display());
    }
    s
}

fn format_exit_code(status: ExitStatus) -> String {
    match status.code() {
        Some(c) => c.to_string(),
        None => "signal".to_string(),
    }
}

/// Collect the last 5 non-empty lines of stderr for log output.
fn stderr_tail(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let start = lines.len().saturating_sub(5);
    lines[start..].join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_cmd_includes_program_args_cwd() {
        let mut cmd = Command::new("git");
        cmd.args(["rev-parse", "HEAD"]);
        cmd.current_dir("/tmp");
        let s = format_cmd(&cmd);
        assert_eq!(s, "git rev-parse HEAD (cwd=/tmp)");
    }

    #[test]
    fn format_cmd_without_cwd() {
        let mut cmd = Command::new("ls");
        cmd.arg("-l");
        let s = format_cmd(&cmd);
        assert_eq!(s, "ls -l");
    }

    #[test]
    fn stderr_tail_limits_to_five_lines() {
        let input = "a\nb\nc\nd\ne\nf\ng";
        assert_eq!(stderr_tail(input), "c | d | e | f | g");
    }

    #[test]
    fn stderr_tail_skips_blanks() {
        let input = "\n\nfatal: bad\n\n";
        assert_eq!(stderr_tail(input), "fatal: bad");
    }

    #[test]
    fn run_status_returns_success() {
        let status = Command::new("true")
            .run_status()
            .expect("true should be runnable");
        assert!(status.success());
    }

    #[test]
    fn run_status_quiet_returns_non_zero_without_error() {
        let status = Command::new("false")
            .run_status_quiet()
            .expect("false should be runnable");
        assert!(!status.success());
    }

    #[test]
    fn run_output_captures_stderr() {
        let out = Command::new("sh")
            .args(["-c", "echo err >&2; exit 2"])
            .run_output()
            .expect("sh should be runnable");
        assert!(!out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stderr).trim(), "err");
    }
}
