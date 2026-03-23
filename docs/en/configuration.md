# Configuration

copse stores its configuration in a TOML file managed by [confy](https://github.com/rust-cli/confy).

## Configuration File

**Path**: `~/.config/copse/default-config.toml`

Generate the config file with `--init`:

```sh
copse --init
# → Created ~/.config/copse/default-config.toml
```

If the file already exists, the command exits with an error to prevent overwriting. If the file does not exist, copse uses default values for all options.

```toml
auto_commit = false
auto_permissions = false
permission_mode = "default"
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
