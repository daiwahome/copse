# Configuration

copse stores its configuration in a TOML file.

## Configuration File

**Path**: `~/.config/copse/config.toml`

Generate the config file with `--init`:

```sh
copse --init
# → Created ~/.config/copse/config.toml
```

If the file already exists, the command exits with an error to prevent overwriting. If the file does not exist, copse uses default values for all options.

```toml
agent = "claudecode"
backend = "builtin"
diff_filter = "none"
auto_commit = false
auto_permissions = false
log_level = "info"
# notification_command = "osascript -e 'display notification \"Needs input\" with title \"Copse\"'"

[claudecode]
permission_mode = "default"

[color]
cursor = { bg = "236" }
cursor-blur = { fg = "252", bg = "234" }
title-focus-tasks = { fg = "black", bg = "166" }
title-focus-agent = { fg = "black", bg = "217" }
title-focus-diff = { fg = "black", bg = "33" }
title-blur = { fg = "black", bg = "240" }
title-text-focus = { fg = "252", bg = "234" }
title-text-blur = { fg = "245", bg = "234" }
title-hints = { fg = "245", bg = "234" }
search-result = { bg = "238" }
diff-add = { fg = "green" }
diff-del = { fg = "red" }
diff-chunk = { fg = "cyan" }
diff-header = { fg = "white" }
diff-context = { fg = "white" }
list-highlight = { fg = "166", bg = "234" }
list-highlight-blur = { fg = "252", bg = "234" }
list-header = { fg = "245" }

[keys.global]
focus-toggle = ["Ctrl-W"]
help = ["Ctrl-G"]

[keys.tasks]
new-task = ["n"]
move-down = ["j", "Down"]
move-up = ["k", "Up"]
open = ["a"]
show-diff = ["d", "Enter"]
merge = ["M"]
sync = ["S"]
change-upstream = ["U"]
delete = ["!"]
refresh = ["R"]
fullscreen = ["O", "Ctrl-O"]
quit = ["q", "Q"]
kill = ["Ctrl-K"]
close-children = ["Ctrl-Q"]
start-fresh = ["Ctrl-A"]

[keys.diff]
move-down = ["j", "Down"]
move-up = ["k", "Up"]
next-hunk = ["@"]
search = ["/"]
search-next = ["n"]
search-prev = ["N"]
refresh = ["R"]
fullscreen = ["O", "Ctrl-O"]
close = ["q", "Esc", "Ctrl-Q"]
page-up = ["Ctrl-B"]
page-down = ["Ctrl-F"]
half-page-up = ["Ctrl-U"]
half-page-down = ["Ctrl-D"]
add-comment = ["o"]
edit-comment = ["e"]
delete-comment = ["!"]
send-review = ["S"]
next-comment = ["c"]

[keys.agent]
fullscreen = ["Ctrl-O"]
close = ["Ctrl-Q"]
page-up = ["Ctrl-B"]
page-down = ["Ctrl-F"]
line-up = ["k"]
line-down = ["j"]
half-page-up = ["Ctrl-U"]
half-page-down = ["Ctrl-D"]
exit-scroll-mode = ["q", "Enter"]
```

## Options

| Option                 | Type            | Default        | Description                                                             |
| ---------------------- | --------------- | -------------- | ----------------------------------------------------------------------- |
| `agent`                | string          | `"claudecode"` | Agent to use: `"claudecode"`                                            |
| `backend`              | string          | `"builtin"`    | Process backend: `"builtin"` or `"tmux"`                                |
| `diff_filter`          | string          | `"none"`       | Diff colorizer: `"none"` or `"delta"`                                   |
| `auto_commit`          | bool            | `false`        | Auto-commit changes after each agent response                           |
| `auto_permissions`     | bool            | `false`        | Auto-approve safe commands in the agent                                 |
| `log_level`            | string          | `"info"`       | Log level: `"trace"`, `"debug"`, `"info"`, `"warn"`, `"error"`, `"off"` |
| `notification_command` | string (option) | —              | Command to run when the agent is waiting for user input                 |

### `[claudecode]` Section

Agent-specific options for Claude Code.

| Option            | Type   | Default     | Description                               |
| ----------------- | ------ | ----------- | ----------------------------------------- |
| `permission_mode` | string | `"default"` | Claude Code permission mode for all tasks |

### Permission Mode

When set, copse passes `--permission-mode <mode>` to every `claude` invocation. Available modes:

| Mode                | Description                              |
| ------------------- | ---------------------------------------- |
| `default`           | Normal — prompts for every tool use      |
| `acceptEdits`       | Auto-approve file edits, prompt for Bash |
| `plan`              | Plan only, no execution                  |
| `auto`              | Auto-approve edits and Bash              |
| `bypassPermissions` | Skip all permission checks               |
| `dontAsk`           | Skip disallowed operations silently      |

