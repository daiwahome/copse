# Views

copse は [tig](https://github.com/jonas/tig) の split-pane レイアウトにインスパイアされた 2 つのメインビューを持つ。

## Tasks View

デフォルトのビュー。全タスクのステータス、upstream ブランチ、コミット進捗を表示する。

### 表示

```
 ▶ task-a  (upstream: feature-x)  running   3 ahead
 ⏸ task-b  (upstream: develop)    waiting   synced
 ■ task-c  (upstream: feature-y)  stopped   1 ahead
```

各タスクの表示要素:

| 要素                     | 説明                                                    |
|--------------------------|---------------------------------------------------------|
| アイコン (`▶` / `⏸` / `■`) | 実行中 (処理中) / 待機中 (プロンプト) / 停止             |
| 名前                     | タスク名 (ブランチ名のサフィックス: `copse/<name>`)      |
| Upstream                 | タスクの分岐元ブランチ                                   |
| ステータス               | `running` / `waiting` / `stopped`                       |
| Commits ahead            | upstream からの差分コミット数、または `synced`            |

### キーバインド

| キー             | アクション                                       |
|------------------|--------------------------------------------------|
| `j` / `↓`        | 次のタスクを選択                                 |
| `k` / `↑`        | 前のタスクを選択                                 |
| `Enter`          | Agent view を開く (実行中) / 再開 (停止中)        |
| `n`              | 新しいタスク (名前 → upstream 選択)               |
| `Ctrl-k`         | タスクを停止 (実行中のみ)                         |
| `Shift-M`        | upstream にマージ (ff / squash, 停止中のみ)       |
| `Shift-S`        | upstream から同期 (reset, 停止中のみ)             |
| `!`              | タスクを削除 (worktree + branch, 停止中のみ)      |
| `Ctrl-r`         | commits ahead を更新                              |
| `q` / `Q`        | copse を終了                                      |

### タスク作成フロー

1. `n` を押す
2. タスク名を入力 → `Enter`
3. upstream ブランチをリストから選択 (`j`/`k` → `Enter`)
4. タスクが停止状態 (`■`) で表示される
5. `Enter` を押すと worktree 内で claude が起動する

## Agent View

claude プロセスの出力を表示する。`Ctrl-]` 以外の全てのキー入力は claude に転送される。

### レイアウトモード

**Split view** (デフォルト): 左にタスクリスト、右に agent 出力。

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

### キーバインド

| キー      | アクション                                         | Overrides    |
|-----------|----------------------------------------------------|--------------|
| (TBD)     | Fullscreen に拡大 (対応予定)                        |              |
| `Ctrl-b`  | 1 ページ上にスクロール (スクロールバック)            | cursor left  |
| `Ctrl-f`  | 1 ページ下にスクロール (スクロールバック)            | cursor right |
| `Ctrl-]`  | Fullscreen → split view、Split → Tasks view に戻る |              |
| その他    | スクロール位置をリセットして Claude Code に転送      |              |

### ステータスバー

両方のビューの下部にステータスバーが表示される:

- **左**: ビューバッジ (`TASKS` または `AGENT`) + コンテキスト情報
- **右**: 使用可能なキーヒント

