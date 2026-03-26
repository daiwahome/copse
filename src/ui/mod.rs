mod agent;
mod diff;
mod list;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, ChildView, Dialog, Pane, View, ViewLayout};
use crate::diff::DiffState;

fn diff_state_mut(view_stack: &mut [ChildView]) -> Option<&mut DiffState> {
    view_stack.iter_mut().find_map(|v| match v {
        ChildView::Diff(s) => Some(s),
        _ => None,
    })
}

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    match app.layout() {
        ViewLayout::Single(View::Tasks) | ViewLayout::Fullscreen(View::Tasks) => {
            render_single_tasks(frame, area, app);
        }
        ViewLayout::Single(View::Agent) | ViewLayout::Fullscreen(View::Agent) => {
            render_single_agent(frame, area, app);
        }
        ViewLayout::Single(View::Diff) | ViewLayout::Fullscreen(View::Diff) => {
            render_single_diff(frame, area, app);
        }
        ViewLayout::Split(View::Tasks, View::Agent) => {
            render_split_tasks_agent(frame, area, app);
        }
        ViewLayout::Split(View::Tasks, View::Diff) => {
            render_split_tasks_diff(frame, area, app);
        }
        ViewLayout::Split(View::Diff, View::Agent) => {
            render_split_diff_agent(frame, area, app);
        }
        _ => {
            render_single_tasks(frame, area, app);
        }
    }
}

/// Single tasks view (full screen task list + status bar).
fn render_single_tasks(frame: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(area);

    render_tasks_pane(frame, chunks[0], app, true);
    render_tasks_status_bar(frame, chunks[1], app);
}

/// Single/fullscreen agent view.
fn render_single_agent(frame: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(area);
    let scroll_offset = agent::render(frame, chunks[0], app);
    render_agent_status_bar(frame, chunks[1], app, true, scroll_offset, true);
}

/// Single/fullscreen diff view.
fn render_single_diff(frame: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(area);
    if let Some(state) = diff_state_mut(&mut app.view_stack) {
        diff::render(frame, chunks[0], state, true, &app.theme);
    }
    render_diff_status_bar(frame, chunks[1], app, true);
}

/// Split view: [tasks | agent]
fn render_split_tasks_agent(frame: &mut Frame, area: Rect, app: &mut App) {
    let focus = app.focus;
    let list_width = (area.width / 2).max(20);

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

    let divider_block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(divider_block, divider_area);

    // Left pane: task list
    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(left_area);
    render_tasks_pane(frame, left_rows[0], app, focus == Pane::Left);
    let hints: &[(&str, &str)] = if focus == Pane::Left {
        &[
            ("j/k", "select"),
            ("Enter", "diff"),
            ("a", "agent"),
            ("C-a", "fresh"),
            ("C-w", "focus"),
            ("q", "back"),
        ]
    } else {
        &[("C-w", "focus")]
    };
    render_split_tasks_status_bar(frame, left_rows[1], app, focus, hints);

    // Right pane: agent
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(right_area);
    let scroll_offset = agent::render(frame, right_rows[0], app);
    render_agent_status_bar(
        frame,
        right_rows[1],
        app,
        false,
        scroll_offset,
        focus == Pane::Right,
    );
}

/// Split view: [tasks | diff]
fn render_split_tasks_diff(frame: &mut Frame, area: Rect, app: &mut App) {
    let focus = app.focus;
    let list_width = (area.width / 2).max(20);

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

    let divider_block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(divider_block, divider_area);

    // Left pane: task list
    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(left_area);
    render_tasks_pane(frame, left_rows[0], app, focus == Pane::Left);
    let hints: &[(&str, &str)] = if focus == Pane::Left {
        &[
            ("j/k", "select"),
            ("a", "agent"),
            ("C-a", "fresh"),
            ("C-w", "diff"),
            ("q", "back"),
        ]
    } else {
        &[("C-w", "focus")]
    };
    render_split_tasks_status_bar(frame, left_rows[1], app, focus, hints);

    // Right pane: diff view
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(right_area);
    if let Some(state) = diff_state_mut(&mut app.view_stack) {
        diff::render(
            frame,
            right_rows[0],
            state,
            focus == Pane::Right,
            &app.theme,
        );
    }
    render_diff_status_bar(frame, right_rows[1], app, focus == Pane::Right);
}

