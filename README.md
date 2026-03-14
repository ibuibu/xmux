# xmux

サイドバー付きターミナルマルチプレクサ。tmuxにサイドバーを追加したようなツール。

## インストール

```bash
cargo install --path .
```

## 使い方

```bash
xmux
```

起動すると左側にサイドバー、右側にシェルが表示される。

## キーバインド

すべての操作はプレフィックスキー `Ctrl-b` の後に入力する。

| キー | 操作 |
|------|------|
| `Ctrl-b` `%` | 垂直分割（左右） |
| `Ctrl-b` `"` | 水平分割（上下） |
| `Ctrl-b` `↑↓←→` | フォーカス移動 |
| `Ctrl-b` `Ctrl-↑↓←→` | ペインリサイズ |
| `Ctrl-b` `x` | ペインを閉じる |
| `Ctrl-b` `c` | 新しいウィンドウを作成 |
| `Ctrl-b` `1`~`9` | ウィンドウを番号で切り替え |
| `Ctrl-b` `z` | サイドバーの表示/非表示 |
| `Ctrl-b` `q` | 終了 |

マウスクリックでペインやサイドバーのウィンドウを直接選択することもできる。

## 設定

`~/.config/xmux/config.toml` で設定を変更できる。

```toml
# プレフィックスキーの変更（デフォルト: C-b）
prefix = "C-a"

# キーバインドのカスタマイズ
[bindings]
split_vertical = "%"
split_horizontal = "\""
close_pane = "x"
toggle_sidebar = "z"
new_window = "c"
quit = "q"
focus_up = "Up"
focus_down = "Down"
focus_left = "Left"
focus_right = "Right"
resize_up = "C-Up"
resize_down = "C-Down"
resize_left = "C-Left"
resize_right = "C-Right"
```

### キーの書式

| 書式 | 意味 |
|------|------|
| `C-a` | Ctrl + a |
| `A-a` | Alt + a |
| `C-Space` | Ctrl + Space |
| `Up` / `Down` / `Left` / `Right` | 矢印キー |

修飾キー: `C`/`Ctrl`, `A`/`Alt`/`M`, `S`/`Shift`

### アクション一覧

`split_vertical`, `split_horizontal`, `close_pane`, `toggle_sidebar`, `new_window`, `quit`, `focus_up`, `focus_down`, `focus_left`, `focus_right`, `resize_up`, `resize_down`, `resize_left`, `resize_right`

## サイドバー

左側のサイドバーにはウィンドウ一覧が表示される。

- `►` アクティブなウィンドウ
- フォアグラウンドプロセス名を自動表示（zsh, vim, claude 等）
- 複数ペインがある場合はペイン数を `[N]` で表示
- アクティブペインに隣接するボーダーはシアンでハイライト表示

## 開発

```bash
cargo build
cargo test
cargo run
```
