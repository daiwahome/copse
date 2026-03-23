# Configuration

copse は [confy](https://github.com/rust-cli/confy) が管理する TOML ファイルに設定を保存する。

## 設定ファイル

**パス**: `~/.config/copse/default-config.toml`

`--init` で設定ファイルを生成する:

```sh
copse --init
# → Created ~/.config/copse/default-config.toml
```

ファイルが既に存在する場合は上書きせずエラーになる。ファイルが存在しない場合、copse は全てのオプションにデフォルト値を使用する。

```toml
auto_commit = false
auto_permissions = false
permission_mode = "default"
```

## オプション

| オプション          | 型     | デフォルト  | 説明                                          |
|--------------------|--------|-------------|-----------------------------------------------|
| `auto_commit`      | bool   | `false`     | Claude の応答ごとに変更を自動コミット           |
| `auto_permissions` | bool   | `false`     | 安全なコマンドを Claude Code で自動承認         |
| `permission_mode`  | string | `"default"` | 全タスクの Claude Code パーミッションモード      |

### Permission Mode

設定すると、copse は全ての `claude` 起動時に `--permission-mode <mode>` を渡す。利用可能なモード:

| モード               | 説明                                                |
|---------------------|-----------------------------------------------------|
| `default`           | 通常 — ツール使用のたびに確認                         |
| `acceptEdits`       | ファイル編集は自動承認、Bash は確認                    |
| `plan`              | 計画のみ、実行しない                                  |
| `auto`              | 編集と Bash を自動承認                                |
| `bypassPermissions` | パーミッションチェックを完全スキップ                    |
| `dontAsk`           | 許可されていない操作を黙ってスキップ                   |

session 中に Claude Code 内でモードを変更することもできる（例: `/permissions`）。

## Auto-Commit

`auto_commit` が有効な場合、copse は各 worktree に Claude Code の [Stop hook](https://docs.anthropic.com/en/docs/claude-code/hooks) をインストールする。Claude の応答完了後にフックが実行され:

1. 全ての変更をステージング (`git add -A`)
2. ステージされた変更がなければスキップ (`git diff --cached --quiet`)
3. `copse auto-commit` というメッセージでコミット

Tasks view の commits ahead カウントは 5 秒ごとにリフレッシュされるため、新しいコミットが作成されると間もなく表示に反映される。

## Auto-Permissions

`auto_permissions` が有効な場合、copse は以下の安全なコマンドを事前承認し、Claude Code が確認プロンプトを出さないようにする:

| カテゴリ         | コマンド                                                 |
|-----------------|-----------------------------------------------------------|
| バージョン管理    | `git`                                                    |
| ファイル読み取り   | `cat`, `head`, `tail`                                   |
| 検索             | `find`, `grep`, `rg`                                    |
| ディレクトリ      | `ls`, `tree`, `pwd`, `mkdir`                            |
| テキスト処理      | `wc`, `diff`, `sort`, `uniq`, `cut`                     |
| ユーティリティ    | `echo`, `which`, `file`, `date`, `basename`, `dirname`  |
| 組み込みツール    | `Edit`, `NotebookEdit`, `WebFetch`, `WebSearch`, `Write` |

ビルドツール (例: `cargo`, `npm`) は任意のコードを実行できるため、意図的に除外している。

## 設定のマージ戦略

copse はタスク起動時に各 worktree に `.claude/settings.local.json` を書き込む。ファイルが既に存在する場合、copse は既存のキーを保持する:

- `hooks` が既に設定されている場合、copse は上書きしない
- `permissions` が既に設定されている場合、copse は上書きしない
- 不足しているキーのみが組み込みテンプレートから補完される

つまり、worktree の設定をカスタマイズしても copse がリセットすることはない。
