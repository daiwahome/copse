use std::path::PathBuf;

use etcetera::BaseStrategy;
use serde::Deserialize;

use crate::keybind::RawKeyBindings;

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    #[default]
    ClaudeCode,
}

impl std::fmt::Display for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Agent::ClaudeCode => write!(f, "claudecode"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClaudeCodeConfig {
    #[serde(default = "default_permission_mode")]
    pub permission_mode: String,
}

impl Default for ClaudeCodeConfig {
    fn default() -> Self {
        Self {
            permission_mode: default_permission_mode(),
        }
    }
}

fn default_permission_mode() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    BuiltIn,
    Tmux,
}

impl std::fmt::Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Backend::BuiltIn => write!(f, "builtin"),
            Backend::Tmux => write!(f, "tmux"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffFilter {
    #[default]
    None,
    Delta,
}

impl std::fmt::Display for DiffFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffFilter::None => write!(f, "none"),
            DiffFilter::Delta => write!(f, "delta"),
        }
    }
}

fn config_path() -> anyhow::Result<PathBuf> {
    let strategy = etcetera::base_strategy::Xdg::new()
        .map_err(|e| anyhow::anyhow!("Failed to determine config directory: {e}"))?;
    Ok(strategy.config_dir().join("copse").join("config.toml"))
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub agent: Agent,
    #[serde(default)]
    pub backend: Backend,
    #[serde(default)]
    pub diff_filter: DiffFilter,
    #[serde(default)]
    pub auto_commit: bool,
    #[serde(default)]
    pub auto_permissions: bool,
    #[serde(default, rename = "claudecode")]
    pub claude_code: ClaudeCodeConfig,
    #[serde(default)]
    pub color: ColorConfig,
    #[serde(default)]
    pub keys: RawKeyBindings,
    #[serde(default)]
    pub notification_command: Option<String>,
}

impl Config {
    pub fn validate_notification_command(&self) -> anyhow::Result<()> {
        if let Some(ref cmd) = self.notification_command {
            if cmd.is_empty() {
                anyhow::bail!(
                    "notification_command must not be empty; remove the key to disable it"
                );
            }
        }
        Ok(())
    }

    pub fn load() -> anyhow::Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read config from {}: {e}", path.display()))?;
        let cfg = toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse config from {}: {e}", path.display()))?;
        Ok(cfg)
    }

    /// Write the default config file. Returns an error if the file already exists.
    pub fn init() -> anyhow::Result<()> {
        let path = config_path()?;

        if path.exists() {
            anyhow::bail!("Config file already exists: {}", path.display());
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("Failed to create config directory: {e}"))?;
        }

        let content = Self::default().to_toml();
        std::fs::write(&path, content)
            .map_err(|e| anyhow::anyhow!("Failed to write config: {e}"))?;

        println!("Created {}", path.display());
        Ok(())
    }

    /// Serialize config to a human-friendly TOML string with inline color tables.
    fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("agent = \"{}\"\n", self.agent));
        out.push_str(&format!("backend = \"{}\"\n", self.backend));
        out.push_str(&format!("diff_filter = \"{}\"\n", self.diff_filter));
        out.push_str(&format!("auto_commit = {}\n", self.auto_commit));
        out.push_str(&format!("auto_permissions = {}\n", self.auto_permissions));
        out.push_str("# notification_command = \"osascript -e 'display notification \\\"Needs input\\\" with title \\\"Copse\\\"'\"\n");
        out.push_str("\n[claudecode]\n");
        out.push_str(&format!(
            "permission_mode = \"{}\"\n",
            self.claude_code.permission_mode
        ));

        out.push_str("\n[color]\n");

        let entries: &[(&str, &ColorEntry)] = &[
            ("cursor", &self.color.cursor),
            ("cursor-blur", &self.color.cursor_blur),
            ("title-focus-tasks", &self.color.title_focus_tasks),
            ("title-focus-agent", &self.color.title_focus_agent),
            ("title-focus-diff", &self.color.title_focus_diff),
            ("title-blur", &self.color.title_blur),
            ("title-text-focus", &self.color.title_text_focus),
            ("title-text-blur", &self.color.title_text_blur),
            ("title-hints", &self.color.title_hints),
            ("search-result", &self.color.search_result),
            ("diff-add", &self.color.diff_add),
            ("diff-del", &self.color.diff_del),
            ("diff-chunk", &self.color.diff_chunk),
            ("diff-header", &self.color.diff_header),
            ("diff-context", &self.color.diff_context),
            ("list-highlight", &self.color.list_highlight),
            ("list-highlight-blur", &self.color.list_highlight_blur),
        ];

        for (key, entry) in entries {
            out.push_str(&format!("{key} = {}\n", entry.to_inline_toml()));
        }

        out.push_str(&crate::keybind::default_keys_toml());

        out
    }
}

/// A single color entry in the config: foreground, background, and attributes.
#[derive(Debug, Clone, Deserialize)]
pub struct ColorEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<String>,
}

impl ColorEntry {
    fn new() -> Self {
        Self {
            fg: None,
            bg: None,
            attrs: vec![],
        }
    }

    fn fg(mut self, color: &str) -> Self {
        self.fg = Some(color.to_string());
        self
    }

    fn bg(mut self, color: &str) -> Self {
        self.bg = Some(color.to_string());
        self
    }

