use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

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

    let lines: Vec<Line> = (start..end)
        .map(|idx| {
            let line = &state.lines[idx];
            let is_cursor = idx == state.cursor;
            let is_search_match = state.line_matches_search(idx);

            // Line content styling based on kind
            let (content_style, prefix) = match line.kind {
                DiffLineKind::Added => (theme.diff_add, "+"),
                DiffLineKind::Removed => (theme.diff_del, "-"),
                DiffLineKind::Context => (theme.diff_context, " "),
                DiffLineKind::HunkHeader => (theme.diff_chunk, ""),
                DiffLineKind::FileHeader => (theme.diff_header, ""),
            };

            // For headers, show the raw content; for code lines, show prefix + content
            let display_text = if matches!(
                line.kind,
                DiffLineKind::HunkHeader | DiffLineKind::FileHeader
            ) {
                line.content.clone()
            } else {
                format!("{prefix}{}", line.content)
            };

            // Apply cursor and search match background highlights
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
                // Pad to full width so background color extends to the right edge
                let width = area.width as usize;
                let pad = width.saturating_sub(display_text.len());
                Line::from(Span::styled(
                    format!("{display_text}{}", " ".repeat(pad)),
                    final_style,
                ))
            } else {
                Line::from(Span::styled(display_text, final_style))
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}
