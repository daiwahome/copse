use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use ansi_to_tui::IntoText;
use ratatui::text::Line;

use crate::config::DiffFilter;

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

/// A review comment attached to a specific diff line.
#[derive(Debug, Clone)]
pub struct ReviewComment {
    /// Index into DiffState.lines
    pub line_index: usize,
    /// The file path this comment pertains to (derived from nearest FileHeader)
    pub file_path: String,
    /// The comment text entered by the user
    pub text: String,
}

/// Inline editing state for a review comment.
#[derive(Debug, Clone)]
pub struct EditingComment {
    /// The text being edited (supports multiple lines via '\n')
    pub text: String,
}

/// Search/jump mode for the diff view.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchMode {
    /// Text pattern search (set by `/` or `@` for hunk jump)
    Pattern(String),
    /// Jump between commented lines (set by `c`)
    Comments,
}

pub struct DiffState {
    pub lines: Vec<DiffLine>,
    pub cursor: usize,
    pub scroll_offset: usize,
    pub task_name: String,
    /// Current search/jump mode
    pub search_mode: Option<SearchMode>,
    /// Review comments attached to diff lines
    pub comments: Vec<ReviewComment>,
    /// Currently editing a comment inline (at cursor position)
    pub editing_comment: Option<EditingComment>,
}

impl DiffState {
    /// Run `git diff <upstream>..<branch>` and parse the output.
    /// Returns `(DiffState, Option<warning_message>)`.
    pub fn from_task(
        repo_root: &Path,
        name: &str,
        upstream: &str,
        diff_filter: &DiffFilter,
    ) -> anyhow::Result<(Self, Option<String>)> {
        let raw = get_diff(repo_root, name, upstream)?;
        let (colored, warning) = match diff_filter {
            DiffFilter::None => (None, None),
            DiffFilter::Auto => (colorize_with_delta(&raw), None),
            DiffFilter::Delta => {
                let colored = colorize_with_delta(&raw);
                let warning = if colored.is_none() {
                    Some("diff_filter = \"delta\" but delta is not installed".to_string())
                } else {
                    None
                };
                (colored, warning)
            }
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
            search_mode: None,
            comments: Vec::new(),
            editing_comment: None,
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
        self.search_mode = Some(SearchMode::Pattern(pattern.to_string()));
        self.search_next();
    }

    /// Jump to next match of current search/jump mode (no wrapping).
    /// If no mode is set, defaults to `^@@` (hunk jump, like tig).
    pub fn search_next(&mut self) {
        if self.search_mode.is_none() {
            self.search_mode = Some(SearchMode::Pattern("^@@".to_string()));
        }
        let mode = self.search_mode.clone().unwrap();

        for i in (self.cursor + 1)..self.lines.len() {
            if self.line_matches_mode(i, &mode) {
                self.cursor = i;
                return;
            }
        }
    }

    /// Jump to previous match of current search/jump mode (no wrapping).
    /// If no mode is set, defaults to `^@@` (hunk jump, like tig).
    pub fn search_prev(&mut self) {
        if self.search_mode.is_none() {
            self.search_mode = Some(SearchMode::Pattern("^@@".to_string()));
        }
        let mode = self.search_mode.clone().unwrap();

        for i in (0..self.cursor).rev() {
            if self.line_matches_mode(i, &mode) {
                self.cursor = i;
                return;
            }
        }
    }

    /// Check if a line matches the given search mode.
    fn line_matches_mode(&self, line_index: usize, mode: &SearchMode) -> bool {
        match mode {
            SearchMode::Pattern(pat) => {
                let is_anchor = pat.starts_with('^');
                let needle = if is_anchor { &pat[1..] } else { pat.as_str() };
                self.lines
                    .get(line_index)
                    .map(|l| line_matches(l, needle, is_anchor))
                    .unwrap_or(false)
            }
            SearchMode::Comments => self.has_comment(line_index),
        }
    }

    /// Check if a line matches the current search mode (for highlighting).
    pub fn line_matches_search(&self, line_index: usize) -> bool {
        let Some(mode) = &self.search_mode else {
            return false;
        };
        self.line_matches_mode(line_index, mode)
    }

    /// Ensure the cursor is visible by adjusting scroll_offset.
    /// Accounts for comment lines that take up extra visual space.
    pub fn ensure_cursor_visible(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        } else {
            // Compute visual rows from scroll_offset to cursor (inclusive)
            let mut visual_rows = 0;
            for i in self.scroll_offset..=self.cursor {
                visual_rows += self.line_visual_height(i);
            }
            if visual_rows > viewport_height {
                // Scroll down: find new scroll_offset so cursor fits
                let mut rows_needed = 0;
                let mut new_offset = self.cursor;
                loop {
                    rows_needed += self.line_visual_height(new_offset);
                    if rows_needed > viewport_height || new_offset == 0 {
                        if rows_needed > viewport_height {
                            new_offset += 1;
                        }
                        break;
                    }
                    new_offset -= 1;
                }
                self.scroll_offset = new_offset;
            }
        }
    }