/// Split view: [diff | agent]
fn render_split_diff_agent(frame: &mut Frame, area: Rect, app: &mut App) {
    let focus = app.focus;
    let list_width = (area.width / 2).max(20);

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

    let divider_block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(divider_block, divider_area);

    // Left pane: diff
    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(left_area);
    if let Some(state) = diff_state_mut(&mut app.view_stack) {
        diff::render(frame, left_rows[0], state, focus == Pane::Left, &app.theme);
    }
    render_diff_status_bar(frame, left_rows[1], app, focus == Pane::Left);

    // Right pane: agent
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(right_area);
    let scroll_offset = agent::render(frame, right_rows[0], app);
    render_agent_status_bar(
        frame,
        right_rows[1],
        app,
        false,
        scroll_offset,
        focus == Pane::Right,
    );
}

/// Render the tasks list pane (list + dialog overlay).
fn render_tasks_pane(frame: &mut Frame, area: Rect, app: &mut App, focused: bool) {
    list::render(frame, area, app, focused);
    render_dialog_overlay(frame, area, app);
}

/// Render dialog overlay if present.
fn render_dialog_overlay(frame: &mut Frame, area: Rect, app: &App) {
    match &app.dialog {
        Some(Dialog::NewTask { input }) => render_new_task_dialog(frame, area, input),
        Some(Dialog::NewTaskUpstream {
            name,
            branches,
            selected,
        }) => {
            render_new_task_upstream_dialog(frame, area, name, branches, *selected);
        }
        Some(Dialog::ConfirmQuit) => render_confirm_quit_dialog(frame, area, app),
        Some(Dialog::ConfirmKill) => render_confirm_kill_dialog(frame, area, app),
        Some(Dialog::ConfirmDelete) => render_confirm_delete_dialog(frame, area, app),
        Some(Dialog::ConfirmSync) => render_confirm_sync_dialog(frame, area, app),
        Some(Dialog::ConfirmMerge) => render_confirm_merge_dialog(frame, area, app),
        Some(Dialog::ChangeUpstream { branches, selected }) => {
            render_change_upstream_dialog(frame, area, app, branches, *selected);
        }
        Some(Dialog::DiffSearch { .. }) | None => {}
    }
}

/// Status bar for the tasks pane in split views.
fn render_split_tasks_status_bar(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    focus: Pane,
    hints: &[(&str, &str)],
) {
    if render_error_bar(frame, area, app) {
        return;
    }
    let left = format_task_info(app);
    let t = &app.theme;
    let focused = focus == Pane::Left;
    let styles = StatusBarStyle {
        badge: if focused {
            t.title_focus_tasks
        } else {
            t.title_blur
        },
        text: if focused {
            t.title_text_focus
        } else {
            t.title_text_blur
        },
        hints: t.title_hints,
    };
    render_badge_status_bar(frame, area, " TASKS ", &styles, &left, hints);
}

/// Status bar for the tasks view.
fn render_tasks_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    if render_error_bar(frame, area, app) {
        return;
    }

    let left = format_task_info(app);

    let hints: &[(&str, &str)] = match &app.dialog {
        Some(Dialog::ConfirmKill) => &[("y", "kill"), ("n/Esc", "cancel")],
        Some(Dialog::ConfirmDelete) => &[("y", "delete"), ("n/Esc", "cancel")],
        Some(Dialog::ConfirmSync) => &[("y", "sync"), ("n/Esc", "cancel")],
        Some(Dialog::ConfirmMerge) => &[("f", "ff"), ("s", "squash"), ("Esc", "cancel")],
        Some(Dialog::ChangeUpstream { .. }) => {
            &[("j/k", "select"), ("Enter", "confirm"), ("Esc", "cancel")]
        }
        _ => &[
            ("n", "new"),
            ("Ctrl-k", "kill"),
            ("M", "merge"),
            ("S", "sync"),
            ("U", "upstream"),
            ("!", "delete"),
            ("R", "refresh"),
            ("Enter", "diff"),
            ("a", "agent"),
            ("C-a", "fresh"),
            ("q", "quit"),
        ],
    };

    let t = &app.theme;
    render_badge_status_bar(
        frame,
        area,
        " TASKS ",
        &StatusBarStyle {
            badge: t.title_focus_tasks,
            text: t.title_text_focus,
            hints: t.title_hints,
        },
        &left,
        hints,
    );
}

