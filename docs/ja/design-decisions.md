# 設計判断

## なぜ `claude` CLI の薄いラッパーなのか?

copse は `claude` を PTY 内でそのまま実行する。claude の出力をパースしたり、API を呼んだり、特定の CLI フラグに依存したりしない。これは意図的な設計。

- **Claude Code CLI の更新が速い。** 出力フォーマットのパースや特定フラグへの依存は、更新のたびに壊れる。worktree ディレクトリで `claude` を起動するだけの薄いラッパーは、CLI の変更に強い。
- **copse のスコープが明確。** copse は worktree、ブランチ、タスクのライフサイクルを管理する。claude はコード生成、ツール使用、会話を担当する。責務が重複しない。
- **特定の claude バージョンに依存しない。** copse は作業ディレクトリを受け付ける任意のバージョンの `claude` で動作する。最低バージョン要件もフィーチャー検出も不要。

## なぜ `claude --worktree` を使わないのか?

Claude Code には `--worktree` フラグが組み込まれており、worktree を自動作成する。copse が自前で worktree を管理するのは以下の理由による。

### ライフサイクルの制御

`claude --worktree` は worktree のライフサイクルを内部で管理しており、セッション終了時に worktree を削除する可能性がある。copse はタスクの **再開** が必要 — claude を停止して、後で同じブランチから再開する。`git worktree add` で直接管理することで、copse はブランチを再起動をまたいで保持する。次回起動時に既存の `copse/*` ブランチを検出し、再開可能な停止中タスクとして表示する。

### 目的の違い

`claude --worktree` は単一の Claude セッションを分離するための機能。copse は複数のタスクを管理し、upstream ブランチの追跡やライフサイクル管理を行うタスクマネージャー。

| 機能                     | `claude --worktree`                | copse                                      |
|--------------------------|------------------------------------|--------------------------------------------|
| Worktree パス            | `.claude/worktrees/<n>/`           | `<git-common-dir>/copse-worktrees/<name>`  |
| ブランチ命名             | `worktree-<n>` (自動採番)           | `copse/<name>` (ユーザー指定)              |
| Upstream 追跡            | なし                               | あり (git tracking branch)                 |
| マージ / 同期操作        | なし                               | あり (ff, squash, reset)                   |
| タスクライフサイクル     | 単一セッション                     | 作成 / 停止 / 再開 / 削除                   |
| git での直接確認         | 限定的                             | 完全 (`git branch -vv` 等)                  |

### git ネイティブ設計

copse は標準的な git プリミティブ（ブランチ、tracking branch、worktree）を使用するため、全て `git` コマンドで直接確認・変更できる。`claude --worktree` は自動採番されたブランチとパスを使うため、ユーザーにとって意味が分かりにくい。

## なぜタスクのリネームをサポートしないのか

タスクのリネームには git ブランチ (`copse/<name>`) のリネームと worktree ディレクトリの移動が必要。`git branch -m` と `git worktree move` で技術的には可能だが、Claude Code の `--continue` セッションは worktree パスに紐づいている。リネーム後はセッション履歴が失われ、再開時に新しいセッションとして開始される。セッションの継続性は copse のタスクライフサイクルの中核機能であるため、タスクのリネームは意図的にサポートしていない。

## なぜ `git diff` をサブプロセスで実行するのか

Diff view は `git diff <upstream>..<branch>` をサブプロセスとして実行し、unified diff 出力を直接パースする。これは最もシンプルなアプローチで、tig と同じ方式。copse が既に git と対話している方法（`rev-list`、`worktree add`、`branch` 等で `std::process::Command` を使用）とも一貫性がある。`git2` や diff パース用 crate 等の追加依存は不要。