    // -- Review comment methods --

    /// Returns true if the given line index has a comment attached.
    pub fn has_comment(&self, line_index: usize) -> bool {
        self.comments.iter().any(|c| c.line_index == line_index)
    }

    /// Returns the comment at the given line index, if any.
    pub fn comment_at(&self, line_index: usize) -> Option<&ReviewComment> {
        self.comments.iter().find(|c| c.line_index == line_index)
    }

    /// Number of review comments.
    pub fn comment_count(&self) -> usize {
        self.comments.len()
    }

    /// Visual height of a line: 1 + number of comment display lines.
    /// Accounts for multi-line comments and inline editing state.
    pub fn line_visual_height(&self, line_index: usize) -> usize {
        // If editing at this line, use editing text height (preserves trailing newlines for cursor)
        if line_index == self.cursor {
            if let Some(editing) = &self.editing_comment {
                return 1 + count_editing_lines(&editing.text);
            }
        }
        if let Some(comment) = self.comment_at(line_index) {
            1 + count_committed_lines(&comment.text)
        } else {
            1
        }
    }

    /// Derive the file path for a given line index by scanning backwards
    /// for the nearest `diff --git a/... b/...` FileHeader line.
    ///
    /// NOTE: Uses `rsplit_once(" b/")` which can misparse paths containing
    /// literal ` b/` (e.g. `lib/b/config.rs`). This matches the approach used
    /// by tig and delta. A more robust alternative would be to parse `---`/`+++`
    /// header lines instead.
    pub fn file_path_for_line(&self, line_index: usize) -> String {
        for i in (0..=line_index).rev() {
            if self.lines[i].kind == DiffLineKind::FileHeader {
                if let Some(rest) = self.lines[i].content.strip_prefix("diff --git ") {
                    if let Some(b_path) = rest.rsplit_once(" b/") {
                        return b_path.1.to_string();
                    }
                }
            }
        }
        "<unknown>".to_string()
    }

    /// Remove the comment at a given line index, if any.
    pub fn remove_comment(&mut self, line_index: usize) {
        self.comments.retain(|c| c.line_index != line_index);
    }

    /// Whether we are currently editing a comment inline.
    pub fn is_editing(&self) -> bool {
        self.editing_comment.is_some()
    }

    /// Start editing a comment at the current cursor position.
    /// If a comment already exists, loads its text; otherwise starts empty.
    pub fn start_editing(&mut self) {
        let existing_text = self
            .comment_at(self.cursor)
            .map(|c| c.text.clone())
            .unwrap_or_default();
        self.editing_comment = Some(EditingComment {
            text: existing_text,
        });
    }

    /// Commit the editing text as a comment. Empty text = discard.
    pub fn commit_editing(&mut self) {
        if let Some(editing) = self.editing_comment.take() {
            let text = editing.text.trim_end_matches('\n').to_string();
            if text.is_empty() {
                // Discard: also remove any existing comment
                self.remove_comment(self.cursor);
            } else {
                let file_path = self.file_path_for_line(self.cursor);
                self.comments.retain(|c| c.line_index != self.cursor);
                self.comments.push(ReviewComment {
                    line_index: self.cursor,
                    file_path,
                    text,
                });
            }
        }
    }

    /// Cancel editing without saving.
    pub fn cancel_editing(&mut self) {
        self.editing_comment = None;
    }

