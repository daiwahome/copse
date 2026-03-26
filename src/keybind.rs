use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Action enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TasksAction {
    NewTask,
    MoveDown,
    MoveUp,
    OpenTask,
    ShowDiff,
    Merge,
    Sync,
    ChangeUpstream,
    Delete,
    Refresh,
    Fullscreen,
    Quit,
    Kill,
    CloseChildren,
    StartFresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffAction {
    MoveDown,
    MoveUp,
    NextHunk,
    Search,
    SearchNext,
    SearchPrev,
    Refresh,
    Fullscreen,
    Close,
    PageUp,
    PageDown,
    AddComment,
    EditComment,
    DeleteComment,
    SendReview,
    NextComment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentAction {
    Fullscreen,
    Close,
    PageUp,
    PageDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlobalAction {
    FocusToggle,
}

// ---------------------------------------------------------------------------
// KeyBind – normalised key representation for HashMap lookup
// ---------------------------------------------------------------------------

/// Normalised key representation used as a HashMap key.
///
/// NOTE: This relies on `KeyCode` and `KeyModifiers` implementing `Hash` and
/// `Eq` via derive. If crossterm changes those implementations, key lookups
/// may break silently — verify after crossterm upgrades.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyBind {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBind {
    /// Normalise a crossterm `KeyEvent` into a `KeyBind`.
    ///
    /// crossterm delivers uppercase letters as `Char('O')` with `SHIFT`
    /// modifier. We strip `SHIFT` for plain uppercase so that the user
    /// can write `"O"` in config rather than `"Shift-O"`.
    pub fn from_event(event: &KeyEvent) -> Self {
        let mut mods =
            event.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        let code = event.code;

        // For uppercase letters without CONTROL, remove the SHIFT flag
        // because we treat `Char('O')` as the canonical form for Shift+o.
        if let KeyCode::Char(c) = code {
            if c.is_ascii_uppercase() && !mods.contains(KeyModifiers::CONTROL) {
                mods -= KeyModifiers::SHIFT;
            }
        }

        Self {
            code,
            modifiers: mods,
        }
    }

    /// Format a `KeyBind` back into a human-readable string (inverse of `parse`).
    pub fn display(&self) -> String {
        let base = match self.code {
            KeyCode::Char(c) => {
                if self.modifiers.contains(KeyModifiers::CONTROL) {
                    return format!("Ctrl-{c}");
                }
                if c == ' ' {
                    return "Space".to_string();
                }
                c.to_string()
            }
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Up => "Up".to_string(),
            KeyCode::Down => "Down".to_string(),
            KeyCode::Left => "Left".to_string(),
            KeyCode::Right => "Right".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::F(n) => format!("F{n}"),
            _ => "<unknown>".to_string(),
        };
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            format!("Shift-{base}")
        } else {
            base
        }
    }

    /// Parse a human-readable key string into a `KeyBind`.
    ///
    /// Accepted formats:
    /// - Single character: `"a"`, `"A"`, `"!"`, `"@"`, `"/"`
    /// - Named keys: `"Enter"`, `"Esc"`, `"Up"`, `"Down"`, `"Left"`, `"Right"`,
    ///   `"Tab"`, `"Backspace"`, `"Delete"`, `"Space"`
    /// - Function keys: `"F1"` .. `"F12"`
    /// - Ctrl combos: `"Ctrl-O"`, `"Ctrl-W"`
    pub fn parse(s: &str) -> Result<Self, String> {
        if let Some(rest) = s.strip_prefix("Ctrl-") {
            if rest.len() == 1 {
                let c = rest.chars().next().unwrap().to_ascii_lowercase();
                return Ok(Self {
                    code: KeyCode::Char(c),
                    modifiers: KeyModifiers::CONTROL,
                });
            }
            return Err(format!("Invalid Ctrl combo: {s}"));
        }

        if let Some(rest) = s.strip_prefix("Shift-") {
            let inner = Self::parse(rest)?;
            return Ok(Self {
                code: inner.code,
                modifiers: inner.modifiers | KeyModifiers::SHIFT,
            });
        }

        // Named keys (case-sensitive)
        let (code, mods) = match s {
            "Enter" => (KeyCode::Enter, KeyModifiers::NONE),
            "Esc" => (KeyCode::Esc, KeyModifiers::NONE),
            "Up" => (KeyCode::Up, KeyModifiers::NONE),
            "Down" => (KeyCode::Down, KeyModifiers::NONE),
            "Left" => (KeyCode::Left, KeyModifiers::NONE),
            "Right" => (KeyCode::Right, KeyModifiers::NONE),
            "Tab" => (KeyCode::Tab, KeyModifiers::NONE),
            "Backspace" => (KeyCode::Backspace, KeyModifiers::NONE),
            "Delete" => (KeyCode::Delete, KeyModifiers::NONE),
            "Space" => (KeyCode::Char(' '), KeyModifiers::NONE),
            "PageUp" => (KeyCode::PageUp, KeyModifiers::NONE),
            "PageDown" => (KeyCode::PageDown, KeyModifiers::NONE),
            "Home" => (KeyCode::Home, KeyModifiers::NONE),
            "End" => (KeyCode::End, KeyModifiers::NONE),
            _ => {
                // F-keys
                if let Some(n) = s.strip_prefix('F') {
                    if let Ok(num) = n.parse::<u8>() {
                        if (1..=12).contains(&num) {
                            return Ok(Self {
                                code: KeyCode::F(num),
                                modifiers: KeyModifiers::NONE,
                            });
                        }
                    }
                    return Err(format!("Invalid function key: {s}"));
                }

                // Single character
                let mut chars = s.chars();
                let c = chars.next().ok_or_else(|| "Empty key string".to_string())?;
                if chars.next().is_some() {
                    return Err(format!("Unknown key name: {s}"));
                }
                (KeyCode::Char(c), KeyModifiers::NONE)
            }
        };

        Ok(Self {
            code,
            modifiers: mods,
        })
    }
}

// ---------------------------------------------------------------------------
// ViewBindings<A> – key → action map for one view
// ---------------------------------------------------------------------------

pub struct ViewBindings<A> {
    map: HashMap<KeyBind, A>,
}

impl<A: Copy> ViewBindings<A> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, kb: KeyBind, action: A) {
        self.map.insert(kb, action);
    }

    pub fn remove(&mut self, kb: &KeyBind) {
        self.map.remove(kb);
    }

    pub fn lookup(&self, key: &KeyEvent) -> Option<A> {
        let kb = KeyBind::from_event(key);
        self.map.get(&kb).copied()
    }
}

