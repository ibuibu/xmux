# xmux 実装プラン

## コンセプト

**tmux + サイドバー** = xmux
左側にサイドバー（ペイン一覧・ステータス表示）、右側にターミナルペインを配置するターミナルマルチプレクサ。

## MVP スコープ（最小限）

1. xmux起動 → サイドバー（左） + シェルペイン（右）
2. ペインの水平/垂直分割
3. ペイン間のフォーカス切り替え
4. サイドバーにペイン一覧（番号・名前・アクティブ表示・cwd）
5. ペインのリサイズ・クローズ
6. tmux風プレフィックスキー（`Ctrl-b`）

**MVPに含めないもの**: セッションdetach/attach、git連携、通知バッジ、タブ/ウィンドウ

## クレート依存

| クレート | 用途 |
|---------|------|
| `crossterm` | ターミナルI/O（入力・描画・Raw mode） |
| `tokio` | 非同期ランタイム（複数PTYストリーム処理） |
| `portable-pty` | PTY（疑似端末）管理。クロスプラットフォーム |
| `vt100` | 各ペインの仮想ターミナルエミュレーション |
| `unicode-width` | 日本語等のワイド文字幅計算 |

## モジュール構成

```
src/
├── main.rs            # エントリポイント、tokioランタイム起動
├── app.rs             # Appステート、メインイベントループ
├── event.rs           # イベント定義（キー入力、PTY出力、リサイズ等）
├── input.rs           # キー入力処理、プレフィックスキーステートマシン
├── pane.rs            # Pane構造体、PTY管理、vt100スクリーン
├── layout.rs          # レイアウトツリー（分割の再帰構造）
├── sidebar.rs         # サイドバー描画・ステート
└── render.rs          # 画面全体の描画ロジック
```

## コアデータ構造

```rust
// === pane.rs ===
struct Pane {
    id: u32,
    name: String,
    pty_master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    parser: vt100::Parser,
    cwd: PathBuf,
    cols: u16,
    rows: u16,
}

// === layout.rs ===
enum Split { Horizontal, Vertical }

enum LayoutNode {
    Leaf { pane_id: u32 },
    Split {
        direction: Split,
        ratio: f32,
        children: Vec<LayoutNode>,
    },
}

// === sidebar.rs ===
struct SidebarState {
    width: u16,
    collapsed: bool,
}

// === app.rs ===
struct App {
    panes: HashMap<u32, Pane>,
    layout: LayoutNode,
    sidebar: SidebarState,
    active_pane_id: u32,
    next_pane_id: u32,
    mode: InputMode,
}

// === event.rs ===
enum AppEvent {
    KeyInput(crossterm::event::KeyEvent),
    PtyOutput { pane_id: u32, data: Vec<u8> },
    PtyExit { pane_id: u32 },
    Resize { cols: u16, rows: u16 },
}

// === input.rs ===
enum InputMode {
    Normal,
    Prefix,
}
```

## イベントループ設計

```
┌─────────────┐     ┌──────────────┐     ┌──────────────┐
│ crossterm   │────▶│              │────▶│   render()   │
│ key events  │     │  App::update │     │  全画面再描画  │
├─────────────┤     │  (状態更新)   │     └──────────────┘
│ PTY output  │────▶│              │
│ (per pane)  │     │              │
├─────────────┤     │              │
│ terminal    │────▶│              │
│ resize      │     └──────────────┘
└─────────────┘
```

- tokio::select! で全イベントを統合
- キー入力: crossterm::event::EventStream → mpscチャンネル
- PTY出力: 各ペインのPTY readerをtokioタスクで監視 → mpscに送信
- メインループ: mpscからイベント受信 → App::update() → render()

## 画面レイアウト

```
┌──────────┬────────────────────────────┐
│ SIDEBAR  │                            │
│          │      Pane 1 (active)       │
│ ► 1 bash │                            │
│   2 vim  ├────────────────────────────┤
│   3 htop │                            │
│          │      Pane 2                │
│          │                            │
│          │                            │
└──────────┴────────────────────────────┘
  20 cols              残り全部
```

## キーバインド（MVP）

| キー | アクション |
|------|-----------|
| `Ctrl-b` | プレフィックスモードに入る |
| `Ctrl-b` → `%` | 垂直分割 |
| `Ctrl-b` → `"` | 水平分割 |
| `Ctrl-b` → `↑↓←→` | フォーカス移動 |
| `Ctrl-b` → `x` | ペインクローズ |
| `Ctrl-b` → `z` | サイドバー折りたたみトグル |
| `Ctrl-b` → `Ctrl-↑↓←→` | ペインリサイズ |

## 実装ステップ

| # | ステップ | 内容 |
|---|---------|------|
| 1 | **プロジェクト初期化** | `cargo init`、依存追加、基本構成 |
| 2 | **単一ペイン表示** | PTY起動 → raw mode → 入力転送 → 出力描画。まず1ペインでシェルが動く状態 |
| 3 | **vt100統合** | PTY出力をvt100::Parserに流し、仮想スクリーンからdiffレンダリング |
| 4 | **サイドバー描画** | 画面左にサイドバー領域を確保、ペイン一覧表示 |
| 5 | **ペイン分割** | LayoutNodeツリー実装、垂直/水平分割、各ペインのサイズ計算 |
| 6 | **フォーカス切り替え** | 方向キーでペイン間移動、サイドバーのアクティブ表示更新 |
| 7 | **リサイズ対応** | ターミナルリサイズイベント → レイアウト再計算 → PTY SIGWINCH |
| 8 | **ペインクローズ** | ペイン終了処理、レイアウトツリー再構築 |

## 将来の拡張ポイント（MVP後）

- **git branch表示**: `Pane.cwd` から `git rev-parse --abbrev-ref HEAD` 実行
- **通知バッジ**: `Pane` に `unread_count: u32` 追加
- **プロセス実行インジケータ**: PTYのフォアグラウンドプロセスを監視
- **セッションdetach/attach**: クライアント-サーバー分離（大きなリファクタ）
- **タブ（ウィンドウ）**: `App` に `windows: Vec<Window>` 追加