    /// Format all comments as a structured review prompt for Claude.
    pub fn format_review_prompt(&self) -> String {
        let mut sorted: Vec<&ReviewComment> = self.comments.iter().collect();
        sorted.sort_by_key(|c| c.line_index);

        let mut prompt = String::from("Please address the following code review comments:\n\n");

        for comment in &sorted {
            // Gather context lines (2 before, 2 after)
            let context_start = comment.line_index.saturating_sub(2);
            let context_end = (comment.line_index + 3).min(self.lines.len());
            let mut context_lines = String::new();
            for i in context_start..context_end {
                let line = &self.lines[i];
                let prefix = match line.kind {
                    DiffLineKind::Added => "+",
                    DiffLineKind::Removed => "-",
                    DiffLineKind::Context => " ",
                    DiffLineKind::HunkHeader | DiffLineKind::FileHeader => "",
                };
                let marker = if i == comment.line_index {
                    " // <-- comment"
                } else {
                    ""
                };
                context_lines.push_str(&format!("{prefix}{}{marker}\n", line.content));
            }

            prompt.push_str(&format!(
                "## File: {}\n```diff\n{}```\nComment: {}\n\n",
                comment.file_path, context_lines, comment.text,
            ));
        }

        prompt
    }
}

/// Count display lines for editing text. Uses `split('\n')` so trailing
/// newlines produce an extra line (where the cursor sits).
fn count_editing_lines(text: &str) -> usize {
    if text.is_empty() {
        1
    } else {
        text.split('\n').count()
    }
}

