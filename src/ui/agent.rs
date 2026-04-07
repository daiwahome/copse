use ratatui::{
    layout::{Position, Rect},
    Frame,
};
use tui_term::widget::{Cursor, PseudoTerminal};

use crate::app::App;

/// Render the agent PTY view.
/// Returns (actual_scroll_offset, optional_cursor_position).
/// The cursor position is in absolute screen coordinates, suitable for
/// `frame.set_cursor_position()`. It is `None` when scrolled back or
/// when the cursor is outside the visible area.
pub fn render(frame: &mut Frame, area: Rect, app: &App) -> (usize, Option<Position>) {
    let task = app.focused_task().or_else(|| app.selected_task());

    if let Some(task) = task {
        if task.scroll_offset > 0 {
            // Scrolled back: clone the screen so we can apply the scrollback
            // offset without affecting the live parser state.
            let mut screen = task
                .parser
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .screen()
                .clone();
            screen.set_scrollback(task.scroll_offset);
            let actual_offset = screen.scrollback();
            let pseudo_term =
                PseudoTerminal::new(&screen).cursor(Cursor::default().visibility(false));
            frame.render_widget(pseudo_term, area);
            (actual_offset, None)
        } else {
            // Live view: render directly from the parser's screen while
            // holding the lock to avoid cloning the scrollback buffer.
            let guard = task.parser.lock().unwrap_or_else(|e| e.into_inner());
            let screen = guard.screen();
            let pseudo_term = PseudoTerminal::new(screen);
            frame.render_widget(pseudo_term, area);

            // Compute absolute cursor position for IME support.
            // Use the PTY cursor position even when the cursor is hidden,
            // so that the hardware cursor stays at a known location and
            // prevents Terminal.app's IME from drifting to an arbitrary cell.
            let (row, col) = screen.cursor_position();
            let abs_x = area.x + col;
            let abs_y = area.y + row;
            let cursor_pos = if abs_x < area.right() && abs_y < area.bottom() {
                Some(Position { x: abs_x, y: abs_y })
            } else {
                None
            };

            (0, cursor_pos)
        }
    } else {
        (0, None)
    }
}
