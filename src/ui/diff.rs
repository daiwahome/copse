use ratatui::{
    layout::{Position, Rect},
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

/// Compute the digit width needed for a column of line numbers.
fn digit_width(max_no: usize) -> usize {
    if max_no == 0 {
        return 0;
    }
    max_no.ilog10() as usize + 1
}

/// Total gutter width: old_digits + space + new_digits + separator.
/// Format: `{old} {new} ` (two columns + trailing separator).
/// Uses cached max values from DiffState to avoid per-frame full scans.
fn gutter_total_width(state: &DiffState) -> (usize, usize, usize) {
    let old_w = digit_width(state.max_old_line_no);
    let new_w = digit_width(state.max_new_line_no);
    if old_w == 0 && new_w == 0 {
        return (0, 0, 0);
    }
    // total = old_digits + " " + new_digits + " "
    (old_w, new_w, old_w + 1 + new_w + 1)
}

/// Build gutter span for a single diff line (old + new columns).
fn gutter_span(
    old_no: Option<usize>,
    new_no: Option<usize>,
    old_w: usize,
    new_w: usize,
    total_w: usize,
) -> Span<'static> {
    let gutter_style = Style::default().fg(Color::DarkGray);
    if total_w == 0 {
        return Span::raw("");
    }
    let old_str = match old_no {
        Some(n) => format!("{n:>old_w$}"),
        None => " ".repeat(old_w),
    };
    let new_str = match new_no {
        Some(n) => format!("{n:>new_w$}"),
        None => " ".repeat(new_w),
    };
    Span::styled(format!("{old_str} {new_str} "), gutter_style)
}

/// Render the diff view with inline comment display.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    state: &mut DiffState,
    focused: bool,
    theme: &Theme,
    show_line_numbers: bool,
) {
    let height = area.height as usize;
    if height == 0 || state.lines.is_empty() {
        return;
    }

    // Ensure cursor is visible (accounts for comment visual heights)
    state.ensure_cursor_visible(height);

    let (old_w, new_w, gutter_w) = if show_line_numbers {
        gutter_total_width(state)
    } else {
        (0, 0, 0)
    };
    let width = area.width as usize;
    let content_width = width.saturating_sub(gutter_w);

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

        // Meta rows (Summary, FileHeader, HunkHeader) span the full terminal
        // width. Line numbers only appear alongside actual code content.
        let is_meta = matches!(
            line.kind,
            DiffLineKind::Summary | DiffLineKind::FileHeader | DiffLineKind::HunkHeader
        );
        let gutter = if is_meta {
            Span::raw("")
        } else {
            gutter_span(line.old_line_no, line.new_line_no, old_w, new_w, gutter_w)
        };
        let line_content_width = if is_meta { width } else { content_width };

        // Delta colors content lines (Added/Removed/Context) well, but does not
        // reliably color FileHeader / HunkHeader lines (they appear white).
        // Summary rows use our own pre-styled ansi_line.
        // → Use ansi_line only for Summary and content; always use fallback for
        //   FileHeader and HunkHeader so our tig-style coloring applies.
        let use_ansi = line.ansi_line.is_some()
            && !matches!(
                line.kind,
                DiffLineKind::FileHeader | DiffLineKind::HunkHeader
            );

        // Render the diff line itself
        if use_ansi {
            let ansi_line = line.ansi_line.as_ref().unwrap();
            // Delta-colored rendering (also used for pre-styled Summary rows)
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

            // Pad to content width
            let content_len: usize = spans.iter().map(|s| s.content.width()).sum();
            let pad = line_content_width.saturating_sub(content_len);
            if pad > 0 {
                let pad_style = spans.last().map(|s| s.style).unwrap_or_default();
                spans.push(Span::styled(" ".repeat(pad), pad_style));
            }

            // Prepend gutter (empty for meta rows)
            spans.insert(0, gutter);
            lines.push(Line::from(spans));
        } else {
            // Fallback: original plain-color rendering.
            // FileHeader sub-types get distinct colors (tig-style):
            //   diff --git  → diff_header (bold)
            //   index ...   → DarkGray (less prominent)
            //   --- a/...   → diff_del (red, like removed lines)
            //   +++ b/...   → diff_add (green, like added lines)
            let (content_style, prefix) = match line.kind {
                DiffLineKind::Added => (theme.diff_add, "+"),
                DiffLineKind::Removed => (theme.diff_del, "-"),
                DiffLineKind::Context => (theme.diff_context, " "),
                DiffLineKind::HunkHeader => (theme.diff_chunk, ""),
                DiffLineKind::FileHeader => {
                    let style = if line.content.starts_with("---") {
                        theme.diff_del
                    } else if line.content.starts_with("+++") {
                        theme.diff_add
                    } else if line.content.starts_with("index ") {
                        Style::default().fg(Color::DarkGray)
                    } else {
                        theme.diff_header
                    };
                    (style, "")
                }
                DiffLineKind::Summary => (theme.diff_header, ""),
            };

            let display_text = if matches!(
                line.kind,
                DiffLineKind::HunkHeader | DiffLineKind::FileHeader | DiffLineKind::Summary
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
                let pad = line_content_width.saturating_sub(display_text.width());
                lines.push(Line::from(vec![
                    gutter,
                    Span::styled(format!("{display_text}{}", " ".repeat(pad)), final_style),
                ]));
            } else {
                lines.push(Line::from(vec![
                    gutter,
                    Span::styled(display_text, final_style),
                ]));
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

    // Set cursor position only when editing a comment (IME input needed).
    // In non-editing mode, leave the cursor hidden to avoid Terminal.app
    // writing inline pre-edit text into the diff content.
    if focused && state.is_editing() && state.cursor >= start && state.cursor < end {
        let mut visual_y = 0usize;
        for idx in start..state.cursor {
            visual_y += state.line_visual_height(idx);
        }
        // Position at the end of comment lines (below the diff line)
        visual_y += state.line_visual_height(state.cursor);
        let cursor_y = area.y + visual_y.saturating_sub(1) as u16;

        // Compute X: prefix width + last line of editing text
        // Note: comment lines are rendered without gutter prefix, so gutter_w is NOT added here.
        let editing_text = state
            .editing_comment
            .as_ref()
            .map(|e| e.text.as_str())
            .unwrap_or("");
        let last_line = editing_text.split('\n').next_back().unwrap_or("");
        let cursor_x =
            area.x + COMMENT_PREFIX.width() as u16 + UnicodeWidthStr::width(last_line) as u16;

        if cursor_y < area.bottom() && cursor_x < area.right() {
            frame.set_cursor_position(Position {
                x: cursor_x,
                y: cursor_y,
            });
        }
    }
}