    fn bold(mut self) -> Self {
        self.attrs.push("bold".to_string());
        self
    }

    /// Format as a TOML inline table, e.g. `{ fg = "black", bg = "166" }`
    fn to_inline_toml(&self) -> String {
        let mut parts = Vec::new();
        if let Some(ref fg) = self.fg {
            parts.push(format!("fg = \"{fg}\""));
        }
        if let Some(ref bg) = self.bg {
            parts.push(format!("bg = \"{bg}\""));
        }
        format!("{{ {} }}", parts.join(", "))
    }
}

/// Color configuration matching the Theme struct fields, using kebab-case TOML keys.
#[derive(Debug, Clone, Deserialize)]
pub struct ColorConfig {
    #[serde(default = "default_cursor")]
    pub cursor: ColorEntry,
    #[serde(default = "default_cursor_blur", rename = "cursor-blur")]
    pub cursor_blur: ColorEntry,
    #[serde(default = "default_title_focus_tasks", rename = "title-focus-tasks")]
    pub title_focus_tasks: ColorEntry,
    #[serde(default = "default_title_focus_agent", rename = "title-focus-agent")]
    pub title_focus_agent: ColorEntry,
    #[serde(default = "default_title_focus_diff", rename = "title-focus-diff")]
    pub title_focus_diff: ColorEntry,
    #[serde(default = "default_title_blur", rename = "title-blur")]
    pub title_blur: ColorEntry,
    #[serde(default = "default_title_text_focus", rename = "title-text-focus")]
    pub title_text_focus: ColorEntry,
    #[serde(default = "default_title_text_blur", rename = "title-text-blur")]
    pub title_text_blur: ColorEntry,
    #[serde(default = "default_title_hints", rename = "title-hints")]
    pub title_hints: ColorEntry,
    #[serde(default = "default_search_result", rename = "search-result")]
    pub search_result: ColorEntry,
    #[serde(default = "default_diff_add", rename = "diff-add")]
    pub diff_add: ColorEntry,
    #[serde(default = "default_diff_del", rename = "diff-del")]
    pub diff_del: ColorEntry,
    #[serde(default = "default_diff_chunk", rename = "diff-chunk")]
    pub diff_chunk: ColorEntry,
    #[serde(default = "default_diff_header", rename = "diff-header")]
    pub diff_header: ColorEntry,
    #[serde(default = "default_diff_context", rename = "diff-context")]
    pub diff_context: ColorEntry,
    #[serde(default = "default_list_highlight", rename = "list-highlight")]
    pub list_highlight: ColorEntry,
    #[serde(
        default = "default_list_highlight_blur",
        rename = "list-highlight-blur"
    )]
    pub list_highlight_blur: ColorEntry,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            cursor: default_cursor(),
            cursor_blur: default_cursor_blur(),
            title_focus_tasks: default_title_focus_tasks(),
            title_focus_agent: default_title_focus_agent(),
            title_focus_diff: default_title_focus_diff(),
            title_blur: default_title_blur(),
            title_text_focus: default_title_text_focus(),
            title_text_blur: default_title_text_blur(),
            title_hints: default_title_hints(),
            search_result: default_search_result(),
            diff_add: default_diff_add(),
            diff_del: default_diff_del(),
            diff_chunk: default_diff_chunk(),
            diff_header: default_diff_header(),
            diff_context: default_diff_context(),
            list_highlight: default_list_highlight(),
            list_highlight_blur: default_list_highlight_blur(),
        }
    }
}

// Default value functions for serde
fn default_cursor() -> ColorEntry {
    ColorEntry::new().bg("236")
}
fn default_cursor_blur() -> ColorEntry {
    ColorEntry::new().fg("252").bg("234")
}
fn default_title_focus_tasks() -> ColorEntry {
    ColorEntry::new().fg("black").bg("166").bold()
}
fn default_title_focus_agent() -> ColorEntry {
    ColorEntry::new().fg("black").bg("217").bold()
}
fn default_title_focus_diff() -> ColorEntry {
    ColorEntry::new().fg("black").bg("33").bold()
}
fn default_title_blur() -> ColorEntry {
    ColorEntry::new().fg("black").bg("240").bold()
}
fn default_title_text_focus() -> ColorEntry {
    ColorEntry::new().fg("252").bg("234")
}
fn default_title_text_blur() -> ColorEntry {
    ColorEntry::new().fg("245").bg("234")
}
fn default_title_hints() -> ColorEntry {
    ColorEntry::new().fg("245").bg("234")
}
fn default_search_result() -> ColorEntry {
    ColorEntry::new().bg("238")
}
fn default_diff_add() -> ColorEntry {
    ColorEntry::new().fg("green")
}
fn default_diff_del() -> ColorEntry {
    ColorEntry::new().fg("red")
}
fn default_diff_chunk() -> ColorEntry {
    ColorEntry::new().fg("cyan")
}
fn default_diff_header() -> ColorEntry {
    ColorEntry::new().fg("white").bold()
}
fn default_diff_context() -> ColorEntry {
    ColorEntry::new().fg("white")
}
fn default_list_highlight() -> ColorEntry {
    ColorEntry::new().fg("166").bg("234").bold()
}
fn default_list_highlight_blur() -> ColorEntry {
    ColorEntry::new().fg("252").bg("234")
}
