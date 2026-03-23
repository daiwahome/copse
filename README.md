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

## Preview

TODO

## Requirements

- [Claude Code](https://docs.anthropic.com/en/docs/claude-code)
- A git repository (copse must be run from within one)

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
 ├── main.rs      Entry point, SIGTSTP handling, panic hook
 ├── tui.rs       Ratatui + Crossterm event loop
 ├── app.rs       Application state (tasks, mode, key handling)
 ├── task.rs      Manages git worktrees, spawns `claude` in a PTY
 │                Reads PTY output → vt100 parser → screen buffer
 ├── config.rs    Configuration (confy, ~/.config/copse/)
 ├── event.rs     AppEvent enum (key, task lifecycle, resize)
 ├── templates/
 │    └── settings.local.json   Claude Code settings template
 └── ui/
      ├── mod.rs     Layout, status bars, dialogs
      ├── list.rs    Task list panel
      └── agent.rs   PseudoTerminal widget (tui-term)
```

## License

[MIT](LICENSE)
