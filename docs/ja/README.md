# copse

Claude Code のタスクを git worktree で並列実行する TUI ツール。

[tig](https://github.com/jonas/tig) にインスパイアされた設計。`claude` CLI をそのままラップし、copse はフロントエンドのみを提供する。

## コンセプト

- **Task** — 作業単位。git ブランチ (`copse/<name>`) と [git worktree](https://git-scm.com/docs/git-worktree) に対応する。各タスクは独立した `claude` プロセスを実行する。
- **Upstream** — タスクの分岐元ブランチ。git の [tracking branch](https://git-scm.com/book/en/v2/Git-Branching-Remote-Branches) として保存される。タスクを upstream にマージしたり、同期したりできる。

```
upstream branch (例: feature-x)
 ├── copse/task-a  (git worktree + tracking branch)
 ├── copse/task-b
 └── copse/task-c
```

## Preview

TODO

## 必要なもの

- [Claude Code](https://docs.anthropic.com/en/docs/claude-code)
- git リポジトリ内で実行する必要がある

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

- [設定](configuration.md) — 設定ファイル、自動コミット、自動承認
- [Git マッピング](git-mapping.md) — copse の概念と git コマンドの対応
- [ビュー](views.md) — ビューの詳細説明とキーバインド
- [設計判断](design-decisions.md) — 設計上の判断とその理由
- [English README](../../README.md)

## 仕組み

```
copse/src
 ├── main.rs      エントリーポイント、SIGTSTP 処理、パニックフック
 ├── tui.rs       Ratatui + Crossterm イベントループ
 ├── app.rs       アプリケーション状態 (タスク、モード、キー処理)
 ├── task.rs      git worktree 管理、PTY で `claude` を起動
 │                PTY 出力 → vt100 パーサー → スクリーンバッファ
 ├── diff.rs      Unified diff パーサーと検索
 ├── config.rs    設定管理 (confy, ~/.config/copse/)
 ├── event.rs     AppEvent enum (キー、タスクライフサイクル、リサイズ)
 ├── templates/
 │    └── settings.local.json   Claude Code 設定テンプレート
 └── ui/
      ├── mod.rs     レイアウト、ステータスバー、ダイアログ
      ├── list.rs    タスクリストパネル
      ├── diff.rs    Diff view レンダリング
      └── agent.rs   PseudoTerminal ウィジェット (tui-term)
```

## 開発

```sh
cargo fmt --check             # フォーマットチェック
cargo clippy -- -D warnings   # リント
cargo test                    # テスト実行
cargo build --release         # リリースビルド
```

これらのチェックは GitHub Actions により、すべての PR と `main` へのプッシュで自動実行される。

## ライセンス

[MIT](../../LICENSE)
