# Git マッピング

copse は git ネイティブな設計。全ての概念と操作は標準的な git コマンドに直接対応している。copse を介さず `git` で直接操作することも可能で、次回起動時やリフレッシュ (`R`) で変更が反映される。

## コンセプト → Git プリミティブ

| copse の概念  | git プリミティブ                                 |
| ------------- | ------------------------------------------------ |
| Task          | ブランチ `copse/<name>` + worktree               |
| Upstream      | Tracking branch (`git branch --set-upstream-to`) |
| Task status   | ランタイムのみ (git に保存されない)              |
| Commits ahead | `git rev-list --count <upstream>..<branch>`      |

## 操作 → Git コマンド

**タスク作成**

```sh
git worktree add -b copse/<name> <path> <upstream>
git branch --set-upstream-to=<upstream> copse/<name>
```

**タスク再開** (worktree が削除済みの場合)

```sh
git worktree add <path> copse/<name>
```

**タスク削除**

```sh
git worktree remove --force <path>
git branch -D copse/<name>
```

**マージ (fast-forward)**

```sh
# upstream が worktree にチェックアウトされている場合:
git -C <upstream-worktree> merge --ff-only <task-commit>
# upstream がどの worktree にもチェックアウトされていない場合:
git branch -f <upstream> <task-commit>
```

**マージ (squash)**

```sh
# upstream が worktree にチェックアウトされている場合、その worktree 内で実行:
git merge --squash copse/<name>
git commit
git -C <task-worktree> reset --hard <upstream>
# upstream が未チェックアウトの場合、タスク worktree で一時的に switch:
git -C <task-worktree> switch <upstream>
git -C <task-worktree> merge --squash copse/<name>
git -C <task-worktree> commit
git -C <task-worktree> switch copse/<name>
git -C <task-worktree> reset --hard <upstream>
```

**upstream から同期**

```sh
git reset --hard <upstream>   # worktree 内で実行
```

**upstream の確認**

```sh
git for-each-ref --format='%(upstream:short)' refs/heads/copse/<name>
```

**upstream の変更**

```sh
git branch --set-upstream-to=<new-upstream> copse/<name>
```

**タスク一覧**

```sh
git branch --list 'copse/*'
```