The mode can be changed during a session within Claude Code (e.g. via `/permissions`).

### Backend

Controls how Claude Code processes are managed.

| Value     | Description                                                                                          |
| --------- | ---------------------------------------------------------------------------------------------------- |
| `builtin` | Default — runs claude directly in a PTY; processes are killed when copse exits                       |
| `tmux`    | Runs claude inside a tmux session; processes continue running after copse exits (requires tmux 3.0+) |

When using the `tmux` backend:

- Claude processes keep running in the background when copse exits
- On restart, copse detects existing tmux sessions and shows them as Running
- Opening a Running (detached) task reattaches to the tmux session
- Requires tmux to be installed; copse exits with an error if tmux is not found

copse runs its own tmux server (socket `copse`) with no user configuration. Sessions are named `<host>/<owner>/<repo>/<task>`. You can check running sessions with:

```sh
tmux -L copse list-sessions
```

### Diff Filter

Controls how diffs are colorized in the diff view.

| Value   | Description                                                                                               |
| ------- | --------------------------------------------------------------------------------------------------------- |
| `none`  | Default — no external filter; plain tig-style coloring (green/red foreground)                             |
| `delta` | Use [delta](https://github.com/dandavison/delta) for syntax highlighting (requires delta to be installed) |

When delta is used, diffs get syntax highlighting, colored backgrounds, and word-level emphasis.

## Color Theme

The `[color]` section lets you customize UI colors. Each entry accepts `fg` (foreground) and `bg` (background). Omitted fields keep their default values.

### Color Formats

| Format          | Example                      | Description                                     |
| --------------- | ---------------------------- | ----------------------------------------------- |
| Color name      | `"red"`, `"green"`, `"blue"` | 8 basic colors + `"default"` (terminal default) |
| 256-color index | `"166"`, `"234"`             | Numeric string from `0` to `255`                |

Available color names: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, `default`

### Color Areas

| Key                   | Description                        |
| --------------------- | ---------------------------------- |
| `cursor`              | Cursor line when focused           |
| `cursor-blur`         | Cursor line when unfocused         |
| `title-focus-tasks`   | TASKS status bar badge (focused)   |
| `title-focus-agent`   | AGENT status bar badge (focused)   |
| `title-focus-diff`    | DIFF status bar badge (focused)    |
| `title-blur`          | Status bar badge (unfocused)       |
| `title-text-focus`    | Status bar text (focused)          |
| `title-text-blur`     | Status bar text (unfocused)        |
| `title-hints`         | Status bar key hints               |
| `search-result`       | Search match highlight             |
| `diff-add`            | Diff added lines                   |
| `diff-del`            | Diff deleted lines                 |
| `diff-chunk`          | Diff hunk header                   |
| `diff-header`         | Diff file header                   |
| `diff-context`        | Diff context lines                 |
| `list-highlight`      | Task list selected row (focused)   |
| `list-highlight-blur` | Task list selected row (unfocused) |
| `list-header`         | Task list header row               |

Invalid color names show a warning in the status bar on startup.

## Key Bindings

The `[keys.*]` sections let you customize key bindings per view. Each action maps to an array of key strings.

### Override Behavior

Only the actions you specify are overridden; unspecified actions keep their defaults. For example, if you only set `move-down` in `[keys.tasks]`, all other task view bindings remain unchanged.

Setting an action to an empty array `[]` disables that action entirely.

Dialog key bindings (confirm dialogs, text input, etc.) are not configurable.

### Key Notation

| Format       | Example                                                                           | Description                  |
| ------------ | --------------------------------------------------------------------------------- | ---------------------------- |
| Single char  | `"a"`, `"O"`, `"!"`, `"/"`                                                        | Lowercase, uppercase, symbol |
| Ctrl combo   | `"Ctrl-O"`, `"Ctrl-W"`                                                            | Control + key                |
| Named key    | `"Enter"`, `"Esc"`, `"Tab"`                                                       | Special keys                 |
| Arrow key    | `"Up"`, `"Down"`, `"Left"`, `"Right"`                                             | Arrow keys                   |
| Function key | `"F1"` .. `"F12"`                                                                 | Function keys                |
| Other        | `"Backspace"`, `"Delete"`, `"Space"`, `"PageUp"`, `"PageDown"`, `"Home"`, `"End"` | Other special keys           |

### Views

| Section         | Description                                                      |
| --------------- | ---------------------------------------------------------------- |
| `[keys.global]` | Bindings active in all views (checked before view-specific ones) |
| `[keys.tasks]`  | Task list view                                                   |
| `[keys.diff]`   | Diff view                                                        |
| `[keys.agent]`  | Agent (PTY) view — unbound keys are forwarded to the PTY process |

### Example

```toml
[keys.tasks]
move-down = ["j"]       # remove Down arrow from move-down
fullscreen = ["O", "Ctrl-O", "F11"]  # add F11
```

Invalid key strings or unknown action names show a warning in the status bar on startup.

## Auto-Commit

When `auto_commit` is enabled, copse installs a Claude Code [Stop hook](https://docs.anthropic.com/en/docs/claude-code/hooks) in each worktree. After every Claude response, the hook:

1. Stages all changes (`git add -A`)
2. Skips if there are no staged changes (`git diff --cached --quiet`)
3. Commits with the message `copse auto-commit`

The commits ahead count in the Tasks view refreshes every 5 seconds, so you can see new commits appear shortly after they are created.

## Auto-Permissions

When `auto_permissions` is enabled, copse pre-approves the following safe commands so Claude Code does not prompt for confirmation:

| Category        | Commands                                                                                                                                                |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Git (read-only) | `blame`, `branch`, `cat-file`, `config`, `diff`, `log`, `ls-files`, `ls-tree`, `remote`, `rev-parse`, `shortlog`, `show`, `stash list`, `status`, `tag` |
| Directory       | `find`, `ls`, `tree`, `pwd`, `mkdir`                                                                                                                    |
| Text processing | `wc`, `diff` (coreutils), `sort`, `uniq`, `cut`                                                                                                         |
| Utilities       | `echo`, `which`, `file`, `date`, `basename`, `dirname`                                                                                                  |
| Built-in tools  | `Edit`, `NotebookEdit`, `WebFetch`, `WebSearch`, `Write`                                                                                                |

`Edit`, `Write`, and `NotebookEdit` are restricted to the worktree directory so the agent cannot modify files outside its workspace.

Additionally, reading sensitive files is denied by default:

| Category           | Paths                                                             |
| ------------------ | ----------------------------------------------------------------- |
| Credentials & keys | `~/.ssh`, `~/.gnupg`, `~/.aws`, `~/.config/gcloud`, `~/.azure`    |
| Secrets files      | `**/.env`, `**/.env.*`, `**/*.pem`, `**/*.key`                    |
| Shell history      | `~/.bash_history`, `~/.zsh_history`                               |
| Auth configs       | `~/.netrc`, `~/.docker/config.json`, `~/.kube/config`, `~/.npmrc` |

## Logging

copse writes logs to `~/.local/state/copse/copse.log` (XDG state directory). Since copse is a TUI application, logs cannot be printed to stdout/stderr — they are written to the log file instead.

The log level is determined by (in priority order):

1. `COPSE_LOG` environment variable
2. `log_level` field in config
3. Default: `info`

| Level   | Description                                |
| ------- | ------------------------------------------ |
| `trace` | Very detailed internal state               |
| `debug` | Diagnostic information for troubleshooting |
| `info`  | General operational messages (default)     |
| `warn`  | Potential issues                           |
| `error` | Errors that affect functionality           |
| `off`   | Disable logging entirely                   |

To temporarily change the log level without editing the config:

```sh
COPSE_LOG=debug copse
```

## Notification Command

When set, copse installs a Claude Code [Notification hook](https://docs.anthropic.com/en/docs/claude-code/hooks) in each worktree. The hook runs the specified command whenever Claude Code is waiting for user input.

Omit the key entirely to disable notifications. Setting `notification_command = ""` is a validation error.

### Example: macOS Native Notification

```toml
notification_command = "osascript -e 'display notification \"Needs input\" with title \"Copse\"'"
```

### Example: Terminal Bell

```toml
notification_command = "printf '\\a'"
```

### Example: Both

```toml
notification_command = "printf '\\a' && osascript -e 'display notification \"Needs input\" with title \"Copse\"'"
```

### Terminal Bell Tips

When using `printf '\a'` as the notification command, the terminal emulator receives a bell character. Most terminals can be configured to:

- **Bounce the Dock icon** (macOS Terminal, iTerm2)
- **Flash the taskbar** (Windows Terminal)
- **Show a visual indicator** (many Linux terminals)

Check your terminal's notification/bell settings for the desired behavior.

## Settings Merge Strategy

copse writes a `.claude/settings.local.json` file into each worktree when a task is launched. The final settings are built by merging layers in order:

1. **Repository settings** — `.claude/settings.local.json` at the repository root (if it exists)
2. **copse template** — built-in permissions and hooks (controlled by `auto_commit` / `auto_permissions`)
3. **Sensitive-path deny rules** — automatically generated when `auto_permissions` is enabled

Arrays are concatenated and deduplicated, so additional permissions added in the repository settings are preserved alongside the copse template.