// ---------------------------------------------------------------------------
// KeyBindings – complete set of bindings for the application
// ---------------------------------------------------------------------------

pub struct KeyBindings {
    pub global: ViewBindings<GlobalAction>,
    pub tasks: ViewBindings<TasksAction>,
    pub diff: ViewBindings<DiffAction>,
    pub agent: ViewBindings<AgentAction>,
}

/// Action descriptor: name + default keys + display name. Used for defaults, `--init`, and help dialog.
struct ActionDef<A: Copy> {
    name: &'static str,
    action: A,
    default_keys: &'static [&'static str],
    display_name: &'static str,
}

// Macro to reduce boilerplate for action definition lists
macro_rules! action_defs {
    ($( $name:expr, $display:expr => $action:expr, [ $($key:expr),* $(,)? ] );* $(;)?) => {
        &[ $( ActionDef { name: $name, action: $action, default_keys: &[$($key),*], display_name: $display } ),* ]
    };
}

fn tasks_action_defs() -> &'static [ActionDef<TasksAction>] {
    action_defs! {
        "new-task", "Create new task" => TasksAction::NewTask, ["n"];
        "move-down", "Move down" => TasksAction::MoveDown, ["j", "Down"];
        "move-up", "Move up" => TasksAction::MoveUp, ["k", "Up"];
        "open", "Open agent view" => TasksAction::OpenTask, ["a"];
        "show-diff", "Open diff view" => TasksAction::ShowDiff, ["d", "Enter"];
        "merge", "Merge to upstream" => TasksAction::Merge, ["M"];
        "sync", "Sync from upstream" => TasksAction::Sync, ["S"];
        "change-upstream", "Change upstream" => TasksAction::ChangeUpstream, ["U"];
        "delete", "Delete task" => TasksAction::Delete, ["!"];
        "refresh", "Refresh status" => TasksAction::Refresh, ["R"];
        "fullscreen", "Toggle fullscreen" => TasksAction::Fullscreen, ["O", "Ctrl-O"];
        "quit", "Quit / Back" => TasksAction::Quit, ["q", "Q"];
        "kill", "Kill running task" => TasksAction::Kill, ["Ctrl-K"];
        "close-children", "Close child views" => TasksAction::CloseChildren, ["Ctrl-Q"];
        "start-fresh", "Start fresh agent" => TasksAction::StartFresh, ["Ctrl-A"];
    }
}

