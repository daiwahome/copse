use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use ansi_to_tui::IntoText;
use ratatui::text::Line;

#[derive(Debug, Clone, PartialEq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
    HunkHeader,
    FileHeader,
}

#[derive(Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    /// ANSI-colored line from delta (None = no delta available)
    pub ansi_line: Option<Line<'static>>,
}

impl std::fmt::Debug for DiffLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiffLine")
            .field("kind", &self.kind)
            .field("content", &self.content)
            .field("ansi_line", &self.ansi_line.as_ref().map(|_| "..."))
            .finish()
    }
}

pub struct DiffState {
    pub lines: Vec<DiffLine>,
    pub cursor: usize,
    pub scroll_offset: usize,
    pub task_name: String,
    /// Current search pattern (set by `/` search or `@` hunk jump)
    pub search_pattern: Option<String>,
}

impl DiffState {
    /// Run `git diff <upstream>..<branch>` and parse the output.
    /// Returns `(DiffState, Option<warning_message>)`.
    pub fn from_task(
        repo_root: &Path,
        name: &str,
        upstream: &str,
        diff_filter: &str,
    ) -> anyhow::Result<(Self, Option<String>)> {
        let raw = get_diff(repo_root, name, upstream)?;
        let (colored, warning) = match diff_filter {
            "none" => (None, None),
            "auto" => (colorize_with_delta(&raw), None),
            "delta" => {
                let colored = colorize_with_delta(&raw);
                let warning = if colored.is_none() {
                    Some("diff_filter = \"delta\" but delta is not installed".to_string())
                } else {
                    None
                };
                (colored, warning)
            }
            other => (
                None,
                Some(format!(
                    "Unknown diff_filter value: \"{other}\", using none"
                )),
            ),
        };
        Ok((
            Self::parse(&raw, name.to_string(), colored.as_deref()),
            warning,
        ))
    }

    /// Parse unified diff text into structured DiffState.
    pub fn parse(raw_diff: &str, task_name: String, colored_diff: Option<&str>) -> Self {
        let mut lines = Vec::new();

        // Convert colored lines into an iterator of ANSI-parsed Lines
        let raw_line_count = raw_diff.lines().count();
        let ansi_lines: Vec<Option<Line<'static>>> = match colored_diff {
            Some(colored) => {
                let parsed: Vec<_> = colored
                    .lines()
                    .map(|l| {
                        l.as_bytes()
                            .into_text()
                            .ok()
                            .and_then(|t| t.lines.into_iter().next())
                    })
                    .collect();
                if parsed.len() != raw_line_count {
                    // Line count mismatch — discard delta output entirely
                    Vec::new()
                } else {
                    parsed
                }
            }
            None => Vec::new(),
        };
        let mut ansi_iter = ansi_lines.into_iter();

        for raw_line in raw_diff.lines() {
            let ansi_line = ansi_iter.next().flatten();

            if raw_line.starts_with("diff --git ")
                || raw_line.starts_with("---")
                || raw_line.starts_with("+++")
                || raw_line.starts_with("index ")
            {
                lines.push(DiffLine {
                    kind: DiffLineKind::FileHeader,
                    content: raw_line.to_string(),
                    ansi_line,
                });
            } else if raw_line.starts_with("@@") {
                lines.push(DiffLine {
                    kind: DiffLineKind::HunkHeader,
                    content: raw_line.to_string(),
                    ansi_line,
                });
            } else if let Some(rest) = raw_line.strip_prefix('+') {
                lines.push(DiffLine {
                    kind: DiffLineKind::Added,
                    content: rest.to_string(),
                    ansi_line,
                });
            } else if let Some(rest) = raw_line.strip_prefix('-') {
                lines.push(DiffLine {
                    kind: DiffLineKind::Removed,
                    content: rest.to_string(),
                    ansi_line,
                });
            } else if let Some(rest) = raw_line.strip_prefix(' ') {
                lines.push(DiffLine {
                    kind: DiffLineKind::Context,
                    content: rest.to_string(),
                    ansi_line,
                });
            } else {
                lines.push(DiffLine {
                    kind: DiffLineKind::FileHeader,
                    content: raw_line.to_string(),
                    ansi_line,
                });
            }
        }

