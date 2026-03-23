use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState},
    Frame,
};

use crate::{app::App, task::TaskStatus};

/// Returns true if the task appears to be waiting for user input.
/// Heuristic: scan the last few non-empty rows of the PTY screen for
/// Claude's prompt character `❯` (U+276F) or `>` at the start of a line.
fn is_waiting_for_input(task: &crate::task::Task) -> bool {
    if task.status != TaskStatus::Running {
        return false;
    }
    let screen = task
        .parser
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .screen()
        .clone();

    let rows = screen.size().0 as usize;
    let cols = screen.size().1 as usize;

    // Check the last 3 non-empty rows for a prompt character
    let mut checked = 0;
    for r in (0..rows).rev() {
        // Build the text content of this row
        let row_text: String = (0..cols)
            .filter_map(|c| screen.cell(r as u16, c as u16).map(|cell| cell.contents().to_string()))
            .collect();
        let trimmed = row_text.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Claude prompt: line starting with ❯ or >
        if trimmed.starts_with('❯') || trimmed.starts_with('>') {
            return true;
        }
        checked += 1;
        if checked >= 3 {
            break;
        }
    }
    false
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    // Pre-compute max column widths for alignment
    let max_name_len = app.tasks.iter().map(|t| t.name.len()).max().unwrap_or(0);
    let max_upstream_len = app
        .tasks
        .iter()
        .map(|t| "(upstream: )".len() + t.upstream.len())
        .max()
        .unwrap_or(0);
    let max_status_len = 7; // "running", "waiting", "stopped" are all 7 chars

    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .map(|task| {
            let (icon, icon_color, status_text) = match task.status {
                TaskStatus::Running => {
                    if is_waiting_for_input(task) {
                        ("⏸ ", Color::Yellow, "waiting")
                    } else {
                        ("▶ ", Color::Green, "running")
                    }
                }
                TaskStatus::Stopped => ("■ ", Color::DarkGray, "stopped"),
            };

            let ahead_text = match task.commits_ahead {
                Some(0) => "synced".to_string(),
                Some(n) => format!("{n} ahead"),
                None => String::new(),
            };

            let upstream_str = format!("(upstream: {})", task.upstream);

            let mut spans = vec![
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::raw(format!("{:<width$}", task.name, width = max_name_len)),
                Span::styled(
                    format!("  {:<width$}", upstream_str, width = max_upstream_len),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("  {:<width$}", status_text, width = max_status_len),
                    Style::default().fg(icon_color),
                ),
            ];
            if !ahead_text.is_empty() {
                spans.push(Span::styled(
                    format!("  {ahead_text}"),
                    Style::default().fg(Color::Indexed(245)),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    // tig-style: no border, no highlight symbol — selected row shown by bg color only
    let list = List::new(items)
        .block(Block::default())
        .highlight_style(
            Style::default()
                .fg(Color::Indexed(166))
                .bg(Color::Indexed(234))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");

    let mut state = ListState::default();
    if !app.tasks.is_empty() {
        state.select(Some(app.selected_index));
    }

    frame.render_stateful_widget(list, area, &mut state);
}
