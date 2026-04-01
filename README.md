# copse

A TUI for running Claude Code tasks in parallel using git worktrees.

Inspired by [tig](https://github.com/jonas/tig). Wraps the `claude` CLI as-is — copse only provides the frontend.

## Concepts

- **Task** — A unit of work, backed by a git branch (`copse/<name>`) and a [git worktree](https://git-scm.com/docs/git-worktree). Each task runs an independent `claude` process.
- **Upstream** — The branch a task was forked from, stored as a git [tracking branch](https://git-scm.com/book/en/v2/Git-Branching-Remote-Branches). Tasks can be merged back into or synced from their upstream.

```
upstream branch (e.g. feature-x)
 ├── copse/task-a  (git worktree + tracking branch)
 ├── copse/task-b
 └── copse/task-c
```

## Features

- **Parallel execution** — Run multiple Claude Code tasks simultaneously, each isolated in its own git worktree
- **Task lifecycle** — Create, start, stop, merge, sync, and delete tasks from one place
- **Code review** — Unified diffs with hunk navigation, search, and inline review comments sent to the agent
- **Split layouts** — Tasks + Diff, Tasks + Agent, or Diff + Agent side by side; toggle fullscreen
- **Configurable** — TOML config for key bindings, colors, auto-commit, and auto-permissions

## Preview

![preview](./docs/preview.gif)

## Requirements

- [Claude Code](https://docs.anthropic.com/en/docs/claude-code)
- A git repository (copse must be run from within one)

### Optional

- [delta](https://github.com/dandavison/delta) — When installed, the diff view uses delta for syntax highlighting and word-level emphasis. Without delta, diffs are shown with plain tig-style coloring.
- [tmux](https://github.com/tmux/tmux) (3.0+) — When configured as the backend (`backend = "tmux"` in config), Claude Code processes run inside tmux sessions and continue running after copse exits. Without tmux, the built-in backend is used and processes are killed on exit. See [Configuration](docs/en/configuration.md#backend) for details.

### Recommended Terminals

copse uses the [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/) for accurate key handling. The following terminals are recommended:

- [Ghostty](https://ghostty.org/)
- [Kitty](https://sw.kovidgoyal.net/kitty/)
- [iTerm2](https://iterm2.com/)
- [WezTerm](https://wezfurlong.org/wezterm/)

See also: [Claude Code terminal setup](https://code.claude.com/docs/en/terminal-config)

## Installation

### Homebrew (macOS)

```sh
brew tap daiwahome/copse
brew install copse
```

### Build from source

Requires the [Rust toolchain](https://rustup.rs/).

```sh
git clone https://github.com/daiwahome/copse.git
cd copse
cargo install --path .
```

## Usage

Run `copse` from within a git repository:

```sh
copse
```

For key bindings and view details, see [Views](docs/en/views.md).

## Documentation

- [Configuration](docs/en/configuration.md) — Settings, auto-commit, and auto-permissions
- [Git Mapping](docs/en/git-mapping.md) — How copse concepts map to git commands
- [Views](docs/en/views.md) — Detailed view descriptions and key bindings
- [Design Decisions](docs/en/design-decisions.md) — Design choices and rationale
- [日本語 README](docs/ja/README.md)

## How It Works

```
copse/src
 ├── main.rs         Entry point, SIGTSTP handling, panic hook
 ├── tui.rs          Ratatui + Crossterm event loop, suspend/resume
 ├── app.rs          Application state (tasks, views, key handling)
 ├── task.rs         Manages git worktrees, spawns `claude` in a PTY
 │                   Reads PTY output → vt100 parser → screen buffer
 ├── agent.rs        Agent configuration, CLAUDE.md management
 ├── backend.rs      Process backend (builtin PTY / tmux sessions)
 ├── diff.rs         Unified diff parser, search, inline comments
 ├── diff_filter.rs  Diff colorizer (delta integration)
 ├── shell.rs        Shell mode (suspend / tmux window)
 ├── config.rs       TOML configuration (~/.config/copse/config.toml)
 ├── keybind.rs      Key binding definitions and TOML overrides
 ├── event.rs        AppEvent enum (key, task lifecycle, resize)
 ├── theme.rs        Color theme from config
 ├── logging.rs      Log file management
 ├── templates/
 │    └── settings.local.json   Claude Code settings template
 └── ui/
      ├── mod.rs     Layout, status bars, dialogs
      ├── list.rs    Task list panel
      ├── diff.rs    Diff view rendering
      └── agent.rs   Agent terminal view (tui-term)
```

## Development

```sh
cargo fmt --check             # Check Rust formatting
cargo clippy -- -D warnings   # Lint
cargo test                    # Run tests
cargo build --release         # Release build
dprint check                  # Check Markdown formatting
```

These checks run automatically via GitHub Actions on every pull request and push to `main`.

## License

[MIT](LICENSE)
