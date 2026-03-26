use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::diff::{DiffLineKind, DiffState};
use crate::theme::Theme;

/// Style for comment lines displayed below commented diff lines.
const COMMENT_BG: Color = Color::Indexed(58); // dark yellow
const COMMENT_FG: Color = Color::Indexed(229); // light yellow text
const COMMENT_PREFIX: &str = "  ┃ ";

/// Render comment lines (confirmed or editing) below a diff line.
/// `text` is the comment content, `is_editing` adds a cursor at the end.
fn render_comment_lines(lines: &mut Vec<Line>, text: &str, width: usize, is_editing: bool) {
    let comment_style = Style::default().fg(COMMENT_FG).bg(COMMENT_BG);
    let cursor_style = Style::default().fg(Color::Yellow).bg(COMMENT_BG);

    let display_lines: Vec<&str> = if text.is_empty() {
        vec![""]
    } else if is_editing {
        // split('\n') preserves trailing newlines (cursor sits on new line)
        text.split('\n').collect()
    } else {
        // lines() ignores trailing newlines (consistent with committed text)
        text.lines().collect()
    };

    let last_idx = display_lines.len() - 1;
    for (i, line_text) in display_lines.iter().enumerate() {
        let content = format!("{COMMENT_PREFIX}{line_text}");
        let is_last = i == last_idx;

        if is_editing && is_last {
            // Show cursor on the last line of editing text
            let cursor_char = "█";
            let pad = width.saturating_sub(content.width() + cursor_char.len());
            lines.push(Line::from(vec![
                Span::styled(content, comment_style),
                Span::styled(cursor_char, cursor_style),
                Span::styled(" ".repeat(pad), comment_style),
            ]));
        } else {
            let pad = width.saturating_sub(content.width());
            lines.push(Line::from(Span::styled(
                format!("{content}{}", " ".repeat(pad)),
                comment_style,
            )));
        }
    }
}

/// Render the diff view with inline comment display.
pub fn render(frame: &mut Frame, area: Rect, state: &mut DiffState, focused: bool, theme: &Theme) {
    let height = area.height as usize;
    if height == 0 || state.lines.is_empty() {
        return;
    }

    // Ensure cursor is visible (accounts for comment visual heights)
    state.ensure_cursor_visible(height);

    let width = area.width as usize;

    // Compute which lines fit in the viewport, accounting for comment lines
    let start = state.scroll_offset;
    let mut visual_rows_used = 0;
    let mut end = start;
    while end < state.lines.len() {
        let line_height = state.line_visual_height(end);
        if visual_rows_used + line_height > height {
            break;
        }
        visual_rows_used += line_height;
        end += 1;
    }

    let mut lines: Vec<Line> = Vec::with_capacity(visual_rows_used);

    for idx in start..end {
        let line = &state.lines[idx];
        let is_cursor = idx == state.cursor;
        let is_search_match = state.line_matches_search(idx);
        let is_editing = is_cursor && state.is_editing();

        // Render the diff line itself
        if let Some(ansi_line) = &line.ansi_line {
            // Delta-colored rendering
            let mut spans: Vec<Span> = ansi_line.spans.clone();

            if is_cursor || is_search_match {
                let overlay = if is_cursor && focused {
                    theme.cursor
                } else if is_cursor {
                    theme.cursor_blur
                } else {
                    theme.search_result
                };
                spans = spans
                    .into_iter()
                    .map(|s| Span::styled(s.content, s.style.patch(overlay)))
                    .collect();
            }

            // Pad to full width
            let content_len: usize = spans.iter().map(|s| s.content.width()).sum();
            let pad = width.saturating_sub(content_len);
            if pad > 0 {
                let pad_style = spans.last().map(|s| s.style).unwrap_or_default();
                spans.push(Span::styled(" ".repeat(pad), pad_style));
            }

            lines.push(Line::from(spans));
        } else {
            // Fallback: original plain-color rendering
            let (content_style, prefix) = match line.kind {
                DiffLineKind::Added => (theme.diff_add, "+"),
                DiffLineKind::Removed => (theme.diff_del, "-"),
                DiffLineKind::Context => (theme.diff_context, " "),
                DiffLineKind::HunkHeader => (theme.diff_chunk, ""),
                DiffLineKind::FileHeader => (theme.diff_header, ""),
            };

            let display_text = if matches!(
                line.kind,
                DiffLineKind::HunkHeader | DiffLineKind::FileHeader
            ) {
                line.content.clone()
            } else {
                format!("{prefix}{}", line.content)
            };

            let final_style = if is_cursor && focused {
                content_style.patch(theme.cursor)
            } else if is_cursor {
                theme.cursor_blur
            } else if is_search_match {
                content_style.patch(theme.search_result)
            } else {
                content_style
            };

            if is_cursor || is_search_match {
                let pad = width.saturating_sub(display_text.width());
                lines.push(Line::from(Span::styled(
                    format!("{display_text}{}", " ".repeat(pad)),
                    final_style,
                )));
            } else {
                lines.push(Line::from(Span::styled(display_text, final_style)));
            }
        }

        // Render comment lines below (editing takes precedence over confirmed)
        if is_editing {
            let text = state
                .editing_comment
                .as_ref()
                .map(|e| e.text.as_str())
                .unwrap_or("");
            render_comment_lines(&mut lines, text, width, true);
        } else if let Some(comment) = state.comment_at(idx) {
            render_comment_lines(&mut lines, &comment.text, width, false);
        }
    }

    // Fill remaining viewport rows if needed
    while lines.len() < height {
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}
