# Git マッピング

copse は git ネイティブな設計。全ての概念と操作は標準的な git コマンドに直接対応している。copse を介さず `git` で直接操作することも可能で、次回起動時やリフレッシュ (`Ctrl-r`) で変更が反映される。

## コンセプト → Git プリミティブ

| copse の概念     | git プリミティブ                                            |
|------------------|-------------------------------------------------------------|
| Task             | ブランチ `copse/<name>` + worktree                          |
| Upstream         | Tracking branch (`git branch --set-upstream-to`)            |
| Task status      | ランタイムのみ (git に保存されない)                          |
| Commits ahead    | `git rev-list --count <upstream>..<branch>`                 |

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
git -C <upstream-worktree> merge --ff-only <task-commit>
```

**マージ (squash)** (upstream の worktree 内で実行)
```sh
git merge --squash copse/<name>
git commit
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

