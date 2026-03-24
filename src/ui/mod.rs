mod agent;
mod list;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, Mode};

pub fn render(frame: &mut Frame, app: &App) {
    match &app.mode {
        Mode::Agent { full: false } => {
            render_split(frame, frame.area(), app);
        }
        _ => {
            // Full-screen: content on top, single status bar at bottom
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Fill(1), Constraint::Length(1)])
                .split(frame.area());

            let scroll_offset = render_main_full(frame, chunks[0], app);
            render_single_status_bar(frame, chunks[1], app, scroll_offset);
        }
    }
}

/// Split view: left pane (tasks) + vertical divider + right pane (agent).
/// Each pane has its own status bar at the bottom, like tig.
fn render_split(frame: &mut Frame, area: Rect, app: &App) {
    let list_width = (area.width / 2).max(20);

    // Horizontal split: left | divider (1 col) | right
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(list_width),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(area);

    let left_area = cols[0];
    let divider_area = cols[1];
    let right_area = cols[2];

    // Draw the vertical divider line (tig uses a plain │ column)
    let divider_block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(divider_block, divider_area);

    // Left pane: content + status bar
    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(left_area);
    list::render(frame, left_rows[0], app);
    render_tasks_status_bar(frame, left_rows[1], app);

    // Right pane: content + status bar
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(right_area);
    let scroll_offset = agent::render(frame, right_rows[0], app);
    render_agent_status_bar(frame, right_rows[1], app, false, scroll_offset);
}

/// Full-screen main content area (Tasks, NewTask, ConfirmQuit, Agent full).
/// Returns the actual (clamped) scroll offset when in Agent mode, 0 otherwise.
fn render_main_full(frame: &mut Frame, area: Rect, app: &App) -> usize {
    match &app.mode {
        Mode::Agent { full: true, .. } => {
            agent::render(frame, area, app)
        }
        _ => {
            list::render(frame, area, app);
            match &app.mode {
                Mode::NewTask { input } => render_new_task_dialog(frame, area, input),
                Mode::NewTaskUpstream { name, branches, selected } => render_new_task_upstream_dialog(frame, area, name, branches, *selected),
                Mode::ConfirmQuit => render_confirm_quit_dialog(frame, area, app),
                Mode::ConfirmKill => render_confirm_kill_dialog(frame, area, app),
                Mode::ConfirmDelete => render_confirm_delete_dialog(frame, area, app),
                Mode::ConfirmSync => render_confirm_sync_dialog(frame, area, app),
                Mode::ConfirmMerge => render_confirm_merge_dialog(frame, area, app),
                Mode::ChangeUpstream { branches, selected } => render_change_upstream_dialog(frame, area, app, branches, *selected),
                _ => {}
            }
            0
        }
    }
}

fn render_new_task_dialog(frame: &mut Frame, area: Rect, input: &str) {
    use ratatui::layout::Alignment;
    use ratatui::widgets::Clear;

    let dialog_width = 50u16.min(area.width.saturating_sub(4));
    let dialog_height = 5u16.min(area.height);
    if dialog_width == 0 || dialog_height == 0 {
        return;
    }
    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" New Task ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let text = Paragraph::new(vec![
        Line::from("Task name:"),
        Line::from(vec![
            Span::raw(input),
            Span::styled("█", Style::default().fg(Color::Yellow)),
        ]),
    ])
    .alignment(Alignment::Left);
    frame.render_widget(text, inner);
}

fn render_new_task_upstream_dialog(frame: &mut Frame, area: Rect, name: &str, branches: &[String], selected: usize) {
    use ratatui::widgets::Clear;

    // Height: 2 header lines + branch list (capped at 10)
    let visible_branches = branches.len().min(10) as u16;
    let dialog_width = 50u16.min(area.width.saturating_sub(4));
    let dialog_height = (4 + visible_branches).min(area.height);
    if dialog_width == 0 || dialog_height == 0 {
        return;
    }
    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" New Task ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Name: ", Style::default().fg(Color::DarkGray)),
            Span::raw(name),
        ]),
        Line::from("Upstream branch:"),
    ];

    // Scroll window: keep selected item visible
    let max_visible = visible_branches as usize;
    let scroll_offset = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    for (i, branch) in branches.iter().enumerate().skip(scroll_offset).take(max_visible) {
        if i == selected {
            lines.push(Line::from(Span::styled(
                format!("> {branch}"),
                Style::default()
                    .fg(Color::Indexed(166))
                    .add_modifier(Modifier::BOLD),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                format!("  {branch}"),
                Style::default().fg(Color::Indexed(252)),
            )));
        }
    }

    let text = Paragraph::new(lines);
    frame.render_widget(text, inner);
}

