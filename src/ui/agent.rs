use ratatui::{
    layout::Rect,
    Frame,
};
use tui_term::widget::PseudoTerminal;

use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    // tig-style: no border — PTY output fills the entire area directly
    let task = app
        .focused_task()
        .or_else(|| app.selected_task());

    if let Some(task) = task {
        // Clone the screen while holding the lock, then release before rendering
        // so the PTY reader thread is not blocked during the draw.
        let screen = task
            .parser
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .screen()
            .clone();

        let pseudo_term = PseudoTerminal::new(&screen);
        frame.render_widget(pseudo_term, area);
    }
}
