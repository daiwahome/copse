# Views

copse has three main views, inspired by [tig](https://github.com/jonas/tig)'s split-pane layout.

## Tasks View

The default view. Shows all tasks with their status, upstream branch, and commit progress.

### Display

```
  Name     Upstream    Status    Commits
▶ task-a   feature-x   running   3 ahead
⏸ task-b   develop     waiting   synced
■ task-c   feature-y   stopped   1 ahead
```

Each task shows:

| Element                | Description                                                                               |
| ---------------------- | ----------------------------------------------------------------------------------------- |
| Icon (`▶` / `⏸` / `■`) | Running (active) / Waiting (prompt) / Stopped                                             |
| Name                   | Task name (also used as branch suffix: `copse/<name>`)                                    |
| Upstream               | The branch the task was forked from (turns red when the upstream branch no longer exists) |
| Status text            | `running` / `waiting` / `stopped`                                                         |
| Commits ahead          | Number of commits ahead of upstream, or `synced`                                          |

### Key Bindings

| Key           | Action                                          |
| ------------- | ----------------------------------------------- |
| `j` / `↓`     | Select next task                                |
| `k` / `↑`     | Select previous task                            |
| `Enter` / `d` | Open diff view (when commits ahead > 0)         |
| `a`           | Open agent view (running) / Start (stopped)     |
| `Ctrl-a`      | Start without `--continue` (stopped only)       |
| `n`           | New task (name → upstream selection)            |
| `Ctrl-k`      | Kill selected task (running only)               |
| `M`           | Merge into upstream (ff / squash, stopped only) |
| `S`           | Sync from upstream (reset, stopped only)        |
| `U`           | Change upstream branch (stopped only)           |
| `!`           | Delete task (worktree + branch, stopped only)   |
| `s`           | Open shell in worktree directory                |
| `R`           | Refresh commits ahead                           |
| `Ctrl-g`      | Show help dialog                                |
| `q` / `Q`     | Quit copse                                      |

### Task Creation Flow

1. Press `n`
2. Enter a task name → `Enter`
3. Select upstream branch from list (`j`/`k` → `Enter`)
4. Task appears as stopped (`■`)
5. Press `a` to start claude in the task's worktree

## Diff View

Shows the unified diff between the task branch and its upstream. Displays the output of `git diff <upstream>..<branch>`.

When [delta](https://github.com/dandavison/delta) is installed, the diff view uses it for syntax highlighting, colored backgrounds, and word-level emphasis. Without delta, diffs are shown with plain tig-style coloring (green/red foreground).

### Layout Modes

**Split view** `[Tasks | Diff]`: Tasks list on the left, diff on the right. Press `d` from the tasks view to open.

```
┌─ Tasks ──────────┬─ Diff ───────────────────────┐
│ ▶ task-a  ...    │ diff --git a/foo.rs b/foo.rs │
│ ■ task-b  ...    │ @@ -1,5 +1,7 @@              │
│                  │ +new line                     │
├──────────────────┼──────────────────────────────┤
│ TASKS status bar │ DIFF status bar              │
└──────────────────┴──────────────────────────────┘
```

**Fullscreen**: Diff output fills the entire screen. Press `O` to toggle.

When an agent is also open, the layout becomes `[Diff | Agent]`: diff on the left, agent on the right.

### Key Bindings (Diff pane)

| Key       | Action                                                |
| --------- | ----------------------------------------------------- |
| `j` / `↓` | Move cursor down                                      |
| `k` / `↑` | Move cursor up                                        |
| `Ctrl-b`  | Scroll up one page                                    |
| `Ctrl-f`  | Scroll down one page                                  |
| `Ctrl-u`  | Scroll up half page                                   |
| `Ctrl-d`  | Scroll down half page                                 |
| `/`       | Open search dialog (enter pattern, `Enter` to search) |
| `n`       | Next search match                                     |
| `N`       | Previous search match                                 |
| `@`       | Jump to next hunk (sets pattern to `^@@`)             |
| `R`       | Refresh diff (preserves cursor position and comments) |
| `O`       | Toggle split ↔ fullscreen                             |
| `q`       | Close diff view (state is restored on reopen)         |
| `o`       | Add review comment on current line (inline editing)   |
| `e`       | Edit existing review comment                          |
| `!`       | Delete review comment on current line                 |
| `c`       | Jump to next comment (then `n`/`N` to navigate)       |
| `S`       | Send all comments to agent (opens preview dialog)     |
| `Ctrl-s`  | Confirm comment (while editing)                       |
| `Esc`     | Cancel comment editing                                |
| `Ctrl-g`  | Show help dialog                                      |

When no search pattern is set, `n`/`N` default to hunk navigation (same as `@`). After pressing `c`, `n`/`N` navigate between commented lines instead.

### Key Bindings (Tasks pane, left focus)

| Key       | Action                                 |
| --------- | -------------------------------------- |
| `j` / `k` | Select task                            |
| `Enter`   | Open diff view                         |
| `a`       | Open agent (running) / Start (stopped) |
| `O`       | Tasks fullscreen                       |
| `Ctrl-w`  | Switch focus to diff pane              |
| `q`       | Close diff, return to tasks fullscreen |

## Agent View

Shows the agent process output. Keystrokes are forwarded to the agent when the agent pane has focus.

### Layout Modes

**Split view** `[Tasks | Agent]` (default): Tasks list on the left, agent output on the right.

```
┌─ Tasks ──────────┬─ Agent ──────────────────────┐
│ ▶ task-a  ...    │ Claude Code v2.x             │
│ ■ task-b  ...    │ > ...                        │
│                  │                              │
├──────────────────┼──────────────────────────────┤
│ TASKS status bar │ AGENT status bar             │
└──────────────────┴──────────────────────────────┘
```

**Fullscreen**: Agent output fills the entire screen.

When a diff is also open, the layout becomes `[Diff | Agent]`: diff on the left, agent on the right.

### Key Bindings (Agent pane, right focus)

| Key       | Action                                    | Overrides    |
| --------- | ----------------------------------------- | ------------ |
| `Ctrl-o`  | Toggle split ↔ fullscreen                 |              |
| `Ctrl-q`  | Close agent view, return to tasks or diff |              |
| `Ctrl-w`  | Switch focus to left pane                 |              |
| `Ctrl-b`  | Scroll up one page (enter scroll mode)    | cursor left  |
| `Ctrl-f`  | Scroll down one page (enter scroll mode)  | cursor right |
| `Ctrl-g`  | Show help dialog                          |              |
| Any other | Forward to the agent                      |              |

#### Scroll mode (builtin backend)

Activated when scrolled back via `Ctrl-b` / `Ctrl-f`. In scroll mode, keys are not forwarded to the PTY.

| Key           | Action             |
| ------------- | ------------------ |
| `k`           | Scroll up 1 line   |
| `j`           | Scroll down 1 line |
| `Ctrl-u`      | Half page up       |
| `Ctrl-d`      | Half page down     |
| `Ctrl-b`      | Page up            |
| `Ctrl-f`      | Page down          |
| `q` / `Enter` | Exit scroll mode   |

#### Copy mode (tmux backend)

When using the tmux backend, `Ctrl-b` / `Ctrl-f` enter tmux copy-mode. Within copy-mode, standard tmux navigation keys are available:

| Key           | Action             |
| ------------- | ------------------ |
| `k`           | Scroll up 1 line   |
| `j`           | Scroll down 1 line |
| `Ctrl-u`      | Half page up       |
| `Ctrl-d`      | Half page down     |
| `Ctrl-b`      | Page up            |
| `Ctrl-f`      | Page down          |
| `/`           | Search             |
| `n`           | Next match         |
| `N`           | Previous match     |
| `q` / `Enter` | Exit copy-mode     |

### Key Bindings (Tasks pane, left focus)

| Key       | Action                                       |
| --------- | -------------------------------------------- |
| `j` / `k` | Select task                                  |
| `d`       | Open diff view in left pane                  |
| `a`       | Focus agent pane (running) / Start (stopped) |
| `O`       | Tasks fullscreen                             |
| `Ctrl-w`  | Switch focus to agent pane                   |
| `q`       | Close agent, return to tasks fullscreen      |

## Focus Switching

In split views, press `Ctrl-w` to toggle focus between the left and right panes. The focused pane's status bar badge is highlighted; the unfocused pane's badge is dimmed.

`Ctrl-o` / `O` and `Ctrl-q` / `q` are equivalent — `Ctrl` variants are provided for the agent pane where regular keys are forwarded to the PTY.

## Status Bar

Each pane has a status bar at the bottom with:

- **Left**: View badge (`TASKS`, `AGENT`, or `DIFF`) + context info
- **Right**: Available key hints
