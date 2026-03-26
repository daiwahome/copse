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
auto_commit = false
auto_permissions = false
permission_mode = "default"
diff_filter = "auto"

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

[keys.global]
focus-toggle = ["Ctrl-W"]

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
```

## オプション

| オプション         | 型     | デフォルト  | 説明                                           |
| ------------------ | ------ | ----------- | ---------------------------------------------- |
| `auto_commit`      | bool   | `false`     | Claude の応答ごとに変更を自動コミット          |
| `auto_permissions` | bool   | `false`     | 安全なコマンドを Claude Code で自動承認        |
| `permission_mode`  | string | `"default"` | 全タスクの Claude Code パーミッションモード    |
| `diff_filter`      | string | `"auto"`    | Diff の着色方法: `"auto"`, `"delta"`, `"none"` |

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

### Diff Filter

diff view での着色方法を制御する。

| 値      | 説明                                                                       |
| ------- | -------------------------------------------------------------------------- |
| `auto`  | [delta](https://github.com/dandavison/delta) があれば使用、なければ `none` |
| `delta` | 常に delta を使用（未インストールの場合は `none` にフォールバック）        |
| `none`  | 外部フィルターなし — tig 風のシンプルな配色（緑/赤の前景色）               |

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

| カテゴリ         | コマンド                                                 |
| ---------------- | -------------------------------------------------------- |
| バージョン管理   | `git`                                                    |
| ファイル読み取り | `cat`, `head`, `tail`                                    |
| 検索             | `find`, `grep`, `rg`                                     |
| ディレクトリ     | `ls`, `tree`, `pwd`, `mkdir`                             |
| テキスト処理     | `wc`, `diff`, `sort`, `uniq`, `cut`                      |
| ユーティリティ   | `echo`, `which`, `file`, `date`, `basename`, `dirname`   |
| 組み込みツール   | `Edit`, `NotebookEdit`, `WebFetch`, `WebSearch`, `Write` |

ビルドツール (例: `cargo`, `npm`) は任意のコードを実行できるため、意図的に除外している。

## 設定のマージ戦略

copse はタスク起動時に各 worktree に `.claude/settings.local.json` を書き込む。ファイルが既に存在する場合、copse は既存のキーを保持する:

- `hooks` が既に設定されている場合、copse は上書きしない
- `permissions` が既に設定されている場合、copse は上書きしない
- 不足しているキーのみが組み込みテンプレートから補完される

つまり、worktree の設定をカスタマイズしても copse がリセットすることはない。
