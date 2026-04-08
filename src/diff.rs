use std::path::Path;

use ansi_to_tui::IntoText;
use ratatui::text::Line;

use crate::config::DiffFilter;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

/// Anchor identifying a diff line by its content rather than index.
#[derive(Debug, Clone)]
pub struct LineAnchor {
    pub file_path: String,
    pub kind: DiffLineKind,
    pub content: String,
}

/// A saved comment with content-based anchor (survives line index shifts).
#[derive(Debug, Clone)]
pub struct SavedComment {
    pub anchor: LineAnchor,
    pub original_index: usize,
    pub text: String,
}

/// Snapshot of DiffState that can survive a refresh or close/reopen.
#[derive(Debug, Clone)]
pub struct SavedDiffState {
    pub task_name: String,
    pub cursor_anchor: Option<LineAnchor>,
    pub cursor_index: usize,
    pub scroll_anchor: Option<LineAnchor>,
    pub scroll_index: usize,
    pub comments: Vec<SavedComment>,
    pub search_mode: Option<SearchMode>,
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
    ) -> anyhow::Result<Self> {
        let raw = get_diff(repo_root, name, upstream)?;
        let colored = diff_filter.colorize(&raw);
        Ok(Self::parse(&raw, name.to_string(), colored.as_deref()))
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

    /// Build a vec mapping each line index to its owning file path. O(n) scan.
    fn build_file_path_map(&self) -> Vec<String> {
        let mut current_path = "<unknown>".to_string();
        self.lines
            .iter()
            .map(|line| {
                if line.kind == DiffLineKind::FileHeader {
                    if let Some(rest) = line.content.strip_prefix("diff --git ") {
                        if let Some(b_path) = rest.rsplit_once(" b/") {
                            current_path = b_path.1.to_string();
                        }
                    }
                }
                current_path.clone()
            })
            .collect()
    }

    /// Save current cursor, scroll, comments, and search state as anchors.
    pub fn save_state(&self) -> SavedDiffState {
        let path_map = self.build_file_path_map();

        let make_anchor = |idx: usize| -> Option<LineAnchor> {
            self.lines.get(idx).map(|line| LineAnchor {
                file_path: path_map[idx].clone(),
                kind: line.kind.clone(),
                content: line.content.clone(),
            })
        };

        let comments = self
            .comments
            .iter()
            .filter_map(|c| {
                self.lines.get(c.line_index).map(|line| SavedComment {
                    anchor: LineAnchor {
                        file_path: path_map[c.line_index].clone(),
                        kind: line.kind.clone(),
                        content: line.content.clone(),
                    },
                    original_index: c.line_index,
                    text: c.text.clone(),
                })
            })
            .collect();

        SavedDiffState {
            task_name: self.task_name.clone(),
            cursor_anchor: make_anchor(self.cursor),
            cursor_index: self.cursor,
            scroll_anchor: make_anchor(self.scroll_offset),
            scroll_index: self.scroll_offset,
            comments,
            search_mode: self.search_mode.clone(),
        }
    }

    /// Restore saved state into this (freshly parsed) DiffState.
    pub fn restore_state(&mut self, saved: &SavedDiffState) {
        if saved.task_name != self.task_name {
            return;
        }

        let path_map = self.build_file_path_map();

        // Find a line matching the anchor, preferring the one closest to `hint`.
        let find_line = |anchor: &LineAnchor, hint: usize| -> Option<usize> {
            self.lines
                .iter()
                .enumerate()
                .filter(|(idx, line)| {
                    line.kind == anchor.kind
                        && line.content == anchor.content
                        && path_map[*idx] == anchor.file_path
                })
                .min_by_key(|(idx, _)| (*idx as isize - hint as isize).unsigned_abs())
                .map(|(idx, _)| idx)
        };

        // Restore cursor
        if let Some(ref anchor) = saved.cursor_anchor {
            self.cursor = find_line(anchor, saved.cursor_index)
                .unwrap_or_else(|| saved.cursor_index.min(self.lines.len().saturating_sub(1)));
        } else {
            self.cursor = saved.cursor_index.min(self.lines.len().saturating_sub(1));
        }

        // Restore scroll offset
        if let Some(ref anchor) = saved.scroll_anchor {
            self.scroll_offset = find_line(anchor, saved.scroll_index)
                .unwrap_or_else(|| saved.scroll_index.min(self.lines.len().saturating_sub(1)));
        } else {
            self.scroll_offset = saved.scroll_index.min(self.lines.len().saturating_sub(1));
        }

        // Restore comments
        for sc in &saved.comments {
            if let Some(new_idx) = find_line(&sc.anchor, sc.original_index) {
                let file_path = path_map[new_idx].clone();
                // Avoid duplicating if already present
                if !self.has_comment(new_idx) {
                    self.comments.push(ReviewComment {
                        line_index: new_idx,
                        file_path,
                        text: sc.text.clone(),
                    });
                }
            }
            // If no match, the comment is dropped (line no longer exists)
        }

        // Restore search mode
        self.search_mode = saved.search_mode.clone();
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

    // -- save_state / restore_state --

    #[test]
    fn save_restore_cursor() {
        let mut state = make_state(SIMPLE_DIFF);
        // Move cursor to the "use std::fs;" added line
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added && l.content == "use std::fs;")
            .unwrap();
        state.cursor = idx;
        state.scroll_offset = 2;

        let saved = state.save_state();

        // Create a fresh state from the same diff (cursor resets to 0)
        let mut new_state = make_state(SIMPLE_DIFF);
        assert_eq!(new_state.cursor, 0);
        new_state.restore_state(&saved);

        assert_eq!(new_state.cursor, idx);
        assert_eq!(new_state.scroll_offset, 2);
    }

    #[test]
    fn save_restore_comments() {
        let mut state = make_state(SIMPLE_DIFF);
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added && l.content == "use std::fs;")
            .unwrap();
        state.comments.push(ReviewComment {
            line_index: idx,
            file_path: "src/foo.rs".to_string(),
            text: "Why this import?".to_string(),
        });

        let saved = state.save_state();
        let mut new_state = make_state(SIMPLE_DIFF);
        new_state.restore_state(&saved);

        assert_eq!(new_state.comments.len(), 1);
        assert_eq!(new_state.comments[0].line_index, idx);
        assert_eq!(new_state.comments[0].text, "Why this import?");
    }

    #[test]
    fn restore_drops_orphaned_comments() {
        let mut state = make_state(SIMPLE_DIFF);
        let idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::Added && l.content == "use std::fs;")
            .unwrap();
        state.comments.push(ReviewComment {
            line_index: idx,
            file_path: "src/foo.rs".to_string(),
            text: "orphan".to_string(),
        });

        let saved = state.save_state();

        // Parse a different diff where "use std::fs;" doesn't exist
        let modified_diff = "\
diff --git a/src/foo.rs b/src/foo.rs
--- a/src/foo.rs
+++ b/src/foo.rs
@@ -1,3 +1,3 @@
 use std::io;
 fn main() {
-    println!(\"hello\");
+    println!(\"world\");
 }
";
        let mut new_state = make_state(modified_diff);
        new_state.restore_state(&saved);

        // Comment should be dropped (no matching line)
        assert_eq!(new_state.comments.len(), 0);
    }

    #[test]
    fn restore_duplicate_lines() {
        // Diff with duplicate blank context lines (space-prefixed as in real git diff).
        // In unified diff, blank context lines are represented as " " (single space).
        let dup_diff = "diff --git a/src/x.rs b/src/x.rs\n--- a/src/x.rs\n+++ b/src/x.rs\n@@ -1,5 +1,5 @@\n \n fn a() {}\n \n fn b() {}\n \n";
        let mut state = make_state(dup_diff);
        // Lines: [FileHeader(diff --git), FileHeader(---), FileHeader(+++),
        //         HunkHeader(@@), Context(blank), Context(fn a), Context(blank),
        //         Context(fn b), Context(blank)]
        assert_eq!(state.lines[4].kind, DiffLineKind::Context);
        assert_eq!(state.lines[4].content, "");
        assert_eq!(state.lines[6].kind, DiffLineKind::Context);
        assert_eq!(state.lines[6].content, "");
        assert_eq!(state.lines[8].kind, DiffLineKind::Context);
        assert_eq!(state.lines[8].content, "");
        // Cursor on the second blank context line (index 6, between fn a and fn b)
        state.cursor = 6;

        let saved = state.save_state();
        let mut new_state = make_state(dup_diff);
        new_state.restore_state(&saved);

        // Should pick the blank context line closest to the original index 6
        assert_eq!(new_state.cursor, 6);
    }

    #[test]
    fn restore_empty_diff() {
        let mut state = make_state(SIMPLE_DIFF);
        state.cursor = 5;
        state.comments.push(ReviewComment {
            line_index: 5,
            file_path: "src/foo.rs".to_string(),
            text: "comment".to_string(),
        });

        let saved = state.save_state();

        let mut empty_state = DiffState::parse("", "test-task".to_string(), None);
        empty_state.restore_state(&saved);

        assert_eq!(empty_state.cursor, 0);
        assert_eq!(empty_state.comments.len(), 0);
    }

    #[test]
    fn save_restore_search_mode() {
        let mut state = make_state(SIMPLE_DIFF);
        state.search_mode = Some(SearchMode::Pattern("hello".to_string()));

        let saved = state.save_state();
        let mut new_state = make_state(SIMPLE_DIFF);
        assert!(new_state.search_mode.is_none());
        new_state.restore_state(&saved);

        assert_eq!(
            new_state.search_mode,
            Some(SearchMode::Pattern("hello".to_string()))
        );
    }

    #[test]
    fn build_file_path_map_correct() {
        let state = make_state(SIMPLE_DIFF);
        let map = state.build_file_path_map();

        assert_eq!(map.len(), state.lines.len());

        // All lines in the first file section should be "src/foo.rs"
        let first_bar_idx = state
            .lines
            .iter()
            .position(|l| l.kind == DiffLineKind::FileHeader && l.content.contains("b/src/bar.rs"))
            .unwrap();
        for i in 0..first_bar_idx {
            assert_eq!(map[i], "src/foo.rs", "line {i} should be src/foo.rs");
        }
        // Lines from bar.rs header onward
        for i in first_bar_idx..map.len() {
            assert_eq!(map[i], "src/bar.rs", "line {i} should be src/bar.rs");
        }
    }
}