        DiffState {
            lines,
            cursor: 0,
            scroll_offset: 0,
            task_name,
            search_pattern: None,
        }
    }

    pub fn move_cursor_down(&mut self) {
        if !self.lines.is_empty() {
            self.cursor = (self.cursor + 1).min(self.lines.len() - 1);
        }
    }

    pub fn move_cursor_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn page_down(&mut self, page_height: usize) {
        if !self.lines.is_empty() {
            self.cursor = (self.cursor + page_height).min(self.lines.len() - 1);
        }
    }

    pub fn page_up(&mut self, page_height: usize) {
        self.cursor = self.cursor.saturating_sub(page_height);
    }

    /// Set search pattern and jump to next match from current cursor.
    pub fn search_forward(&mut self, pattern: &str) {
        self.search_pattern = Some(pattern.to_string());
        self.search_next();
    }

    /// Jump to next match of current search pattern (no wrapping).
    /// If no pattern is set, defaults to `^@@` (hunk jump, like tig).
    pub fn search_next(&mut self) {
        if self.search_pattern.is_none() {
            self.search_pattern = Some("^@@".to_string());
        }
        let pat = self.search_pattern.as_ref().unwrap().clone();
        let is_anchor = pat.starts_with('^');
        let needle = if is_anchor { &pat[1..] } else { &pat };

        for i in (self.cursor + 1)..self.lines.len() {
            if line_matches(&self.lines[i], needle, is_anchor) {
                self.cursor = i;
                return;
            }
        }
    }

    /// Jump to previous match of current search pattern (no wrapping).
    /// If no pattern is set, defaults to `^@@` (hunk jump, like tig).
    pub fn search_prev(&mut self) {
        if self.search_pattern.is_none() {
            self.search_pattern = Some("^@@".to_string());
        }
        let pat = self.search_pattern.as_ref().unwrap().clone();
        let is_anchor = pat.starts_with('^');
        let needle = if is_anchor { &pat[1..] } else { &pat };

        for i in (0..self.cursor).rev() {
            if line_matches(&self.lines[i], needle, is_anchor) {
                self.cursor = i;
                return;
            }
        }
    }

    /// Check if any line matches the current search pattern.
    pub fn line_matches_search(&self, line_index: usize) -> bool {
        let Some(pattern) = &self.search_pattern else {
            return false;
        };
        let is_anchor = pattern.starts_with('^');
        let needle = if is_anchor {
            &pattern[1..]
        } else {
            pattern.as_str()
        };
        self.lines
            .get(line_index)
            .map(|l| line_matches(l, needle, is_anchor))
            .unwrap_or(false)
    }

    /// Ensure the cursor is visible by adjusting scroll_offset.
    pub fn ensure_cursor_visible(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        } else if self.cursor >= self.scroll_offset + viewport_height {
            self.scroll_offset = self.cursor - viewport_height + 1;
        }
    }
}

/// Check if a diff line's display content matches a search pattern.
fn line_matches(line: &DiffLine, needle: &str, anchored: bool) -> bool {
    let display = match line.kind {
        DiffLineKind::Added => format!("+{}", line.content),
        DiffLineKind::Removed => format!("-{}", line.content),
        DiffLineKind::Context => format!(" {}", line.content),
        DiffLineKind::HunkHeader | DiffLineKind::FileHeader => line.content.clone(),
    };
    if anchored {
        display.starts_with(needle)
    } else {
        display.contains(needle)
    }
}

/// Check if delta is available in PATH (cached after first call).
fn is_delta_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("delta")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}

/// Run raw diff text through `delta --color-only` for syntax highlighting.
/// Returns None if delta is not installed or fails.
fn colorize_with_delta(raw_diff: &str) -> Option<String> {
    if !is_delta_available() {
        return None;
    }

    let mut child = Command::new("delta")
        .args(["--no-gitconfig", "--color-only"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    child
        .stdin
        .take()
        .unwrap()
        .write_all(raw_diff.as_bytes())
        .ok()?;

    let output = child.wait_with_output().ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        None
    }
}

/// Run `git diff <upstream>..<branch>` and return the raw output.
pub fn get_diff(repo_root: &Path, name: &str, upstream: &str) -> anyhow::Result<String> {
    let branch = format!("copse/{name}");
    let output = std::process::Command::new("git")
        .args(["diff", &format!("{upstream}..{branch}")])
        .current_dir(repo_root)
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "git diff failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
