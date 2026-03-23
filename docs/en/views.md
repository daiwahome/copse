# Views

copse has two main views, inspired by [tig](https://github.com/jonas/tig)'s split-pane layout.

## Tasks View

The default view. Shows all tasks with their status, upstream branch, and commit progress.

### Display

```
 ▶ task-a  (upstream: feature-x)  running   3 ahead
 ⏸ task-b  (upstream: develop)    waiting   synced
 ■ task-c  (upstream: feature-y)  stopped   1 ahead
```

Each task shows:

| Element                | Description                                           |
|------------------------|-------------------------------------------------------|
| Icon (`▶` / `⏸` / `■`) | Running (active) / Waiting (prompt) / Stopped         |
| Name                   | Task name (also used as branch suffix: `copse/<name>`)|
| Upstream               | The branch the task was forked from                   |
| Status text            | `running` / `waiting` / `stopped`                     |
| Commits ahead          | Number of commits ahead of upstream, or `synced`      |

### Key Bindings

| Key              | Action                                         |
|------------------|-------------------------------------------------|
| `j` / `↓`        | Select next task                               |
| `k` / `↑`        | Select previous task                           |
| `Enter`          | Open agent view (running) / Resume (stopped)   |
| `n`              | New task (name → upstream selection)            |
| `Ctrl-k`         | Kill selected task (running only)              |
| `Shift-M`        | Merge into upstream (ff / squash, stopped only)|
| `Shift-S`        | Sync from upstream (reset, stopped only)       |
| `!`              | Delete task (worktree + branch, stopped only)  |
| `Ctrl-r`         | Refresh commits ahead                          |
| `q` / `Q`        | Quit copse                                     |

### Task Creation Flow

1. Press `n`
2. Enter a task name → `Enter`
3. Select upstream branch from list (`j`/`k` → `Enter`)
4. Task appears as stopped (`■`)
5. Press `Enter` to start claude in the task's worktree

## Agent View

Shows the claude process output. All keystrokes are forwarded to claude except `Ctrl-]`.

### Layout Modes

**Split view** (default): Tasks list on the left, agent output on the right.

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

### Key Bindings

| Key         | Action                                             |
|-------------|----------------------------------------------------|
| (TBD)       | Maximize to fullscreen (planned)                   |
| `Ctrl-]`    | Fullscreen → split view, Split → back to Tasks     |
| Any other   | Forward to Claude Code                             |

### Status Bar

Both views have a status bar at the bottom with:

- **Left**: View badge (`TASKS` or `AGENT`) + context info
- **Right**: Available key hints

