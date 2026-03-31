use ratatui::style::{Color, Modifier, Style};

use crate::config::{ColorConfig, ColorEntry};

/// Color theme for the TUI, using tig-style naming conventions.
pub struct Theme {
    // General UI
    pub cursor: Style,
    pub cursor_blur: Style,
    pub title_focus_tasks: Style,
    pub title_focus_agent: Style,
    pub title_focus_diff: Style,
    pub title_blur: Style,
    pub title_text_focus: Style,
    pub title_text_blur: Style,
    pub title_hints: Style,
    pub search_result: Style,

    // Diff colors
    pub diff_add: Style,
    pub diff_del: Style,
    pub diff_chunk: Style,
    pub diff_header: Style,
    pub diff_context: Style,

    // List (task list)
    pub list_highlight: Style,
    pub list_highlight_blur: Style,
    pub list_header: Style,
}

impl Theme {
    /// Build a Theme from ColorConfig, collecting warnings for invalid values.
    pub fn from_color_config(cc: &ColorConfig) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut convert =
            |key: &str, entry: &ColorEntry| -> Style { convert_entry(key, entry, &mut warnings) };
        let theme = Self {
            cursor: convert("cursor", &cc.cursor),
            cursor_blur: convert("cursor-blur", &cc.cursor_blur),
            title_focus_tasks: convert("title-focus-tasks", &cc.title_focus_tasks),
            title_focus_agent: convert("title-focus-agent", &cc.title_focus_agent),
            title_focus_diff: convert("title-focus-diff", &cc.title_focus_diff),
            title_blur: convert("title-blur", &cc.title_blur),
            title_text_focus: convert("title-text-focus", &cc.title_text_focus),
            title_text_blur: convert("title-text-blur", &cc.title_text_blur),
            title_hints: convert("title-hints", &cc.title_hints),
            search_result: convert("search-result", &cc.search_result),
            diff_add: convert("diff-add", &cc.diff_add),
            diff_del: convert("diff-del", &cc.diff_del),
            diff_chunk: convert("diff-chunk", &cc.diff_chunk),
            diff_header: convert("diff-header", &cc.diff_header),
            diff_context: convert("diff-context", &cc.diff_context),
            list_highlight: convert("list-highlight", &cc.list_highlight),
            list_highlight_blur: convert("list-highlight-blur", &cc.list_highlight_blur),
            list_header: convert("list-header", &cc.list_header),
        };
        (theme, warnings)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_color_config(&ColorConfig::default()).0
    }
}

fn parse_color(s: &str) -> Option<Color> {
    match s {
        "default" => Some(Color::Reset),
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        other => {
            if let Ok(n) = other.parse::<u8>() {
                Some(Color::Indexed(n))
            } else {
                None
            }
        }
    }
}

fn parse_modifier(s: &str) -> Option<Modifier> {
    match s {
        "bold" => Some(Modifier::BOLD),
        "dim" => Some(Modifier::DIM),
        "underline" | "underlined" => Some(Modifier::UNDERLINED),
        "reverse" | "reversed" => Some(Modifier::REVERSED),
        "italic" => Some(Modifier::ITALIC),
        _ => None,
    }
}

/// Convert a ColorEntry to Style, pushing warnings for invalid values.
fn convert_entry(key: &str, entry: &ColorEntry, warnings: &mut Vec<String>) -> Style {
    let mut s = Style::default();
    if let Some(ref fg) = entry.fg {
        if let Some(c) = parse_color(fg) {
            s = s.fg(c);
        } else {
            warnings.push(format!("[color] {key}: unknown fg color '{fg}'"));
        }
    }
    if let Some(ref bg) = entry.bg {
        if let Some(c) = parse_color(bg) {
            s = s.bg(c);
        } else {
            warnings.push(format!("[color] {key}: unknown bg color '{bg}'"));
        }
    }
    for attr in &entry.attrs {
        if let Some(m) = parse_modifier(attr) {
            s = s.add_modifier(m);
        } else {
            warnings.push(format!("[color] {key}: unknown attr '{attr}'"));
        }
    }
    s
}
