# 開発ワークフロー

ブランチの作成からプルリクエストの作成までの一連の開発フローを説明する。

## 概要

```
[git]    1. feature branch (upstream) を作成
         ─────────────────────────────────────
[copse]  2. copse を起動
[copse]  3. task を作成 (feature branch を upstream に選択)
[copse]  4. agent を開始 — エージェントが作業
[copse]  5. diff を確認、レビューコメントを送信
[copse]  6. 反復 (agent がレビューに応答)
[copse]  7. task を upstream にマージ (ff or squash)
         ─────────────────────────────────────
[git]    8. pull request を作成
```

## 推奨設定

`copse --init` で設定ファイルを生成し、`auto_commit` を有効にする:

```sh
copse --init
# → Created ~/.config/copse/config.toml
```

```toml
auto_commit = true
```

`auto_commit` を有効にすると、エージェントの変更が応答ごとに自動コミットされる。これにより各イテレーションが git 履歴に保存される。詳細は[設定](configuration.md#auto-commit)を参照。

## 手順

### 1. feature branch を作成

copse の外で、タスクの upstream となるブランチを作成する:

```sh
git switch -c feature-x
```

### 2. copse を起動

リポジトリ内で copse を実行する:

```sh
copse
```

### 3. task を作成

1. `n` を押して新規タスクダイアログを開く。
2. タスク名を入力して `Enter`。
3. upstream ブランチ (`feature-x`) をリストから選択して `Enter`。

copse が選択した upstream から分岐した `copse/<name>` ブランチと worktree を作成する。

### 4. agent を開始

タスク上で `a` を押してエージェントを開始する。Agent view が開き、エージェントがタスクの worktree で作業を始める。

### 5. 変更を確認

変更を確認するには、Agent view から Tasks view に戻り、Diff view を開く:

1. `Ctrl-Q` で Agent view を閉じる（または分割レイアウトで `Ctrl-W` で Tasks ペインにフォーカスを移動）。
2. タスク上で `d` を押して Diff view を開く。

> **注意**: Diff view は Tasks view からのみ開ける — Agent view から直接開くことはできない。

Diff view での操作:

- `j`/`k` で移動、`@` でハンク間をジャンプ。
- `/` で diff 内を検索（`n`/`N` でマッチ間を移動）。
- 行上で `o` を押してインラインレビューコメントを追加（`Ctrl-s` で確定、`Esc` でキャンセル）。

### 6. 反復

`S` を押してすべてのコメントを agent に送信する。agent が再開しフィードバックに対応する。

変更に満足するまでレビューと反復のサイクル（ステップ 5–6）を繰り返す。

### 7. task を upstream にマージ

タスクが完了したら:

1. agent がまだ実行中の場合は停止する（例: Agent view で `exit` と入力、または `Ctrl-D` を2回押す）。
2. 停止したタスク上で `M` を押す。
3. マージ方法を選択する:
   - **Fast-forward** — 個別のコミットをそのまま upstream ブランチに残す。
   - **Squash** — タスクのコミットを1つにまとめて upstream ブランチにコミットする。

マージ後、タスクは `synced`（0 commits ahead）と表示される。

**コミットメッセージの整理**: auto_commit では細かいコミットが多数作成されるため、履歴を整理するとよい:

- **Squash merge** ではマージと同時に整理される — タスクのコミットが1つにまとめられる。
- **Fast-forward merge** ではコミットがそのまま残る。整理するには、feature branch 側で `git rebase -i` 等を使って履歴を編集し、copse のタスクを sync (`S`) して更新された upstream にリセットする。

### 8. pull request を作成

copse の外で、upstream ブランチから好みの方法（`gh pr create`、Web UI など）でプルリクエストを作成する。

必要に応じて、copse でタスクを `!` で削除し、worktree とブランチをクリーンアップする。
