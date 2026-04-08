# copse

AI コーディングエージェントのタスクを git worktree で並列実行する TUI ツール。

[tig](https://github.com/jonas/tig) にインスパイアされた設計。エージェント CLI (Claude Code, Codex) をそのままラップし、copse はフロントエンドのみを提供する。

## コンセプト

- **Task** — 作業単位。git ブランチ (`copse/<name>`) と [git worktree](https://git-scm.com/docs/git-worktree) に対応する。各タスクは独立したエージェントプロセスを実行する。
- **Upstream** — タスクの分岐元ブランチ。git の [tracking branch](https://git-scm.com/book/en/v2/Git-Branching-Remote-Branches) として保存される。タスクを upstream にマージしたり、同期したりできる。

```
upstream branch (例: feature-x)
 ├── copse/task-a  (git worktree + tracking branch)
 ├── copse/task-b
 └── copse/task-c
```

## 機能

- **並列実行** — 複数のエージェントタスクをそれぞれ独立した git worktree で同時実行
- **タスクライフサイクル** — タスクの作成・開始・停止・マージ・同期・削除をひとつの画面で操作
- **コードレビュー** — unified diff のハンク移動・検索、インラインレビューコメントをエージェントに送信
- **分割レイアウト** — Tasks + Diff、Tasks + Agent、Diff + Agent の並列表示とフルスクリーン切替
- **カスタマイズ** — TOML 設定でキーバインド、カラーテーマ、自動コミット、自動承認を管理

## Preview

![preview](../preview.gif)

## 必要なもの

- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) または [Codex CLI](https://github.com/openai/codex) (設定の `agent` で切り替え)
- git リポジトリ内で実行する必要がある

### オプション

- [delta](https://github.com/dandavison/delta) — インストールされている場合、diff view で delta によるシンタックスハイライトと単語レベルの差分強調が有効になる。delta がない場合は tig 風のシンプルな配色で表示される。
- [tmux](https://github.com/tmux/tmux) (3.0+) — バックエンドとして設定すると（設定ファイルで `backend = "tmux"`）、エージェントプロセスが tmux セッション内で実行され、copse 終了後もバックグラウンドで動作し続ける。tmux がない場合はビルトインバックエンドが使用され、copse 終了時にプロセスは終了する。詳細は[設定](configuration.md#backend)を参照。

### 推奨ターミナル

copse はキー入力の正確な処理のために [Kitty キーボードプロトコル](https://sw.kovidgoyal.net/kitty/keyboard-protocol/)を使用している。以下のターミナルを推奨:

- [Ghostty](https://ghostty.org/)
- [Kitty](https://sw.kovidgoyal.net/kitty/)
- [iTerm2](https://iterm2.com/)
- [WezTerm](https://wezfurlong.org/wezterm/)

参考: [Claude Code ターミナル設定](https://code.claude.com/docs/ja/terminal-config)

## インストール

### Homebrew (macOS)

```sh
brew tap daiwahome/copse
brew install copse
```

### ソースからビルド

[Rust ツールチェーン](https://rustup.rs/)が必要。

```sh
git clone https://github.com/daiwahome/copse.git
cd copse
cargo install --path .
```

## 使い方

git リポジトリ内で `copse` を実行:

```sh
copse
```

キーバインドやビューの詳細は [ビュー](views.md) を参照。

## ドキュメント

- [開発ワークフロー](workflow.md) — ブランチ作成から PR までのガイド
- [ビュー](views.md) — ビューの詳細説明とキーバインド
- [設定](configuration.md) — 設定ファイル、自動コミット、自動承認
- [Git マッピング](git-mapping.md) — copse の概念と git コマンドの対応
- [設計判断](design-decisions.md) — 設計上の判断とその理由
- [English README](../../README.md)

## 仕組み

```
copse/src
 ├── main.rs         エントリーポイント、SIGTSTP 処理、パニックフック
 ├── tui.rs          Ratatui + Crossterm イベントループ、suspend/resume
 ├── app.rs          アプリケーション状態 (タスク、ビュー、キー処理)
 ├── task.rs         git worktree 管理、PTY でエージェントを起動
 │                   PTY 出力 → vt100 パーサー → スクリーンバッファ
 ├── agent.rs        エージェント設定、CLAUDE.md 管理
 ├── backend.rs      プロセスバックエンド (ビルトイン PTY / tmux セッション)
 ├── diff.rs         Unified diff パーサー、検索、インラインコメント
 ├── diff_filter.rs  Diff 着色フィルター (delta 連携)
 ├── shell.rs        シェルモード (suspend / tmux ウィンドウ)
 ├── config.rs       TOML 設定管理 (~/.config/copse/config.toml)
 ├── keybind.rs      キーバインド定義と TOML オーバーライド
 ├── event.rs        AppEvent enum (キー、タスクライフサイクル、リサイズ)
 ├── theme.rs        設定からのカラーテーマ
 ├── logging.rs      ログファイル管理
 ├── templates/
 │    └── settings.local.json   Claude Code 設定テンプレート
 └── ui/
      ├── mod.rs     レイアウト、ステータスバー、ダイアログ
      ├── list.rs    タスクリストパネル
      ├── diff.rs    Diff view レンダリング
      └── agent.rs   エージェントターミナルビュー (tui-term)
```

## 既知の制限

### Agent view での日本語 IME 位置

Agent view で日本語 IME を使用すると、変換ウィンドウが正しくない位置に表示されたり、画面の再描画中に位置がずれることがある。これは ratatui の差分ベース描画とターミナルエミュレータの alternate screen モードにおける IME 位置追跡の相互作用に起因する。確定された文字は正しく入力される — 変換ウィンドウの位置のみが影響を受ける。

## 開発

```sh
cargo fmt --check             # Rust フォーマットチェック
cargo clippy -- -D warnings   # リント
cargo test                    # テスト実行
cargo build --release         # リリースビルド
dprint check                  # Markdown フォーマットチェック
```

これらのチェックは GitHub Actions により、すべての PR と `main` へのプッシュで自動実行される。

## ライセンス

[MIT](../../LICENSE)
