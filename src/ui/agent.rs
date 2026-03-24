use ratatui::{
    layout::Rect,
    Frame,
};
use tui_term::widget::{Cursor, PseudoTerminal};

use crate::app::App;

/// Render the agent PTY view. Returns the actual (clamped) scroll offset.
pub fn render(frame: &mut Frame, area: Rect, app: &App) -> usize {
    // tig-style: no border — PTY output fills the entire area directly
    let task = app
        .focused_task()
        .or_else(|| app.selected_task());

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
            let pseudo_term = PseudoTerminal::new(&screen)
                .cursor(Cursor::default().visibility(false));
            frame.render_widget(pseudo_term, area);
            actual_offset
        } else {
            // Live view: render directly from the parser's screen while
            // holding the lock to avoid cloning the scrollback buffer.
            let guard = task
                .parser
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let screen = guard.screen();
            let pseudo_term = PseudoTerminal::new(screen);
            frame.render_widget(pseudo_term, area);
            0
        }
    } else {
        0
    }
}