fn diff_action_defs() -> &'static [ActionDef<DiffAction>] {
    action_defs! {
        "move-down", "Move down" => DiffAction::MoveDown, ["j", "Down"];
        "move-up", "Move up" => DiffAction::MoveUp, ["k", "Up"];
        "next-hunk", "Jump to next hunk" => DiffAction::NextHunk, ["@"];
        "search", "Search" => DiffAction::Search, ["/"];
        "search-next", "Next match" => DiffAction::SearchNext, ["n"];
        "search-prev", "Previous match" => DiffAction::SearchPrev, ["N"];
        "refresh", "Refresh diff" => DiffAction::Refresh, ["R"];
        "fullscreen", "Toggle fullscreen" => DiffAction::Fullscreen, ["O", "Ctrl-O"];
        "close", "Close diff view" => DiffAction::Close, ["q", "Esc", "Ctrl-Q"];
        "page-up", "Page up" => DiffAction::PageUp, ["Ctrl-B"];
        "page-down", "Page down" => DiffAction::PageDown, ["Ctrl-F"];
        "add-comment", "Add comment" => DiffAction::AddComment, ["o"];
        "edit-comment", "Edit comment" => DiffAction::EditComment, ["e"];
        "delete-comment", "Delete comment" => DiffAction::DeleteComment, ["!"];
        "send-review", "Send review" => DiffAction::SendReview, ["S"];
        "next-comment", "Jump to next comment" => DiffAction::NextComment, ["c"];
    }
}

fn agent_action_defs() -> &'static [ActionDef<AgentAction>] {
    action_defs! {
        "fullscreen", "Toggle fullscreen" => AgentAction::Fullscreen, ["Ctrl-O"];
        "close", "Close agent view" => AgentAction::Close, ["Ctrl-Q"];
        "page-up", "Scroll up" => AgentAction::PageUp, ["Ctrl-B"];
        "page-down", "Scroll down" => AgentAction::PageDown, ["Ctrl-F"];
    }
}

fn global_action_defs() -> &'static [ActionDef<GlobalAction>] {
    action_defs! {
        "focus-toggle", "Toggle focus" => GlobalAction::FocusToggle, ["Ctrl-W"];
    }
}

/// Build a `ViewBindings` from action definitions using their default keys.
fn build_defaults<A: Copy>(defs: &[ActionDef<A>]) -> ViewBindings<A> {
    let mut bindings = ViewBindings::new();
    for def in defs {
        for key_str in def.default_keys {
            if let Ok(kb) = KeyBind::parse(key_str) {
                bindings.insert(kb, def.action);
            }
        }
    }
    bindings
}

/// Apply user overrides onto default bindings for one view.
/// Returns warnings for unknown action names or invalid key strings.
fn apply_overrides<A: Copy>(
    defaults: &mut ViewBindings<A>,
    defs: &[ActionDef<A>],
    raw: &HashMap<String, Vec<String>>,
    view_name: &str,
    warnings: &mut Vec<String>,
) {
    // Build action name → (action, default_keys) lookup
    let action_map: HashMap<&str, &ActionDef<A>> = defs.iter().map(|d| (d.name, d)).collect();

    // Track which key is bound to which user-specified action name, for duplicate detection
    let mut user_key_owner: HashMap<KeyBind, String> = HashMap::new();

    for (action_name, key_strings) in raw {
        if let Some(def) = action_map.get(action_name.as_str()) {
            // Remove old default keys for this action
            for old_key_str in def.default_keys {
                if let Ok(kb) = KeyBind::parse(old_key_str) {
                    defaults.remove(&kb);
                }
            }
            // Insert new keys
            for key_str in key_strings {
                match KeyBind::parse(key_str) {
                    Ok(kb) => {
                        if let Some(prev_action) = user_key_owner.get(&kb) {
                            warnings.push(format!(
                                "[keys.{view_name}] Duplicate key \"{key_str}\": bound to both {prev_action} and {action_name}"
                            ));
                        }
                        user_key_owner.insert(kb.clone(), action_name.clone());
                        defaults.insert(kb, def.action);
                    }
                    Err(e) => {
                        warnings.push(format!("[keys.{view_name}] {action_name}: {e}"));
                    }
                }
            }
        } else {
            warnings.push(format!("[keys.{view_name}] Unknown action: {action_name}"));
        }
    }
}