/// Count display lines for committed comment text. Uses `lines()` which
/// ignores trailing newlines, matching `commit_editing`'s `trim_end_matches('\n')`.
fn count_committed_lines(text: &str) -> usize {
    text.lines().count().max(1)
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

    let mut stdin = child.stdin.take().unwrap();
    let output = std::thread::scope(|s| {
        s.spawn(|| {
            let _ = stdin.write_all(raw_diff.as_bytes());
            drop(stdin);
        });
        child.wait_with_output()
    });

    let output = output.ok()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a minimal DiffState from raw unified diff text.
    fn make_state(raw_diff: &str) -> DiffState {
        DiffState::parse(raw_diff, "test-task".to_string(), None)
    }

    /// Fixture: a simple two-file diff for testing.
    const SIMPLE_DIFF: &str = "\
diff --git a/src/foo.rs b/src/foo.rs
--- a/src/foo.rs
+++ b/src/foo.rs
@@ -1,3 +1,4 @@
 use std::io;
+use std::fs;
 fn main() {
-    println!(\"hello\");
+    println!(\"world\");
 }
diff --git a/src/bar.rs b/src/bar.rs
--- a/src/bar.rs
+++ b/src/bar.rs
@@ -1,2 +1,3 @@
 fn bar() {
+    todo!();
 }
";

    // -- count_editing_lines / count_committed_lines --

    #[test]
    fn editing_lines_empty() {
        assert_eq!(count_editing_lines(""), 1);
    }

    #[test]
    fn editing_lines_single() {
        assert_eq!(count_editing_lines("hello"), 1);
    }

    #[test]
    fn editing_lines_trailing_newline() {
        // During editing, trailing newline = cursor on new line
        assert_eq!(count_editing_lines("hello\n"), 2);
    }

    #[test]
    fn editing_lines_multi() {
        assert_eq!(count_editing_lines("a\nb\nc"), 3);
    }

    #[test]
    fn committed_lines_trailing_newline_ignored() {
        // Committed text: trailing newline should not add an extra line
        assert_eq!(count_committed_lines("hello\n"), 1);
    }

    #[test]
    fn committed_lines_multi() {
        assert_eq!(count_committed_lines("a\nb\nc"), 3);
    }

    #[test]
    fn committed_lines_empty() {
        assert_eq!(count_committed_lines(""), 1);
    }

    // -- file_path_for_line --

    #[test]
    fn file_path_for_added_line() {
        let state = make_state(SIMPLE_DIFF);
        // "+use std::fs;" is in the first file (src/foo.rs)
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added && l.content == "use std::fs;")
            .unwrap();
        assert_eq!(state.file_path_for_line(idx), "src/foo.rs");
    }

    #[test]
    fn file_path_for_second_file() {
        let state = make_state(SIMPLE_DIFF);
        // "+    todo!();" is in the second file (src/bar.rs)
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added && l.content == "    todo!();")
            .unwrap();
        assert_eq!(state.file_path_for_line(idx), "src/bar.rs");
    }

    #[test]
    fn file_path_unknown_at_start() {
        // A diff that starts without "diff --git" header
        let state = make_state("+orphan line\n");
        assert_eq!(state.file_path_for_line(0), "<unknown>");
    }

    // -- line_visual_height --

    #[test]
    fn visual_height_no_comment() {
        let state = make_state(SIMPLE_DIFF);
        assert_eq!(state.line_visual_height(0), 1);
    }

    #[test]
    fn visual_height_with_comment() {
        let mut state = make_state(SIMPLE_DIFF);
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added)
            .unwrap();
        state.comments.push(ReviewComment {
            line_index: idx,
            file_path: "test".to_string(),
            text: "single line".to_string(),
        });
        assert_eq!(state.line_visual_height(idx), 2);
    }

    #[test]
    fn visual_height_multiline_comment() {
        let mut state = make_state(SIMPLE_DIFF);
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added)
            .unwrap();
        state.comments.push(ReviewComment {
            line_index: idx,
            file_path: "test".to_string(),
            text: "line1\nline2\nline3".to_string(),
        });
        assert_eq!(state.line_visual_height(idx), 4); // 1 + 3
    }

    #[test]
    fn visual_height_editing() {
        let mut state = make_state(SIMPLE_DIFF);
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added)
            .unwrap();
        state.cursor = idx;
        state.editing_comment = Some(EditingComment {
            text: "a\nb".to_string(),
        });
        assert_eq!(state.line_visual_height(idx), 3); // 1 + 2
    }

    // -- ensure_cursor_visible --

    #[test]
    fn cursor_visible_scrolls_down_with_comments() {
        let mut state = make_state(SIMPLE_DIFF);
        // Add a comment to make a line take 2 visual rows
        let first_added = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added)
            .unwrap();
        state.comments.push(ReviewComment {
            line_index: first_added,
            file_path: "test".to_string(),
            text: "comment".to_string(),
        });
        // Move cursor to last line
        state.cursor = state.lines.len() - 1;
        state.ensure_cursor_visible(5);
        // scroll_offset should have moved forward
        assert!(state.scroll_offset > 0);
    }

    #[test]
    fn cursor_visible_scrolls_up() {
        let mut state = make_state(SIMPLE_DIFF);
        state.scroll_offset = 5;
        state.cursor = 2;
        state.ensure_cursor_visible(10);
        assert_eq!(state.scroll_offset, 2);
    }

    // -- SearchMode::Comments --

    #[test]
    fn search_next_comment() {
        let mut state = make_state(SIMPLE_DIFF);
        let added_indices: Vec<usize> = state
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.kind == DiffLineKind::Added)
            .map(|(i, _)| i)
            .collect();
        assert!(added_indices.len() >= 2);

        // Add comments to first and third added lines
        for &idx in &[added_indices[0], added_indices[2]] {
            state.comments.push(ReviewComment {
                line_index: idx,
                file_path: "test".to_string(),
                text: "comment".to_string(),
            });
        }

        state.cursor = 0;
        state.search_mode = Some(SearchMode::Comments);
        state.search_next();
        assert_eq!(state.cursor, added_indices[0]);

        state.search_next();
        assert_eq!(state.cursor, added_indices[2]);
    }

    #[test]
    fn search_prev_comment() {
        let mut state = make_state(SIMPLE_DIFF);
        let added_indices: Vec<usize> = state
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.kind == DiffLineKind::Added)
            .map(|(i, _)| i)
            .collect();

        for &idx in &[added_indices[0], added_indices[2]] {
            state.comments.push(ReviewComment {
                line_index: idx,
                file_path: "test".to_string(),
                text: "comment".to_string(),
            });
        }

        state.cursor = state.lines.len() - 1;
        state.search_mode = Some(SearchMode::Comments);
        state.search_prev();
        assert_eq!(state.cursor, added_indices[2]);

        state.search_prev();
        assert_eq!(state.cursor, added_indices[0]);
    }

    #[test]
    fn line_matches_search_in_comment_mode() {
        let mut state = make_state(SIMPLE_DIFF);
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added)
            .unwrap();
        state.comments.push(ReviewComment {
            line_index: idx,
            file_path: "test".to_string(),
            text: "x".to_string(),
        });
        state.search_mode = Some(SearchMode::Comments);
        assert!(state.line_matches_search(idx));
        assert!(!state.line_matches_search(0));
    }

    // -- format_review_prompt --

    #[test]
    fn format_prompt_single_comment() {
        let mut state = make_state(SIMPLE_DIFF);
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added && l.content == "use std::fs;")
            .unwrap();
        state.cursor = idx;
        state.editing_comment = Some(EditingComment {
            text: "Why this import?".to_string(),
        });
        state.commit_editing();

        let prompt = state.format_review_prompt();
        assert!(prompt.contains("## File: src/foo.rs"));
        assert!(prompt.contains("// <-- comment"));
        assert!(prompt.contains("Comment: Why this import?"));
        assert!(prompt.contains("```diff"));
    }

    #[test]
    fn format_prompt_multiline_comment() {
        let mut state = make_state(SIMPLE_DIFF);
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added)
            .unwrap();
        state.cursor = idx;
        state.editing_comment = Some(EditingComment {
            text: "line1\nline2".to_string(),
        });
        state.commit_editing();

        let prompt = state.format_review_prompt();
        assert!(prompt.contains("Comment: line1\nline2"));
    }

    #[test]
    fn format_prompt_sorted_by_line_index() {
        let mut state = make_state(SIMPLE_DIFF);
        let added_indices: Vec<usize> = state
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.kind == DiffLineKind::Added)
            .map(|(i, _)| i)
            .collect();

        // Add comments in reverse order
        for (i, &idx) in added_indices.iter().rev().enumerate() {
            state.comments.push(ReviewComment {
                line_index: idx,
                file_path: state.file_path_for_line(idx),
                text: format!("comment-{i}"),
            });
        }

        let prompt = state.format_review_prompt();
        // Comments should appear sorted by line_index, not insertion order
        let all_positions: Vec<usize> = prompt
            .match_indices("comment-")
            .map(|(pos, _)| pos)
            .collect();
        assert!(all_positions.len() >= 2);
        // Verify ascending order of positions (= sorted by line_index)
        for w in all_positions.windows(2) {
            assert!(w[0] < w[1]);
        }
    }

    // -- commit_editing --

    #[test]
    fn commit_editing_trims_trailing_newlines() {
        let mut state = make_state(SIMPLE_DIFF);
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added)
            .unwrap();
        state.cursor = idx;
        state.editing_comment = Some(EditingComment {
            text: "hello\n\n".to_string(),
        });
        state.commit_editing();

        let comment = state.comment_at(idx).unwrap();
        assert_eq!(comment.text, "hello");
    }

    #[test]
    fn commit_editing_preserves_trailing_spaces() {
        let mut state = make_state(SIMPLE_DIFF);
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added)
            .unwrap();
        state.cursor = idx;
        state.editing_comment = Some(EditingComment {
            text: "hello   ".to_string(),
        });
        state.commit_editing();

        let comment = state.comment_at(idx).unwrap();
        assert_eq!(comment.text, "hello   ");
    }

    #[test]
    fn commit_editing_empty_discards() {
        let mut state = make_state(SIMPLE_DIFF);
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added)
            .unwrap();
        state.cursor = idx;
        // First add a comment
        state.editing_comment = Some(EditingComment {
            text: "exists".to_string(),
        });
        state.commit_editing();
        assert!(state.has_comment(idx));

        // Edit to empty → should remove
        state.editing_comment = Some(EditingComment {
            text: "\n\n".to_string(),
        });
        state.commit_editing();
        assert!(!state.has_comment(idx));
    }

    // -- start_editing --

    #[test]
    fn start_editing_loads_existing() {
        let mut state = make_state(SIMPLE_DIFF);
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added)
            .unwrap();
        state.cursor = idx;
        state.editing_comment = Some(EditingComment {
            text: "original".to_string(),
        });
        state.commit_editing();

        state.start_editing();
        assert_eq!(state.editing_comment.as_ref().unwrap().text, "original");
    }

    #[test]
    fn start_editing_empty_for_new() {
        let mut state = make_state(SIMPLE_DIFF);
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added)
            .unwrap();
        state.cursor = idx;
        state.start_editing();
        assert_eq!(state.editing_comment.as_ref().unwrap().text, "");
    }
}
