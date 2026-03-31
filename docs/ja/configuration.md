# Configuration

copse は TOML ファイルに設定を保存する。

## 設定ファイル

**パス**: `~/.config/copse/config.toml`

`--init` で設定ファイルを生成する:

```sh
copse --init
# → Created ~/.config/copse/config.toml
```

ファイルが既に存在する場合は上書きせずエラーになる。ファイルが存在しない場合、copse は全てのオプションにデフォルト値を使用する。

```toml
agent = "claudecode"
backend = "builtin"
diff_filter = "none"
auto_commit = false
auto_permissions = false
# notification_command = "osascript -e 'display notification \"Needs input\" with title \"Copse\"'"

[claudecode]
permission_mode = "default"

[color]
cursor = { bg = "236" }
cursor-blur = { fg = "252", bg = "234" }
title-focus-tasks = { fg = "black", bg = "166" }
title-focus-agent = { fg = "black", bg = "217" }
title-focus-diff = { fg = "black", bg = "33" }
title-blur = { fg = "black", bg = "240" }
title-text-focus = { fg = "252", bg = "234" }
title-text-blur = { fg = "245", bg = "234" }
title-hints = { fg = "245", bg = "234" }
search-result = { bg = "238" }
diff-add = { fg = "green" }
diff-del = { fg = "red" }
diff-chunk = { fg = "cyan" }
diff-header = { fg = "white" }
diff-context = { fg = "white" }
list-highlight = { fg = "166", bg = "234" }
list-highlight-blur = { fg = "252", bg = "234" }
list-header = { fg = "245" }

[keys.global]
focus-toggle = ["Ctrl-W"]
help = ["Ctrl-G"]

[keys.tasks]
new-task = ["n"]
move-down = ["j", "Down"]
move-up = ["k", "Up"]
open = ["a"]
show-diff = ["d", "Enter"]
merge = ["M"]
sync = ["S"]
change-upstream = ["U"]
delete = ["!"]
refresh = ["R"]
fullscreen = ["O", "Ctrl-O"]
quit = ["q", "Q"]
kill = ["Ctrl-K"]
close-children = ["Ctrl-Q"]
start-fresh = ["Ctrl-A"]

[keys.diff]
move-down = ["j", "Down"]
move-up = ["k", "Up"]
next-hunk = ["@"]
search = ["/"]
search-next = ["n"]
search-prev = ["N"]
refresh = ["R"]
fullscreen = ["O", "Ctrl-O"]
close = ["q", "Esc", "Ctrl-Q"]
page-up = ["Ctrl-B"]
page-down = ["Ctrl-F"]
half-page-up = ["Ctrl-U"]
half-page-down = ["Ctrl-D"]
add-comment = ["o"]
edit-comment = ["e"]
delete-comment = ["!"]
send-review = ["S"]
next-comment = ["c"]

[keys.agent]
fullscreen = ["Ctrl-O"]
close = ["Ctrl-Q"]
page-up = ["Ctrl-B"]
page-down = ["Ctrl-F"]
line-up = ["k"]
line-down = ["j"]
half-page-up = ["Ctrl-U"]
half-page-down = ["Ctrl-D"]
exit-scroll-mode = ["q", "Enter"]
```

## オプション

| オプション             | 型              | デフォルト     | 説明                                               |
| ---------------------- | --------------- | -------------- | -------------------------------------------------- |
| `agent`                | string          | `"claudecode"` | 使用するエージェント: `"claudecode"`               |
| `backend`              | string          | `"builtin"`    | プロセスバックエンド: `"builtin"` または `"tmux"`  |
| `diff_filter`          | string          | `"none"`       | Diff の着色方法: `"none"` または `"delta"`         |
| `auto_commit`          | bool            | `false`        | エージェントの応答ごとに変更を自動コミット         |
| `auto_permissions`     | bool            | `false`        | 安全なコマンドをエージェントで自動承認             |
| `notification_command` | string (省略可) | —              | エージェントが入力待ちになった時に実行するコマンド |

### `[claudecode]` セクション

Claude Code 固有のオプション。

| オプション        | 型     | デフォルト  | 説明                                        |
| ----------------- | ------ | ----------- | ------------------------------------------- |
| `permission_mode` | string | `"default"` | 全タスクの Claude Code パーミッションモード |

### Permission Mode

設定すると、copse は全ての `claude` 起動時に `--permission-mode <mode>` を渡す。利用可能なモード:

| モード              | 説明                                 |
| ------------------- | ------------------------------------ |
| `default`           | 通常 — ツール使用のたびに確認        |
| `acceptEdits`       | ファイル編集は自動承認、Bash は確認  |
| `plan`              | 計画のみ、実行しない                 |
| `auto`              | 編集と Bash を自動承認               |
| `bypassPermissions` | パーミッションチェックを完全スキップ |
| `dontAsk`           | 許可されていない操作を黙ってスキップ |

session 中に Claude Code 内でモードを変更することもできる（例: `/permissions`）。

