# Views

copse は [tig](https://github.com/jonas/tig) の split-pane レイアウトにインスパイアされた 3 つのメインビューを持つ。

## Tasks View

デフォルトのビュー。全タスクのステータス、upstream ブランチ、コミット進捗を表示する。

### 表示

```
▶ task-a  (upstream: feature-x)  running   3 ahead
⏸ task-b  (upstream: develop)    waiting   synced
■ task-c  (upstream: feature-y)  stopped   1 ahead
```

各タスクの表示要素:

| 要素                       | 説明                                                         |
| -------------------------- | ------------------------------------------------------------ |
| アイコン (`▶` / `⏸` / `■`) | 実行中 (処理中) / 待機中 (プロンプト) / 停止                 |
| 名前                       | タスク名 (ブランチ名のサフィックス: `copse/<name>`)          |
| Upstream                   | タスクの分岐元ブランチ (upstream が存在しない場合は赤く表示) |
| ステータス                 | `running` / `waiting` / `stopped`                            |
| Commits ahead              | upstream からの差分コミット数、または `synced`               |

### キーバインド

| キー          | アクション                                   |
| ------------- | -------------------------------------------- |
| `j` / `↓`     | 次のタスクを選択                             |
| `k` / `↑`     | 前のタスクを選択                             |
| `Enter` / `d` | Diff view を開く (commits ahead > 0 の場合)  |
| `a`           | Agent view を開く (実行中) / 起動 (停止中)   |
| `Ctrl-a`      | `--continue` なしで起動 (停止中のみ)         |
| `n`           | 新しいタスク (名前 → upstream 選択)          |
| `Ctrl-k`      | タスクを停止 (実行中のみ)                    |
| `M`           | upstream にマージ (ff / squash, 停止中のみ)  |
| `S`           | upstream から同期 (reset, 停止中のみ)        |
| `U`           | upstream ブランチを変更 (停止中のみ)         |
| `!`           | タスクを削除 (worktree + branch, 停止中のみ) |
| `R`           | commits ahead を更新                         |
| `?`           | ヘルプダイアログを表示                       |
| `q` / `Q`     | copse を終了                                 |

### タスク作成フロー

1. `n` を押す
2. タスク名を入力 → `Enter`
3. upstream ブランチをリストから選択 (`j`/`k` → `Enter`)
4. タスクが停止状態 (`■`) で表示される
5. `a` を押すと worktree 内で claude が起動する

## Diff View

タスクブランチと upstream の unified diff を表示する。`git diff <upstream>..<branch>` の出力を表示する。

[delta](https://github.com/dandavison/delta) がインストールされている場合、シンタックスハイライト、背景色による差分表示、単語レベルの差分強調が有効になる。delta がない場合は tig 風のシンプルな配色（緑/赤の前景色）で表示される。

### レイアウトモード

**Split view** `[Tasks | Diff]`: 左にタスクリスト、右に diff。Tasks view から `d` で開く。

```
┌─ Tasks ──────────┬─ Diff ───────────────────────┐
│ ▶ task-a  ...    │ diff --git a/foo.rs b/foo.rs │
│ ■ task-b  ...    │ @@ -1,5 +1,7 @@              │
│                  │ +new line                     │
├──────────────────┼──────────────────────────────┤
│ TASKS status bar │ DIFF status bar              │
└──────────────────┴──────────────────────────────┘
```

**Fullscreen**: diff 出力が全画面表示。`O` で切り替え。

Agent も開いている場合、レイアウトは `[Diff | Agent]` になる: 左に diff、右に agent。

### キーバインド (Diff ペイン)

| キー      | アクション                                           |
| --------- | ---------------------------------------------------- |
| `j` / `↓` | カーソルを下に移動                                   |
| `k` / `↑` | カーソルを上に移動                                   |
| `Ctrl-b`  | 1 ページ上にスクロール                               |
| `Ctrl-f`  | 1 ページ下にスクロール                               |
| `Ctrl-u`  | 半ページ上にスクロール                               |
| `Ctrl-d`  | 半ページ下にスクロール                               |
| `/`       | 検索ダイアログを開く (パターン入力後 `Enter` で検索) |
| `n`       | 次の検索マッチ                                       |
| `N`       | 前の検索マッチ                                       |
| `@`       | 次のハンクにジャンプ (パターンを `^@@` に設定)       |
| `R`       | Diff を再取得                                        |
| `O`       | Split ↔ fullscreen 切り替え                          |
| `q`       | Diff view を閉じる                                   |
| `o`       | カーソル行にレビューコメントを追加 (インライン編集)  |
| `e`       | 既存のレビューコメントを編集                         |
| `!`       | カーソル行のレビューコメントを削除                   |
| `c`       | 次のコメントにジャンプ (以降 `n`/`N` で移動)         |
| `S`       | 全コメントを agent に送信 (プレビューダイアログ)     |
| `Ctrl-s`  | コメントを確定 (編集中)                              |
| `Esc`     | コメント編集をキャンセル                             |
| `?`       | ヘルプダイアログを表示                               |

検索パターン未設定時、`n`/`N` はハンク移動になる (`@` と同じ)。`c` を押すとコメントモードに切り替わり、`n`/`N` でコメント付き行間を移動する。

### キーバインド (Tasks ペイン、左フォーカス)

| キー      | アクション                            |
| --------- | ------------------------------------- |
| `j` / `k` | タスク選択                            |
| `Enter`   | Diff view を開く                      |
| `a`       | Agent を開く (実行中) / 起動 (停止中) |
| `O`       | Tasks fullscreen                      |
| `Ctrl-w`  | Diff ペインにフォーカス切替           |
| `q`       | Diff を閉じて Tasks fullscreen に戻る |

## Agent View

claude プロセスの出力を表示する。Agent ペインにフォーカスがある時、キー入力は claude に転送される。

### レイアウトモード

**Split view** `[Tasks | Agent]` (デフォルト): 左にタスクリスト、右に agent 出力。

```
┌─ Tasks ──────────┬─ Agent ──────────────────────┐
│ ▶ task-a  ...    │ Claude Code v2.x             │
│ ■ task-b  ...    │ > ...                        │
│                  │                              │
├──────────────────┼──────────────────────────────┤
│ TASKS status bar │ AGENT status bar             │
└──────────────────┴──────────────────────────────┘
```

**Fullscreen**: agent 出力が全画面表示。

Diff も開いている場合、レイアウトは `[Diff | Agent]` になる: 左に diff、右に agent。

### キーバインド (Agent ペイン、右フォーカス)

| キー     | アクション                                   | Overrides    |
| -------- | -------------------------------------------- | ------------ |
| `Ctrl-o` | Split ↔ fullscreen 切り替え                  |              |
| `Ctrl-q` | Agent view を閉じて Tasks または Diff に戻る |              |
| `Ctrl-w` | 左ペインにフォーカス切替                     |              |
| `Ctrl-b` | 1 ページ上にスクロール (scroll mode に入る)  | cursor left  |
| `Ctrl-f` | 1 ページ下にスクロール (scroll mode に入る)  | cursor right |
| `?`      | ヘルプダイアログを表示                       | `?` を転送   |
| その他   | Claude Code に転送                           |              |

#### Scroll mode (builtin backend)

`Ctrl-b` / `Ctrl-f` でスクロールバックすると有効になる。scroll mode 中はキーが PTY に転送されない。

| キー          | アクション         |
| ------------- | ------------------ |
| `k`           | 1 行上にスクロール |
| `j`           | 1 行下にスクロール |
| `Ctrl-u`      | 半ページ上         |
| `Ctrl-d`      | 半ページ下         |
| `Ctrl-b`      | 1 ページ上         |
| `Ctrl-f`      | 1 ページ下         |
| `q` / `Enter` | Scroll mode を終了 |

#### Copy mode (tmux backend)

tmux バックエンド使用時、`Ctrl-b` / `Ctrl-f` で tmux copy-mode に入る。copy-mode 内では標準的な tmux ナビゲーションキーが使える:

| キー          | アクション         |
| ------------- | ------------------ |
| `k`           | 1 行上にスクロール |
| `j`           | 1 行下にスクロール |
| `Ctrl-u`      | 半ページ上         |
| `Ctrl-d`      | 半ページ下         |
| `Ctrl-b`      | 1 ページ上         |
| `Ctrl-f`      | 1 ページ下         |
| `/`           | 検索               |
| `n`           | 次のマッチ         |
| `N`           | 前のマッチ         |
| `q` / `Enter` | Copy-mode を終了   |

### キーバインド (Tasks ペイン、左フォーカス)

| キー      | アクション                                        |
| --------- | ------------------------------------------------- |
| `j` / `k` | タスク選択                                        |
| `d`       | 左ペインに Diff view を表示                       |
| `a`       | Agent ペインにフォーカス (実行中) / 起動 (停止中) |
| `O`       | Tasks fullscreen                                  |
| `Ctrl-w`  | Agent ペインにフォーカス切替                      |
| `q`       | Agent を閉じて Tasks fullscreen に戻る            |

## フォーカス切替

Split view では `Ctrl-w` で左右ペインのフォーカスを切り替える。フォーカスされたペインのステータスバーバッジがハイライト表示され、非フォーカスペインのバッジはグレーになる。

`Ctrl-o` / `O` と `Ctrl-q` / `q` は同等の操作 — `Ctrl` 付きは Agent ペインで通常キーが PTY に転送されるために提供される。

## ステータスバー

各ペインの下部にステータスバーが表示される:

- **左**: ビューバッジ (`TASKS`、`AGENT`、`DIFF`) + コンテキスト情報
- **右**: 使用可能なキーヒント
