use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::diff::{DiffLineKind, DiffState};

/// Render the diff view.
pub fn render(frame: &mut Frame, area: Rect, state: &mut DiffState, focused: bool) {
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
                DiffLineKind::Added => (Style::default().fg(Color::Green), "+"),
                DiffLineKind::Removed => (Style::default().fg(Color::Red), "-"),
                DiffLineKind::Context => (Style::default().fg(Color::White), " "),
                DiffLineKind::HunkHeader => (
                    Style::default().fg(Color::Cyan),
                    "",
                ),
                DiffLineKind::FileHeader => (
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                    "",
                ),
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
                content_style.bg(Color::Indexed(236))
            } else if is_cursor {
                Style::default().fg(Color::Indexed(252)).bg(Color::Indexed(234))
            } else if is_search_match {
                content_style.bg(Color::Indexed(238))
            } else {
                content_style
            };

            if is_cursor || is_search_match {
                // Pad to full width so background color extends to the right edge
                let width = area.width as usize;
                let pad = width.saturating_sub(display_text.len());
                Line::from(Span::styled(format!("{display_text}{}", " ".repeat(pad)), final_style))
            } else {
                Line::from(Span::styled(display_text, final_style))
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}
