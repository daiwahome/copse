use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::diff::{DiffLineKind, DiffState};
use crate::theme::Theme;

/// Render the diff view.
pub fn render(frame: &mut Frame, area: Rect, state: &mut DiffState, focused: bool, theme: &Theme) {
    let height = area.height as usize;
    if height == 0 || state.lines.is_empty() {
        return;
    }

    // Ensure cursor is visible
    state.ensure_cursor_visible(height);

    let start = state.scroll_offset;
    let end = (start + height).min(state.lines.len());

    let width = area.width as usize;

    let lines: Vec<Line> = (start..end)
        .map(|idx| {
            let line = &state.lines[idx];
            let is_cursor = idx == state.cursor;
            let is_search_match = state.line_matches_search(idx);

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

                Line::from(spans)
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
                    Line::from(Span::styled(
                        format!("{display_text}{}", " ".repeat(pad)),
                        final_style,
                    ))
                } else {
                    Line::from(Span::styled(display_text, final_style))
                }
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}