impl KeyBindings {
    pub fn defaults() -> Self {
        Self {
            global: build_defaults(global_action_defs()),
            tasks: build_defaults(tasks_action_defs()),
            diff: build_defaults(diff_action_defs()),
            agent: build_defaults(agent_action_defs()),
        }
    }

    pub fn with_overrides(raw: &RawKeyBindings) -> (Self, Vec<String>) {
        let mut bindings = Self::defaults();
        let mut warnings = Vec::new();

        if let Some(ref global_raw) = raw.global {
            apply_overrides(
                &mut bindings.global,
                global_action_defs(),
                global_raw,
                "global",
                &mut warnings,
            );
        }
        if let Some(ref tasks_raw) = raw.tasks {
            apply_overrides(
                &mut bindings.tasks,
                tasks_action_defs(),
                tasks_raw,
                "tasks",
                &mut warnings,
            );
        }
        if let Some(ref diff_raw) = raw.diff {
            apply_overrides(
                &mut bindings.diff,
                diff_action_defs(),
                diff_raw,
                "diff",
                &mut warnings,
            );
        }
        if let Some(ref agent_raw) = raw.agent {
            apply_overrides(
                &mut bindings.agent,
                agent_action_defs(),
                agent_raw,
                "agent",
                &mut warnings,
            );
        }

        (bindings, warnings)
    }
}

// ---------------------------------------------------------------------------
// TOML output helpers (for --init)
// ---------------------------------------------------------------------------

fn action_defs_to_toml<A: Copy>(defs: &[ActionDef<A>]) -> String {
    let mut out = String::new();
    for def in defs {
        let keys: Vec<String> = def
            .default_keys
            .iter()
            .map(|k| format!("\"{k}\""))
            .collect();
        out.push_str(&format!("{} = [{}]\n", def.name, keys.join(", ")));
    }
    out
}

pub fn default_keys_toml() -> String {
    let mut out = String::new();

    out.push_str("\n[keys.global]\n");
    out.push_str(&action_defs_to_toml(global_action_defs()));

    out.push_str("\n[keys.tasks]\n");
    out.push_str(&action_defs_to_toml(tasks_action_defs()));

    out.push_str("\n[keys.diff]\n");
    out.push_str(&action_defs_to_toml(diff_action_defs()));

    out.push_str("\n[keys.agent]\n");
    out.push_str(&action_defs_to_toml(agent_action_defs()));

    out
}

// ---------------------------------------------------------------------------
// Help entries – derive dialog content from actual bindings
// ---------------------------------------------------------------------------

/// Build help entries for a view by reverse-looking-up keys from the bindings map.
/// Returns `(keys_display, display_name)` pairs in the order of `defs`.
fn build_help_entries<A: Copy + Eq + std::hash::Hash>(
    bindings: &ViewBindings<A>,
    defs: &[ActionDef<A>],
) -> Vec<(String, &'static str)> {
    defs.iter()
        .filter_map(|def| {
            let mut keys: Vec<String> = bindings
                .map
                .iter()
                .filter(|(_, a)| **a == def.action)
                .map(|(kb, _)| kb.display())
                .collect();
            if keys.is_empty() {
                return None;
            }
            keys.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
            Some((keys.join("/"), def.display_name))
        })
        .collect()
}

pub fn tasks_help_entries(bindings: &ViewBindings<TasksAction>) -> Vec<(String, &'static str)> {
    build_help_entries(bindings, tasks_action_defs())
}

pub fn agent_help_entries(bindings: &ViewBindings<AgentAction>) -> Vec<(String, &'static str)> {
    build_help_entries(bindings, agent_action_defs())
}

pub fn diff_help_entries(bindings: &ViewBindings<DiffAction>) -> Vec<(String, &'static str)> {
    build_help_entries(bindings, diff_action_defs())
}

