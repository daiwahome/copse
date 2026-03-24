# Design Decisions

## Why a thin wrapper around `claude` CLI?

copse runs `claude` as-is inside a PTY. It does not parse claude's output, call its API, or depend on any specific CLI flags. This is intentional:

- **Claude Code CLI updates frequently.** Deep integration (parsing output formats, relying on specific flags) would break on every update. A thin wrapper that just spawns `claude` in a worktree directory is resilient to CLI changes.
- **copse's scope is clear.** copse manages worktrees, branches, and task lifecycle. claude handles everything else — code generation, tool use, conversation. There is no overlap.
- **No lock-in to a specific claude version.** copse works with any version of `claude` that accepts a working directory. No minimum version requirement, no feature detection.

## Why not `claude --worktree`?

Claude Code has a built-in `--worktree` flag that creates worktrees automatically. copse manages worktrees itself instead, for the following reasons:

### Lifecycle control

`claude --worktree` manages the worktree lifecycle internally — it may delete the worktree when the session ends. copse needs tasks to be **resumable**: stop claude, come back later, and resume from the same branch. By managing worktrees directly with `git worktree add`, copse keeps branches alive across restarts. On next launch, copse discovers existing `copse/*` branches and shows them as stopped tasks ready to resume.

### Different goals

`claude --worktree` is designed for isolating a single Claude session. copse is a task manager that tracks multiple tasks, their upstream branches, and their lifecycle.

| Feature                  | `claude --worktree`                | copse                                      |
|--------------------------|------------------------------------|--------------------------------------------|
| Worktree path            | `.claude/worktrees/<n>/`           | `<git-common-dir>/copse-worktrees/<name>`  |
| Branch naming            | `worktree-<n>` (auto-numbered)     | `copse/<name>` (user-named)                |
| Upstream tracking        | No                                 | Yes (git tracking branch)                  |
| Merge / sync operations  | No                                 | Yes (ff, squash, reset)                    |
| Task lifecycle           | Single session                     | Create / stop / resume / delete            |
| Git-native inspection    | Limited                            | Full (`git branch -vv`, etc.)              |

### Git-native design

copse uses standard git primitives (branches, tracking branches, worktrees) so that everything is inspectable and modifiable with plain `git` commands. `claude --worktree` uses auto-numbered branches and paths that are less meaningful to the user.

## Why task rename is not supported

Renaming a task would require renaming the git branch (`copse/<name>`) and moving the worktree directory. While both operations are straightforward with `git branch -m` and `git worktree move`, Claude Code's `--continue` session is tied to the worktree path. After a rename, the session history would be lost and a new session would start on resume. Since session continuity is a core feature of copse's task lifecycle, task rename is intentionally not supported.
