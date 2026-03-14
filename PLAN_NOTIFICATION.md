# 通知機能

## 実装済み

### UDSサーバー + CLIコマンド + サイドバーインジケータ

- xmux起動時にUDSサーバーを開始（`$XDG_RUNTIME_DIR/xmux.sock` or `/tmp/xmux.sock`）
- `xmux notify --title "..." --body "..." [--window N]` で通知送信
- `--window` 未指定時は環境変数 `XMUX_WINDOW` から自動取得
- 各ペインに `XMUX_WINDOW=N`（1-indexed）環境変数を設定
- 通知が来たWindowのサイドバーに黄色の `●` 表示、Window切替でクリア
- Claude Code Hooks連携設定済み（`~/.claude/settings.json`）

## Step 3: OS通知

`xmux notify` 受信時にOS側のデスクトップ通知も出す。title/bodyをそのまま使う。

### やること

1. **OS通知の発火** (`src/app.rs` or `src/notification_server.rs`)
   - `ExternalNotification` 受信時に、サイドバーインジケータに加えてOS通知も出す
   - プラットフォーム判定:
     - Linux: `notify-send "title" "body"`
     - WSL: `powershell.exe -Command "New-BurntToastNotification -Text 'title','body'"` or `wslnotifysend`的なもの
     - macOS: `osascript -e 'display notification "body" with title "title"'`

2. **通知の重複防止**
   - アクティブWindowからの通知は出さない（画面見てるから不要）
   - 短時間に大量の通知が来た場合のスロットリングを検討

3. **設定でON/OFF**
   - `~/.config/xmux/config.toml` に `os_notification = true/false` を追加
   - デフォルトはtrueでいい？

### 制約・注意点

- `notify-send` はバックグラウンドで非同期実行（UIブロックしない）
- WSL環境ではWindows側の通知システムに橋渡しが必要
- 既存のClaude Code Hooks設定（tada.wav再生）と組み合わせて使える