// ---------------------------------------------------------------------------
// RawKeyBindings – serde intermediate
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct RawKeyBindings {
    pub global: Option<HashMap<String, Vec<String>>>,
    pub tasks: Option<HashMap<String, Vec<String>>>,
    pub diff: Option<HashMap<String, Vec<String>>>,
    pub agent: Option<HashMap<String, Vec<String>>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The TOML produced by `--init` must round-trip through serde.
    #[test]
    fn default_keys_toml_round_trips() {
        let toml_str = default_keys_toml();

        #[derive(Deserialize)]
        struct Wrapper {
            keys: RawKeyBindings,
        }

        let wrapper: Wrapper = toml::from_str(&toml_str)
            .unwrap_or_else(|e| panic!("Failed to parse default keys TOML: {e}\n---\n{toml_str}"));

        // Every view section should be present
        assert!(wrapper.keys.global.is_some(), "global section missing");
        assert!(wrapper.keys.tasks.is_some(), "tasks section missing");
        assert!(wrapper.keys.diff.is_some(), "diff section missing");
        assert!(wrapper.keys.agent.is_some(), "agent section missing");

        // Applying the parsed defaults should produce no warnings
        let (_, warnings) = KeyBindings::with_overrides(&wrapper.keys);
        assert!(warnings.is_empty(), "Unexpected warnings: {warnings:?}");
    }

    #[test]
    fn parse_key_strings() {
        // Single chars
        assert_eq!(KeyBind::parse("j").unwrap().code, KeyCode::Char('j'));
        assert_eq!(KeyBind::parse("O").unwrap().code, KeyCode::Char('O'));
        assert_eq!(KeyBind::parse("!").unwrap().code, KeyCode::Char('!'));

        // Ctrl combos
        let ctrl_o = KeyBind::parse("Ctrl-O").unwrap();
        assert_eq!(ctrl_o.code, KeyCode::Char('o'));
        assert!(ctrl_o.modifiers.contains(KeyModifiers::CONTROL));

        // Shift combos
        let shift_enter = KeyBind::parse("Shift-Enter").unwrap();
        assert_eq!(shift_enter.code, KeyCode::Enter);
        assert!(shift_enter.modifiers.contains(KeyModifiers::SHIFT));

        // Named keys
        assert_eq!(KeyBind::parse("Enter").unwrap().code, KeyCode::Enter);
        assert_eq!(KeyBind::parse("Esc").unwrap().code, KeyCode::Esc);
        assert_eq!(KeyBind::parse("Up").unwrap().code, KeyCode::Up);
        assert_eq!(KeyBind::parse("Down").unwrap().code, KeyCode::Down);
        assert_eq!(KeyBind::parse("F1").unwrap().code, KeyCode::F(1));

        // Invalid
        assert!(KeyBind::parse("").is_err());
        assert!(KeyBind::parse("Ctrl-").is_err());
        assert!(KeyBind::parse("Ctrl-AB").is_err());
        assert!(KeyBind::parse("F0").is_err());
        assert!(KeyBind::parse("F13").is_err());
        assert!(KeyBind::parse("Unknown").is_err());
    }

    #[test]
    fn duplicate_key_warning() {
        let mut raw = HashMap::new();
        raw.insert("move-down".to_string(), vec!["j".to_string()]);
        raw.insert("new-task".to_string(), vec!["j".to_string()]);

        let mut bindings = build_defaults(tasks_action_defs());
        let mut warnings = Vec::new();
        apply_overrides(
            &mut bindings,
            tasks_action_defs(),
            &raw,
            "tasks",
            &mut warnings,
        );

        assert!(
            warnings.iter().any(|w| w.contains("Duplicate key")),
            "Expected duplicate key warning, got: {warnings:?}"
        );
    }

    #[test]
    fn display_parse_round_trip() {
        let cases = [
            "j",
            "O",
            "!",
            "/",
            "@",
            "Ctrl-o",
            "Ctrl-w",
            "Enter",
            "Esc",
            "Up",
            "Down",
            "Tab",
            "Backspace",
            "Delete",
            "PageUp",
            "PageDown",
            "Home",
            "End",
            "Space",
            "F1",
            "F5",
            "F12",
            "Shift-Enter",
        ];
        for input in cases {
            let kb = KeyBind::parse(input).unwrap_or_else(|e| panic!("parse({input:?}): {e}"));
            let displayed = kb.display();
            let kb2 = KeyBind::parse(&displayed)
                .unwrap_or_else(|e| panic!("re-parse({displayed:?}) from {input:?}: {e}"));
            assert_eq!(
                kb, kb2,
                "round-trip failed: {input:?} -> display {displayed:?} -> parse {:?} (expected {:?})",
                kb2, kb
            );
        }
    }
}
