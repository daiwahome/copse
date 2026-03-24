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
auto_commit = false
auto_permissions = false
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
```

## Options

| Option             | Type   | Default     | Description                                    |
|--------------------|--------|-------------|------------------------------------------------|
| `auto_commit`      | bool   | `false`     | Auto-commit changes after each Claude response |
| `auto_permissions` | bool   | `false`     | Auto-approve safe commands in Claude Code      |
| `permission_mode`  | string | `"default"` | Claude Code permission mode for all tasks      |

### Permission Mode

When set, copse passes `--permission-mode <mode>` to every `claude` invocation. Available modes:

| Mode                | Description                                          |
|---------------------|------------------------------------------------------|
| `default`           | Normal — prompts for every tool use                  |
| `acceptEdits`       | Auto-approve file edits, prompt for Bash             |
| `plan`              | Plan only, no execution                              |
| `auto`              | Auto-approve edits and Bash                          |
| `bypassPermissions` | Skip all permission checks                           |
| `dontAsk`           | Skip disallowed operations silently                  |

The mode can be changed during a session within Claude Code (e.g. via `/permissions`).

## Color Theme

The `[color]` section lets you customize UI colors. Each entry accepts `fg` (foreground) and `bg` (background). Omitted fields keep their default values.

### Color Formats

| Format            | Example                      | Description                                    |
|-------------------|------------------------------|------------------------------------------------|
| Color name        | `"red"`, `"green"`, `"blue"` | 8 basic colors + `"default"` (terminal default) |
| 256-color index   | `"166"`, `"234"`             | Numeric string from `0` to `255`               |

Available color names: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, `default`

### Color Areas

| Key                   | Description                                  |
|-----------------------|----------------------------------------------|
| `cursor`              | Cursor line when focused                     |
| `cursor-blur`         | Cursor line when unfocused                   |
| `title-focus-tasks`   | TASKS status bar badge (focused)             |
| `title-focus-agent`   | AGENT status bar badge (focused)             |
| `title-focus-diff`    | DIFF status bar badge (focused)              |
| `title-blur`          | Status bar badge (unfocused)                 |
| `title-text-focus`    | Status bar text (focused)                    |
| `title-text-blur`     | Status bar text (unfocused)                  |
| `title-hints`         | Status bar key hints                         |
| `search-result`       | Search match highlight                       |
| `diff-add`            | Diff added lines                             |
| `diff-del`            | Diff deleted lines                           |
| `diff-chunk`          | Diff hunk header                             |
| `diff-header`         | Diff file header                             |
| `diff-context`        | Diff context lines                           |
| `list-highlight`      | Task list selected row (focused)             |
| `list-highlight-blur` | Task list selected row (unfocused)           |

Invalid color names show a warning in the status bar on startup.

## Auto-Commit

When `auto_commit` is enabled, copse installs a Claude Code [Stop hook](https://docs.anthropic.com/en/docs/claude-code/hooks) in each worktree. After every Claude response, the hook:

1. Stages all changes (`git add -A`)
2. Skips if there are no staged changes (`git diff --cached --quiet`)
3. Commits with the message `copse auto-commit`

The commits ahead count in the Tasks view refreshes every 5 seconds, so you can see new commits appear shortly after they are created.

## Auto-Permissions

When `auto_permissions` is enabled, copse pre-approves the following safe commands so Claude Code does not prompt for confirmation:

| Category        | Commands                                                    |
|-----------------|-------------------------------------------------------------|
| Version control | `git`                                                       |
| File reading    | `cat`, `head`, `tail`                                       |
| Search          | `find`, `grep`, `rg`                                        |
| Directory       | `ls`, `tree`, `pwd`, `mkdir`                                |
| Text processing | `wc`, `diff`, `sort`, `uniq`, `cut`                         |
| Utilities       | `echo`, `which`, `file`, `date`, `basename`, `dirname`      |
| Built-in tools  | `Edit`, `NotebookEdit`, `WebFetch`, `WebSearch`, `Write`    |

Build tools (e.g. `cargo`, `npm`) are intentionally excluded as they can execute arbitrary code.

## Settings Merge Strategy

copse writes a `.claude/settings.local.json` file into each worktree when a task is launched. If the file already exists, copse preserves existing keys:

- If `hooks` is already set, copse does not overwrite it
- If `permissions` is already set, copse does not overwrite it
- Only missing keys are populated from the built-in template

This means you can customize a worktree's settings and copse will not reset your changes.
