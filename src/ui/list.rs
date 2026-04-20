use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::{app::App, config::Agent, task::TaskStatus};

/// Returns the brand color used for each agent's icon in the task list.
/// Kept in the UI layer to avoid coupling `src/agent.rs` to ratatui.
fn agent_icon_color(agent: &Agent) -> Color {
    match agent {
        Agent::ClaudeCode => Color::Indexed(166), // Claude orange
        Agent::Codex => Color::Indexed(36),       // teal
        Agent::CopilotCli => Color::Indexed(75),  // Copilot blue
    }
}

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
    let max_agents_len: usize = 6; // "Agents".len() — accommodates header and future multi-agent display

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
                Span::raw("  "),
            ];
            // Agents column: show the task's current agent plus any other agent
            // that has a session marker. Active agent is colored when the task
            // is running; otherwise all shown agents are grey. Session-marker
            // lookup uses the `session_agents` cache on the Task to avoid
            // per-frame filesystem stats.
            let is_running = task.status == TaskStatus::Running;
            let agents_to_show: Vec<&Agent> = Agent::all()
                .iter()
                .filter(|a| **a == task.agent || task.has_marker_for(a))
                .collect();
            for (i, agent) in agents_to_show.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw(" "));
                }
                let color = if is_running && **agent == task.agent {
                    agent_icon_color(agent)
                } else {
                    Color::DarkGray
                };
                spans.push(Span::styled(agent.icon(), Style::default().fg(color)));
            }
            // Pad agent cell to `max_agents_len`. Use unicode-width so that
            // icons with varying terminal widths (including any future additions)
            // align correctly; inter-icon spaces contribute 1 col each.
            let icons_width: usize = agents_to_show.iter().map(|a| a.icon().width()).sum();
            let separators_width = agents_to_show.len().saturating_sub(1);
            let rendered_width = icons_width + separators_width;
            let pad = max_agents_len.saturating_sub(rendered_width);
            if pad > 0 {
                spans.push(Span::raw(" ".repeat(pad)));
            }
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
        Span::styled(
            format!("  {:<width$}", "Agents", width = max_agents_len),
            header_style,
        ),
        Span::styled("  Commits", header_style),
    ]);
    frame.render_widget(Paragraph::new(header_line), header_area);

    frame.render_stateful_widget(list, list_area, &mut state);
}