### Backend

Claude Code プロセスの管理方法を制御する。

| 値        | 説明                                                                                   |
| --------- | -------------------------------------------------------------------------------------- |
| `builtin` | デフォルト — PTY で直接 claude を実行。copse 終了時にプロセスを終了する                |
| `tmux`    | tmux セッション内で claude を実行。copse 終了後もプロセスが継続する (tmux 3.0+ が必要) |

`tmux` バックエンド使用時:

- copse 終了時、Claude プロセスはバックグラウンドで継続実行される
- copse 再起動時、既存の tmux セッションを自動検出し Running として表示する
- Running (デタッチ状態) のタスクを開くと tmux セッションに再接続する
- tmux のインストールが必要。未インストールの場合、copse はエラーで終了する

copse は独自の tmux サーバー (ソケット名 `copse`) をユーザー設定なしで起動する。セッション名は `<host>/<owner>/<repo>/<task>` 形式。実行中のセッションは以下で確認できる:

```sh
tmux -L copse list-sessions
```

### Diff Filter

diff view での着色方法を制御する。

| 値      | 説明                                                                                           |
| ------- | ---------------------------------------------------------------------------------------------- |
| `none`  | デフォルト — 外部フィルターなし。tig 風のシンプルな配色（緑/赤の前景色）                       |
| `delta` | [delta](https://github.com/dandavison/delta) でシンタックスハイライト（要 delta インストール） |

delta 使用時はシンタックスハイライト、背景色による差分表示、単語レベルの差分強調が有効になる。

## カラーテーマ

`[color]` セクションで UI の色をカスタマイズできる。各エントリは `fg`（前景色）、`bg`（背景色）を指定する。指定しないフィールドはデフォルト値が使われる。

### 色の指定方法

| 形式              | 例                           | 説明                                          |
| ----------------- | ---------------------------- | --------------------------------------------- |
| 色名              | `"red"`, `"green"`, `"blue"` | 基本8色 + `"default"`（ターミナルデフォルト） |
| 256色インデックス | `"166"`, `"234"`             | `0`〜`255` の数値文字列                       |

利用可能な色名: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, `default`

### カラーエリア

| キー                  | 説明                                         |
| --------------------- | -------------------------------------------- |
| `cursor`              | フォーカス時のカーソル行                     |
| `cursor-blur`         | フォーカス喪失時のカーソル行                 |
| `title-focus-tasks`   | TASKS ステータスバー（フォーカス時）         |
| `title-focus-agent`   | AGENT ステータスバー（フォーカス時）         |
| `title-focus-diff`    | DIFF ステータスバー（フォーカス時）          |
| `title-blur`          | ステータスバー（フォーカス喪失時）           |
| `title-text-focus`    | ステータスバーのテキスト（フォーカス時）     |
| `title-text-blur`     | ステータスバーのテキスト（フォーカス喪失時） |
| `title-hints`         | ステータスバーのキーヒント                   |
| `search-result`       | 検索マッチのハイライト                       |
| `diff-add`            | Diff の追加行                                |
| `diff-del`            | Diff の削除行                                |
| `diff-chunk`          | Diff の Hunk ヘッダ                          |
| `diff-header`         | Diff のファイルヘッダ                        |
| `diff-context`        | Diff のコンテキスト行                        |
| `list-highlight`      | タスクリストの選択行（フォーカス時）         |
| `list-highlight-blur` | タスクリストの選択行（フォーカス喪失時）     |
| `list-header`         | タスクリストのヘッダー行                     |

不正な色名が指定された場合、起動時にステータスバーにワーニングが表示される。

## キーバインド

`[keys.*]` セクションでビューごとにキーバインドをカスタマイズできる。各アクションにキー文字列の配列をマッピングする。

### 上書きの挙動

指定したアクションのみが上書きされ、指定しなかったアクションはデフォルトのまま維持される。例えば `[keys.tasks]` で `move-down` のみを設定した場合、他のタスクビューのバインドはそのまま使える。

アクションに空配列 `[]` を設定するとそのアクションのキーバインドが無効になる。

ダイアログのキーバインド（確認ダイアログ、テキスト入力など）は設定対象外。

### キーの表記

| 形式               | 例                                                                                | 説明                 |
| ------------------ | --------------------------------------------------------------------------------- | -------------------- |
| 単一文字           | `"a"`, `"O"`, `"!"`, `"/"`                                                        | 小文字、大文字、記号 |
| Ctrl 組み合わせ    | `"Ctrl-O"`, `"Ctrl-W"`                                                            | Control + キー       |
| 名前付きキー       | `"Enter"`, `"Esc"`, `"Tab"`                                                       | 特殊キー             |
| 矢印キー           | `"Up"`, `"Down"`, `"Left"`, `"Right"`                                             | 矢印キー             |
| ファンクションキー | `"F1"` .. `"F12"`                                                                 | ファンクションキー   |
| その他             | `"Backspace"`, `"Delete"`, `"Space"`, `"PageUp"`, `"PageDown"`, `"Home"`, `"End"` | その他の特殊キー     |

### ビュー

| セクション      | 説明                                                               |
| --------------- | ------------------------------------------------------------------ |
| `[keys.global]` | 全ビューで有効（ビュー固有のバインドより先にチェックされる）       |
| `[keys.tasks]`  | タスクリストビュー                                                 |
| `[keys.diff]`   | Diff ビュー                                                        |
| `[keys.agent]`  | Agent (PTY) ビュー — バインドされていないキーは PTY プロセスに転送 |

### 設定例

```toml
[keys.tasks]
move-down = ["j"]       # Down 矢印キーを move-down から外す
fullscreen = ["O", "Ctrl-O", "F11"]  # F11 を追加
```

不正なキー文字列や不明なアクション名が指定された場合、起動時にステータスバーにワーニングが表示される。

## Auto-Commit

`auto_commit` が有効な場合、copse は各 worktree に Claude Code の [Stop hook](https://docs.anthropic.com/en/docs/claude-code/hooks) をインストールする。Claude の応答完了後にフックが実行され:

1. 全ての変更をステージング (`git add -A`)
2. ステージされた変更がなければスキップ (`git diff --cached --quiet`)
3. `copse auto-commit` というメッセージでコミット

Tasks view の commits ahead カウントは 5 秒ごとにリフレッシュされるため、新しいコミットが作成されると間もなく表示に反映される。

## Auto-Permissions

`auto_permissions` が有効な場合、copse は以下の安全なコマンドを事前承認し、Claude Code が確認プロンプトを出さないようにする:

| カテゴリ       | コマンド                                                                                                                                                |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Git (参照系)   | `blame`, `branch`, `cat-file`, `config`, `diff`, `log`, `ls-files`, `ls-tree`, `remote`, `rev-parse`, `shortlog`, `show`, `stash list`, `status`, `tag` |
| ディレクトリ   | `find`, `ls`, `tree`, `pwd`, `mkdir`                                                                                                                    |
| テキスト処理   | `wc`, `diff` (coreutils), `sort`, `uniq`, `cut`                                                                                                         |
| ユーティリティ | `echo`, `which`, `file`, `date`, `basename`, `dirname`                                                                                                  |
| 組み込みツール | `Edit`, `NotebookEdit`, `WebFetch`, `WebSearch`, `Write`                                                                                                |

`Edit`、`Write`、`NotebookEdit` は worktree ディレクトリ内に制限されるため、エージェントはワークスペース外のファイルを変更できない。

また、機密ファイルの読み取りはデフォルトで拒否される:

| カテゴリ             | パス                                                              |
| -------------------- | ----------------------------------------------------------------- |
| 認証情報・鍵         | `~/.ssh`, `~/.gnupg`, `~/.aws`, `~/.config/gcloud`, `~/.azure`    |
| シークレットファイル | `**/.env`, `**/.env.*`, `**/*.pem`, `**/*.key`                    |
| シェル履歴           | `~/.bash_history`, `~/.zsh_history`                               |
| 認証設定             | `~/.netrc`, `~/.docker/config.json`, `~/.kube/config`, `~/.npmrc` |

## Notification Command

設定すると、copse は各 worktree に Claude Code の [Notification hook](https://docs.anthropic.com/en/docs/claude-code/hooks) をインストールする。Claude Code がユーザの入力待ちになった時にコマンドが実行される。

キーを省略すると通知は無効になる。`notification_command = ""` はバリデーションエラーになる。

### 設定例: macOS ネイティブ通知

```toml
notification_command = "osascript -e 'display notification \"Needs input\" with title \"Copse\"'"
```

### 設定例: ターミナルベル

```toml
notification_command = "printf '\\a'"
```

### 設定例: 両方

```toml
notification_command = "printf '\\a' && osascript -e 'display notification \"Needs input\" with title \"Copse\"'"
```

### ターミナルベルについて

`printf '\a'` をコマンドに使う場合、ターミナルエミュレータにベル文字が送信される。多くのターミナルで以下の設定が可能:

- **Dock アイコンのバウンス** (macOS Terminal, iTerm2)
- **タスクバーの点滅** (Windows Terminal)
- **視覚的インジケーター** (多くの Linux ターミナル)

望む動作に応じてターミナルの通知・ベル設定を確認すること。

## 設定のマージ戦略

copse はタスク起動時に各 worktree に `.claude/settings.local.json` を書き込む。最終的な設定は以下のレイヤーを順にマージして構築される:

1. **リポジトリ設定** — リポジトリルートの `.claude/settings.local.json` (存在する場合)
2. **copse テンプレート** — 組み込みの permissions と hooks (`auto_commit` / `auto_permissions` で制御)
3. **機密パス deny ルール** — `auto_permissions` が有効な場合に自動生成

配列は結合・重複排除されるため、リポジトリ設定に追加した permissions は copse テンプレートと共に保持される。
