# Development Workflow

This guide walks through the full development cycle — from creating a branch to opening a pull request.

## Overview

```
[git]    1. Create a feature branch (upstream)
         ─────────────────────────────────────
[copse]  2. Launch copse
[copse]  3. Create a task (select the feature branch as upstream)
[copse]  4. Start the agent — the agent does the work
[copse]  5. Review the diff, send review comments
[copse]  6. Iterate (agent responds to the review)
[copse]  7. Merge the task into upstream (ff or squash)
         ─────────────────────────────────────
[git]    8. Create a pull request
```

## Recommended Settings

Generate the config file with `copse --init`, then enable `auto_commit`:

```sh
copse --init
# → Created ~/.config/copse/config.toml
```

```toml
auto_commit = true
```

With `auto_commit` enabled, the agent's changes are committed automatically after each response. This keeps every iteration preserved in git history. See [Configuration](configuration.md#auto-commit) for details.

## Step by Step

### 1. Create a feature branch

Outside copse, create the branch that will serve as the task's upstream:

```sh
git switch -c feature-x
```

### 2. Launch copse

Run copse from within the repository:

```sh
copse
```

### 3. Create a task

1. Press `n` to open the new-task dialog.
2. Enter a task name and press `Enter`.
3. Select the upstream branch (`feature-x`) from the list and press `Enter`.

copse creates a branch `copse/<name>` and a worktree, forked from the selected upstream.

### 4. Start the agent

Press `a` on the task to start the agent using the default agent from your config. The Agent view opens and the agent begins working in the task's worktree.

To choose a different agent just for this launch, press `A` (Shift+A) instead — a dialog appears where you can pick `claudecode`, `codex`, or `copilotcli`. The `agent` value in your config stays as the default.

### 5. Review changes

To review changes, switch from the Agent view back to the Tasks view, then open the Diff view:

1. Press `Ctrl-Q` to close the Agent view (or `Ctrl-W` to move focus to the Tasks pane in split layout).
2. Press `d` on the task to open the Diff view.

> **Note**: The Diff view can only be opened from the Tasks view — it cannot be opened directly from the Agent view.

In the Diff view:

- Navigate with `j`/`k`, jump between hunks with `@`.
- Press `/` to search within the diff (`n`/`N` to navigate matches).
- Press `o` on a line to add an inline review comment (`Ctrl-s` to save, `Esc` to cancel).

### 6. Iterate

Press `S` to send all comments to the agent. The agent resumes and addresses the feedback.

Repeat the review-and-iterate cycle (steps 5–6) until you are satisfied with the changes.

### 7. Merge the task into upstream

Once the task is complete:

1. Stop the agent if it is still running (e.g., type `exit` or press `Ctrl-D` twice in the Agent view).
2. Press `M` on the stopped task.
3. Choose a merge strategy:
   - **Fast-forward** — Keeps individual commits on the upstream branch.
   - **Squash** — Combines all task commits into one commit on the upstream branch.

After merging, the task shows `synced` (0 commits ahead).

**Cleaning up commit messages**: Since auto-commit produces many small commits, it is worth tidying the history:

- **Squash merge** cleans up at merge time — all task commits are combined into a single commit.
- **Fast-forward merge** keeps commits as-is. To clean up afterward, edit the history on the feature branch (e.g., `git rebase -i`), then sync (`S`) the task to reset it to the updated upstream.

### 8. Create a pull request

Outside copse, create a pull request from the upstream branch using your preferred method (e.g., `gh pr create`, web UI).

Optionally, delete the task in copse with `!` to clean up the worktree and branch.
