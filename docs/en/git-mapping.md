# Git Mapping

copse is designed to be git-native. Every concept and operation maps directly to standard git commands. You can always bypass copse and work with git directly — copse will pick up the changes on next launch or refresh (`R`).

## Concepts → Git Primitives

| copse concept | git primitive                                    |
| ------------- | ------------------------------------------------ |
| Task          | Branch `copse/<name>` + worktree                 |
| Upstream      | Tracking branch (`git branch --set-upstream-to`) |
| Task status   | Runtime only (not stored in git)                 |
| Session state | Marker file outside git (`<name>.has-session`)   |
| Commits ahead | `git rev-list --count <upstream>..<branch>`      |

## Operations → Git Commands

**Create task**

```sh
git worktree add -b copse/<name> <path> <upstream>
git branch --set-upstream-to=<upstream> copse/<name>
```

**Resume task** (if worktree was removed)

```sh
git worktree add <path> copse/<name>
```

**Delete task**

```sh
git worktree remove --force <path>
git branch -D copse/<name>
```

**Merge (fast-forward)**

```sh
# If upstream is checked out in a worktree:
git -C <upstream-worktree> merge --ff-only <task-commit>
# If upstream is not checked out anywhere:
git branch -f <upstream> <task-commit>
```

**Merge (squash)**

```sh
# If upstream is checked out in a worktree, run inside that worktree:
git merge --squash copse/<name>
git commit
git -C <task-worktree> reset --hard <upstream>
# If upstream is not checked out, temporarily switch the task worktree:
git -C <task-worktree> switch <upstream>
git -C <task-worktree> merge --squash copse/<name>
git -C <task-worktree> commit
git -C <task-worktree> switch copse/<name>
git -C <task-worktree> reset --hard <upstream>
```

**Sync from upstream**

```sh
git reset --hard <upstream>   # inside worktree
```

**Read upstream**

```sh
git for-each-ref --format='%(upstream:short)' refs/heads/copse/<name>
```

**Change upstream**

```sh
git branch --set-upstream-to=<new-upstream> copse/<name>
```

**List tasks**

```sh
git branch --list 'copse/*'
```