/// Status bar for the agent pane.
fn render_agent_status_bar(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    full: bool,
    scroll_offset: usize,
    focused: bool,
) {
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
        &[("Ctrl-b/f", "scroll"), ("C-o", "split"), ("C-q", "back")]
    } else {
        &[
            ("Ctrl-b/f", "scroll"),
            ("C-w", "left"),
            ("C-o", "full"),
            ("C-q", "back"),
        ]
    };

    let t = &app.theme;
    let styles = StatusBarStyle {
        badge: if focused {
            t.title_focus_agent
        } else {
            t.title_blur
        },
        text: if focused {
            t.title_text_focus
        } else {
            t.title_text_blur
        },
        hints: t.title_hints,
    };
    render_badge_status_bar(frame, area, " AGENT ", &styles, &location, hints);
}

/// Create a centered dialog with border, clearing the background.
/// Returns the inner Rect for content, or None if too small.
fn create_centered_dialog(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    width: u16,
    height: u16,
    border_color: Color,
) -> Option<Rect> {
    use ratatui::widgets::Clear;

    let dialog_width = width.min(area.width.saturating_sub(4));
    let dialog_height = height.min(area.height);
    if dialog_width == 0 || dialog_height == 0 {
        return None;
    }
    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);
    Some(inner)
}

