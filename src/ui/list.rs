use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::{app::App, task::TaskStatus};

pub fn render(frame: &mut Frame, area: Rect, app: &App, focused: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Fill(1)])
        .split(area);
    let header_area = chunks[0];
    let list_area = chunks[1];

    // Pre-compute max column widths for alignment
    let max_name_len = app
        .tasks
        .iter()
        .map(|t| t.name.len())
        .max()
        .unwrap_or(0)
        .max(4); // "Name".len()
    let max_upstream_len = app
        .tasks
        .iter()
        .map(|t| match &t.upstream {
            Some(u) => u.len(),
            None => 1, // "-"
        })
        .max()
        .unwrap_or(0)
        .max(8); // "Upstream".len()
    let max_status_len = 8; // "deleting" is the longest status text

    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .map(|task| {
            let (icon, icon_color, status_text) = match task.status {
                TaskStatus::Running => {
                    if task.waiting_for_input {
                        ("⏸ ", Color::Yellow, "waiting")
                    } else {
                        ("▶ ", Color::Green, "running")
                    }
                }
                TaskStatus::Stopped => ("■ ", Color::DarkGray, "stopped"),
                TaskStatus::Deleting => ("✕ ", Color::Red, "deleting"),
            };

            let ahead_text = if task.upstream_exists {
                match task.commits_ahead {
                    Some(0) => "synced".to_string(),
                    Some(n) => format!("{n} ahead"),
                    None => String::new(),
                }
            } else {
                String::new()
            };

            let upstream_str = match &task.upstream {
                Some(u) => u.clone(),
                None => "-".to_string(),
            };
            let upstream_color = if task.upstream_exists {
                Color::DarkGray
            } else {
                Color::Red
            };

            let mut spans = vec![
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::raw(format!("{:<width$}", task.name, width = max_name_len)),
                Span::styled(
                    format!("  {:<width$}", upstream_str, width = max_upstream_len),
                    Style::default().fg(upstream_color),
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
    let theme = &app.theme;
    let highlight = if focused {
        theme.list_highlight
    } else {
        theme.list_highlight_blur
    };
    let list = List::new(items)
        .block(Block::default())
        .highlight_style(highlight)
        .highlight_symbol("");

    let mut state = ListState::default();
    if !app.tasks.is_empty() {
        state.select(Some(app.selected_index));
    }

    // Render header
    let header_style = app.theme.list_header;
    let header_line = Line::from(vec![
        Span::styled("  ", header_style),
        Span::styled(
            format!("{:<width$}", "Name", width = max_name_len),
            header_style,
        ),
        Span::styled(
            format!("  {:<width$}", "Upstream", width = max_upstream_len),
            header_style,
        ),
        Span::styled(
            format!("  {:<width$}", "Status", width = max_status_len),
            header_style,
        ),
        Span::styled("  Commits", header_style),
    ]);
    frame.render_widget(Paragraph::new(header_line), header_area);

    frame.render_stateful_widget(list, list_area, &mut state);
}