fn render_confirm_quit_dialog(frame: &mut Frame, area: Rect, app: &App) {
    use ratatui::layout::Alignment;
    use ratatui::widgets::Clear;

    let running_count = app
        .tasks
        .iter()
        .filter(|t| t.status == crate::task::TaskStatus::Running)
        .count();

    let dialog_width = 52u16.min(area.width.saturating_sub(4));
    let dialog_height = 6u16.min(area.height);
    if dialog_width == 0 || dialog_height == 0 {
        return;
    }
    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Quit copse? ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let msg = format!(
        "{} running task{} will be terminated.",
        running_count,
        if running_count == 1 { "" } else { "s" }
    );
    let text = Paragraph::new(vec![
        Line::from(Span::styled(msg, Style::default().fg(Color::Yellow))),
        Line::from(""),
        Line::from(vec![
            Span::styled("[y]", Style::default().fg(Color::Green)),
            Span::raw(" quit  "),
            Span::styled("[n/Esc]", Style::default().fg(Color::DarkGray)),
            Span::raw(" cancel"),
        ]),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(text, inner);
}

fn render_confirm_kill_dialog(frame: &mut Frame, area: Rect, app: &App) {
    use ratatui::layout::Alignment;
    use ratatui::widgets::Clear;

    let name = app
        .selected_task()
        .map(|t| t.name.as_str())
        .unwrap_or("?");

    let dialog_width = 52u16.min(area.width.saturating_sub(4));
    let dialog_height = 6u16.min(area.height);
    if dialog_width == 0 || dialog_height == 0 {
        return;
    }
    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Kill task? ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let msg = format!("Terminate claude in '{name}'?");
    let text = Paragraph::new(vec![
        Line::from(Span::styled(msg, Style::default().fg(Color::Yellow))),
        Line::from(""),
        Line::from(vec![
            Span::styled("[y]", Style::default().fg(Color::Green)),
            Span::raw(" kill  "),
            Span::styled("[n/Esc]", Style::default().fg(Color::DarkGray)),
            Span::raw(" cancel"),
        ]),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(text, inner);
}

fn render_confirm_delete_dialog(frame: &mut Frame, area: Rect, app: &App) {
    use ratatui::layout::Alignment;
    use ratatui::widgets::Clear;

    let name = app
        .selected_task()
        .map(|t| t.name.as_str())
        .unwrap_or("?");

    let dialog_width = 52u16.min(area.width.saturating_sub(4));
    let dialog_height = 6u16.min(area.height);
    if dialog_width == 0 || dialog_height == 0 {
        return;
    }
    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Delete task? ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let msg = format!("Delete '{name}' (worktree + branch)?");
    let text = Paragraph::new(vec![
        Line::from(Span::styled(msg, Style::default().fg(Color::Yellow))),
        Line::from(""),
        Line::from(vec![
            Span::styled("[y]", Style::default().fg(Color::Green)),
            Span::raw(" delete  "),
            Span::styled("[n/Esc]", Style::default().fg(Color::DarkGray)),
            Span::raw(" cancel"),
        ]),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(text, inner);
}

fn render_confirm_sync_dialog(frame: &mut Frame, area: Rect, app: &App) {
    use ratatui::layout::Alignment;
    use ratatui::widgets::Clear;

    let (name, upstream) = app
        .selected_task()
        .map(|t| (t.name.as_str(), t.upstream.as_str()))
        .unwrap_or(("?", "?"));

    let dialog_width = 52u16.min(area.width.saturating_sub(4));
    let dialog_height = 6u16.min(area.height);
    if dialog_width == 0 || dialog_height == 0 {
        return;
    }
    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Sync from upstream? ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let msg = format!("Reset '{name}' to '{upstream}'?");
    let text = Paragraph::new(vec![
        Line::from(Span::styled(msg, Style::default().fg(Color::Yellow))),
        Line::from(""),
        Line::from(vec![
            Span::styled("[y]", Style::default().fg(Color::Green)),
            Span::raw(" sync  "),
            Span::styled("[n/Esc]", Style::default().fg(Color::DarkGray)),
            Span::raw(" cancel"),
        ]),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(text, inner);
}

fn render_confirm_merge_dialog(frame: &mut Frame, area: Rect, app: &App) {
    use ratatui::layout::Alignment;
    use ratatui::widgets::Clear;

    let (name, upstream) = app
        .selected_task()
        .map(|t| (t.name.as_str(), t.upstream.as_str()))
        .unwrap_or(("?", "?"));

    let dialog_width = 52u16.min(area.width.saturating_sub(4));
    let dialog_height = 6u16.min(area.height);
    if dialog_width == 0 || dialog_height == 0 {
        return;
    }
    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Merge to upstream ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let msg = format!("Merge '{name}' into '{upstream}'?");
    let text = Paragraph::new(vec![
        Line::from(Span::styled(msg, Style::default().fg(Color::Yellow))),
        Line::from(""),
        Line::from(vec![
            Span::styled("[f]", Style::default().fg(Color::Green)),
            Span::raw(" fast-forward  "),
            Span::styled("[s]", Style::default().fg(Color::Green)),
            Span::raw(" squash  "),
            Span::styled("[Esc]", Style::default().fg(Color::DarkGray)),
            Span::raw(" cancel"),
        ]),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(text, inner);
}

fn render_change_upstream_dialog(frame: &mut Frame, area: Rect, app: &App, branches: &[String], selected: usize) {
    use ratatui::widgets::Clear;

    let name = app
        .selected_task()
        .map(|t| t.name.as_str())
        .unwrap_or("?");

    // Height: 2 header lines + branch list (capped at 10)
    let visible_branches = branches.len().min(10) as u16;
    let dialog_width = 50u16.min(area.width.saturating_sub(4));
    let dialog_height = (4 + visible_branches).min(area.height);
    if dialog_width == 0 || dialog_height == 0 {
        return;
    }
    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Change Upstream ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Task: ", Style::default().fg(Color::DarkGray)),
            Span::raw(name),
        ]),
        Line::from("Select upstream branch:"),
    ];

    // Scroll window: keep selected item visible
    let max_visible = visible_branches as usize;
    let scroll_offset = if selected >= max_visible {
        selected - max_visible + 1
    } else {
        0
    };

    for (i, branch) in branches.iter().enumerate().skip(scroll_offset).take(max_visible) {
        if i == selected {
            lines.push(Line::from(Span::styled(
                format!("> {branch}"),
                Style::default()
                    .fg(Color::Indexed(166))
                    .add_modifier(Modifier::BOLD),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                format!("  {branch}"),
                Style::default().fg(Color::Indexed(252)),
            )));
        }
    }

    let text = Paragraph::new(lines);
    frame.render_widget(text, inner);
}

/// Single status bar for full-screen modes (Tasks, Agent full, dialogs).
fn render_single_status_bar(frame: &mut Frame, area: Rect, app: &App, scroll_offset: usize) {
    let width = area.width as usize;

    if let Some(err) = &app.last_error {
        let text = format!(" {err}");
        let padding = " ".repeat(width.saturating_sub(text.len()));
        let bar = Paragraph::new(Line::from(Span::styled(
            format!("{text}{padding}"),
            Style::default().fg(Color::White).bg(Color::Red),
        )));
        frame.render_widget(bar, area);
        return;
    }

    match &app.mode {
        Mode::Agent { .. } => {
            render_agent_status_bar(frame, area, app, true, scroll_offset);
        }
        _ => {
            // Tasks, ConfirmQuit, ConfirmKill, NewTask all show the tasks bar
            render_tasks_status_bar(frame, area, app);
        }
    }
}

/// Status bar for the tasks (left) pane.
fn render_tasks_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width as usize;

    // Show error in left pane status bar when in split view
    if let Some(err) = &app.last_error {
        let text = format!(" {err}");
        let padding = " ".repeat(width.saturating_sub(text.len()));
        let bar = Paragraph::new(Line::from(Span::styled(
            format!("{text}{padding}"),
            Style::default().fg(Color::White).bg(Color::Red),
        )));
        frame.render_widget(bar, area);
        return;
    }

    let left = if let Some(task) = app.selected_task() {
        let st = match task.status {
            crate::task::TaskStatus::Running => "running",
            crate::task::TaskStatus::Stopped => "stopped",
        };
        format!(" {} - {}", task.name, st)
    } else {
        String::new()
    };

    let hints: &[(&str, &str)] = match &app.mode {
        Mode::Agent { .. } => &[("j/k", "select")],
        Mode::ConfirmKill => &[("y", "kill"), ("n/Esc", "cancel")],
        Mode::ConfirmDelete => &[("y", "delete"), ("n/Esc", "cancel")],
        Mode::ConfirmSync => &[("y", "sync"), ("n/Esc", "cancel")],
        Mode::ConfirmMerge => &[("f", "ff"), ("s", "squash"), ("Esc", "cancel")],
        Mode::ChangeUpstream { .. } => &[("j/k", "select"), ("Enter", "confirm"), ("Esc", "cancel")],
        _ => &[
            ("n", "new"),
            ("Ctrl-k", "kill"),
            ("S-M", "merge"),
            ("S-S", "sync"),
            ("S-U", "upstream"),
            ("!", "delete"),
            ("Enter", "open"),
            ("q", "quit"),
        ],
    };

    render_status_bar_line(frame, area, &left, hints);
}

/// Status bar for the agent (right) pane.
fn render_agent_status_bar(frame: &mut Frame, area: Rect, app: &App, full: bool, scroll_offset: usize) {
    let name = app
        .focused_task()
        .or_else(|| app.selected_task())
        .map(|t| t.name.as_str())
        .unwrap_or("");

    let mut location = if full {
        format!(" {name} - fullscreen")
    } else {
        format!(" {name}")
    };

    if scroll_offset > 0 {
        location.push_str(&format!(" [SCROLL +{}]", scroll_offset));
    }

    let hints: &[(&str, &str)] = if full {
        &[("Ctrl-b/f", "scroll"), ("Ctrl-]", "split")]
    } else {
        &[("Ctrl-b/f", "scroll"), ("Ctrl-]", "back")]
    };

    render_agent_status_bar_line(frame, area, &location, hints);
}

/// Render the agent status bar with an AGENT badge on the left.
fn render_agent_status_bar_line(
    frame: &mut Frame,
    area: Rect,
    location: &str,
    hints: &[(&str, &str)],
) {
    use ratatui::text::Text;

    // Right-side hints in a muted style
    let right_str = {
        let s = hints
            .iter()
            .map(|(k, d)| format!("{k}:{d}"))
            .collect::<Vec<_>>()
            .join("  ");
        format!("{s} ")
    };

    // AGENT label badge (same orange as TASKS, for visual pairing)
    let agent_label = " AGENT ";

    // Compute padding between location text and right hints
    let agent_len = agent_label.len();
    let location_len = location.len();
    let right_len = right_str.len();
    let width = area.width as usize;
    let gap = width
        .saturating_sub(agent_len + location_len + right_len);

    let line = Line::from(vec![
        // AGENT label: orange like TASKS badge
        Span::styled(
            agent_label,
            Style::default()
                .fg(Color::Black)
                .bg(Color::Indexed(166))
                .add_modifier(Modifier::BOLD),
        ),
        // Location text on dark bg
        Span::styled(
            format!("{location}{}", " ".repeat(gap)),
            Style::default()
                .fg(Color::Indexed(252))
                .bg(Color::Indexed(234)),
        ),
        // Right hints
        Span::styled(
            right_str,
            Style::default()
                .fg(Color::Indexed(245))
                .bg(Color::Indexed(234)),
        ),
    ]);

    frame.render_widget(Paragraph::new(Text::from(line)), area);
}

/// Render a single tig-style status bar line for the tasks pane.
/// Left: orange [TASKS] badge + location text. Right: key hints.
fn render_status_bar_line(frame: &mut Frame, area: Rect, left: &str, hints: &[(&str, &str)]) {
    use ratatui::text::Text;

    let badge = " TASKS ";

    let right_str = {
        let s = hints
            .iter()
            .map(|(k, d)| format!("{k}:{d}"))
            .collect::<Vec<_>>()
            .join("  ");
        format!("{s} ")
    };

    let badge_len = badge.len();
    let left_len = left.len();
    let right_len = right_str.len();
    let width = area.width as usize;
    let gap = width.saturating_sub(badge_len + left_len + right_len);

    let line = Line::from(vec![
        // Orange badge
        Span::styled(
            badge,
            Style::default()
                .fg(Color::Black)
                .bg(Color::Indexed(166))
                .add_modifier(Modifier::BOLD),
        ),
        // Location text
        Span::styled(
            format!("{left}{}", " ".repeat(gap)),
            Style::default()
                .fg(Color::Indexed(252))
                .bg(Color::Indexed(234)),
        ),
        // Right hints
        Span::styled(
            right_str,
            Style::default()
                .fg(Color::Indexed(245))
                .bg(Color::Indexed(234)),
        ),
    ]);

    frame.render_widget(Paragraph::new(Text::from(line)), area);
}