fn render_new_task_dialog(frame: &mut Frame, area: Rect, input: &str) {
    use ratatui::layout::Alignment;

    let Some(inner) = create_centered_dialog(frame, area, " New Task ", 50, 5, Color::Yellow)
    else {
        return;
    };

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

fn render_new_task_upstream_dialog(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    branches: &[String],
    selected: usize,
) {
    let visible_branches = branches.len().min(10) as u16;
    let Some(inner) = create_centered_dialog(
        frame,
        area,
        " New Task ",
        50,
        4 + visible_branches,
        Color::Yellow,
    ) else {
        return;
    };

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

    for (i, branch) in branches
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(max_visible)
    {
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

    let running_count = app.tasks.iter().filter(|t| t.is_running()).count();

    let Some(inner) = create_centered_dialog(frame, area, " Quit copse? ", 52, 6, Color::Red)
    else {
        return;
    };

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

    let name = app.selected_task().map(|t| t.name.as_str()).unwrap_or("?");

    let Some(inner) = create_centered_dialog(frame, area, " Kill task? ", 52, 6, Color::Red) else {
        return;
    };

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

    let name = app.selected_task().map(|t| t.name.as_str()).unwrap_or("?");

    let Some(inner) = create_centered_dialog(frame, area, " Delete task? ", 52, 6, Color::Red)
    else {
        return;
    };

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

    let (name, upstream) = app
        .selected_task()
        .map(|t| (t.name.as_str(), t.upstream.as_str()))
        .unwrap_or(("?", "?"));

    let Some(inner) =
        create_centered_dialog(frame, area, " Sync from upstream? ", 52, 6, Color::Red)
    else {
        return;
    };

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

    let (name, upstream) = app
        .selected_task()
        .map(|t| (t.name.as_str(), t.upstream.as_str()))
        .unwrap_or(("?", "?"));

    let Some(inner) =
        create_centered_dialog(frame, area, " Merge to upstream ", 52, 6, Color::Yellow)
    else {
        return;
    };

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

fn render_change_upstream_dialog(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    branches: &[String],
    selected: usize,
) {
    let name = app.selected_task().map(|t| t.name.as_str()).unwrap_or("?");

    let visible_branches = branches.len().min(10) as u16;
    let Some(inner) = create_centered_dialog(
        frame,
        area,
        " Change Upstream ",
        50,
        4 + visible_branches,
        Color::Yellow,
    ) else {
        return;
    };

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

    for (i, branch) in branches
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(max_visible)
    {
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

/// Render error message in the status bar area. Returns true if rendered.
fn render_error_bar(frame: &mut Frame, area: Rect, app: &App) -> bool {
    if let Some(err) = &app.last_error {
        let width = area.width as usize;
        let text = format!(" {err}");
        let padding = " ".repeat(width.saturating_sub(text.len()));
        let bar = Paragraph::new(Line::from(Span::styled(
            format!("{text}{padding}"),
            Style::default().fg(Color::White).bg(Color::Red),
        )));
        frame.render_widget(bar, area);
        true
    } else {
        false
    }
}

/// Format selected task info for status bar display.
fn format_task_info(app: &App) -> String {
    if let Some(task) = app.selected_task() {
        let st = match task.status {
            crate::task::TaskStatus::Running => {
                if task.waiting_for_input {
                    "waiting"
                } else {
                    "running"
                }
            }
            crate::task::TaskStatus::Stopped => "stopped",
        };
        format!(" {} - {}", task.name, st)
    } else {
        String::new()
    }
}

struct StatusBarStyle {
    badge: Style,
    text: Style,
    hints: Style,
}

/// Generic status bar with badge, location text, and key hints.
fn render_badge_status_bar(
    frame: &mut Frame,
    area: Rect,
    badge: &str,
    styles: &StatusBarStyle,
    location: &str,
    hints: &[(&str, &str)],
) {
    use ratatui::text::Text;

    let right_str = {
        let s = hints
            .iter()
            .map(|(k, d)| format!("{k}:{d}"))
            .collect::<Vec<_>>()
            .join("  ");
        format!("{s} ")
    };

    let badge_len = badge.len();
    let location_len = location.len();
    let right_len = right_str.len();
    let width = area.width as usize;
    let gap = width.saturating_sub(badge_len + location_len + right_len);

    let line = Line::from(vec![
        Span::styled(badge, styles.badge),
        Span::styled(format!("{location}{}", " ".repeat(gap)), styles.text),
        Span::styled(right_str, styles.hints),
    ]);

    frame.render_widget(Paragraph::new(Text::from(line)), area);
}

/// Status bar for the diff view.
fn render_diff_status_bar(frame: &mut Frame, area: Rect, app: &App, focused: bool) {
    if render_error_bar(frame, area, app) {
        return;
    }

    // DiffSearch dialog: render search input inline in status bar (tig-style)
    if let Some(Dialog::DiffSearch { input, .. }) = &app.dialog {
        use ratatui::text::Text;
        let width = area.width as usize;
        let search_text = format!("/{input}");
        let cursor_char = "█";
        let padding = " ".repeat(width.saturating_sub(search_text.len() + cursor_char.len()));
        let bg = Color::Indexed(234);
        let line = Line::from(vec![
            Span::styled(search_text, Style::default().fg(Color::White).bg(bg)),
            Span::styled(cursor_char, Style::default().fg(Color::Yellow).bg(bg)),
            Span::styled(padding, Style::default().bg(bg)),
        ]);
        frame.render_widget(Paragraph::new(Text::from(line)), area);
        return;
    }

    let location = if let Some(state) = app.diff_state() {
        format!(" {}", state.task_name)
    } else {
        String::new()
    };

    let is_full = app.fullscreen == Some(View::Diff);
    let in_split_with_agent = app.has_view(View::Agent);
    let hints: &[(&str, &str)] = if in_split_with_agent {
        &[
            ("j/k", "move"),
            ("/", "search"),
            ("@", "hunk"),
            ("R", "refresh"),
            ("O", "full"),
            ("C-w", "agent"),
            ("q", "back"),
        ]
    } else if is_full {
        &[
            ("j/k", "move"),
            ("/", "search"),
            ("n/N", "match"),
            ("@", "hunk"),
            ("R", "refresh"),
            ("O", "split"),
            ("q", "back"),
        ]
    } else {
        &[
            ("j/k", "move"),
            ("/", "search"),
            ("n/N", "match"),
            ("@", "hunk"),
            ("R", "refresh"),
            ("q", "back"),
        ]
    };

    let t = &app.theme;
    let styles = StatusBarStyle {
        badge: if focused {
            t.title_focus_diff
        } else {
            t.title_blur
        },
        text: if focused {
            t.title_text_focus
        } else {
            t.title_text_blur
        },
        hints: t.title_hints,
    };
    render_badge_status_bar(frame, area, " DIFF ", &styles, &location, hints);
}
