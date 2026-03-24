use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
    HunkHeader,
    FileHeader,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
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
    pub fn from_task(
        repo_root: &Path,
        name: &str,
        upstream: &str,
    ) -> anyhow::Result<Self> {
        let raw = get_diff(repo_root, name, upstream)?;
        Ok(Self::parse(&raw, name.to_string()))
    }

    /// Parse unified diff text into structured DiffState.
    pub fn parse(raw_diff: &str, task_name: String) -> Self {
        let mut lines = Vec::new();

        for raw_line in raw_diff.lines() {
            if raw_line.starts_with("diff --git ")
                || raw_line.starts_with("---")
                || raw_line.starts_with("+++")
                || raw_line.starts_with("index ")
            {
                lines.push(DiffLine {
                    kind: DiffLineKind::FileHeader,
                    content: raw_line.to_string(),
                });
            } else if raw_line.starts_with("@@") {
                lines.push(DiffLine {
                    kind: DiffLineKind::HunkHeader,
                    content: raw_line.to_string(),
                });
            } else if let Some(rest) = raw_line.strip_prefix('+') {
                lines.push(DiffLine {
                    kind: DiffLineKind::Added,
                    content: rest.to_string(),
                });
            } else if let Some(rest) = raw_line.strip_prefix('-') {
                lines.push(DiffLine {
                    kind: DiffLineKind::Removed,
                    content: rest.to_string(),
                });
            } else if let Some(rest) = raw_line.strip_prefix(' ') {
                lines.push(DiffLine {
                    kind: DiffLineKind::Context,
                    content: rest.to_string(),
                });
            } else {
                lines.push(DiffLine {
                    kind: DiffLineKind::FileHeader,
                    content: raw_line.to_string(),
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
        let needle = if is_anchor { &pattern[1..] } else { pattern.as_str() };
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
