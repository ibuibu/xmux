mod app;
mod config;
mod event;
mod input;
mod layout;
mod notification_server;
mod pane;
mod render;
mod sidebar;
mod window;

use std::io::{Write, stdout};

use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind, MouseButton,
    MouseEventKind,
};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use app::App;
use config::Config;
use event::AppEvent;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // サブコマンド: xmux notify --title "..." --body "..."
    if args.get(1).map(|s| s.as_str()) == Some("notify") {
        return send_notification(&args[2..]).await;
    }

    let config = Config::load();
    let mut stdout = stdout();

    terminal::enable_raw_mode()?;
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        cursor::Hide
    )?;

    let result = run(&mut stdout, &config).await;

    execute!(
        stdout,
        cursor::Show,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal::disable_raw_mode()?;

    notification_server::cleanup();

    result
}

async fn send_notification(args: &[String]) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixStream;

    let mut title = String::new();
    let mut body = String::new();
    let mut window: Option<usize> = None;
    let mut pane: Option<u32> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--title" => {
                if i + 1 < args.len() {
                    title = args[i + 1].clone();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--body" => {
                if i + 1 < args.len() {
                    body = args[i + 1].clone();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--window" => {
                if i + 1 < args.len() {
                    window = args[i + 1].parse().ok();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--pane" => {
                if i + 1 < args.len() {
                    pane = args[i + 1].parse().ok();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    // 未指定の場合、環境変数から取得
    if window.is_none() {
        window = std::env::var("XMUX_WINDOW")
            .ok()
            .and_then(|v| v.parse().ok());
    }
    if pane.is_none() {
        pane = std::env::var("XMUX_PANE").ok().and_then(|v| v.parse().ok());
    }

    // windowが特定できない場合はxmuxの外からの呼び出しなので何もしない
    if window.is_none() {
        return Ok(());
    }

    let path = notification_server::socket_path();
    let mut stream = UnixStream::connect(&path).await?;
    let mut msg = serde_json::json!({ "title": title, "body": body });
    if let Some(w) = window {
        msg["window"] = serde_json::json!(w);
    }
    if let Some(p) = pane {
        msg["pane"] = serde_json::json!(p);
    }
    let mut line = msg.to_string();
    line.push('\n');
    stream.write_all(line.as_bytes()).await?;
    Ok(())
}

async fn run<W: Write>(out: &mut W, config: &Config) -> anyhow::Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    let mut app = App::new(tx.clone(), config)?;
    app.render(out)?;

    // UDS通知サーバー起動
    notification_server::start(tx.clone())?;

    let input_tx = tx.clone();
    tokio::spawn(async move {
        let mut reader = EventStream::new();
        while let Some(Ok(event)) = reader.next().await {
            match event {
                Event::Key(key_event) => {
                    if key_event.kind == KeyEventKind::Press {
                        let _ = input_tx.send(AppEvent::KeyInput(key_event));
                    }
                }
                Event::Mouse(mouse_event) => match mouse_event.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let _ = input_tx.send(AppEvent::MouseClick {
                            col: mouse_event.column,
                            row: mouse_event.row,
                        });
                    }
                    MouseEventKind::Drag(MouseButton::Left) => {
                        let _ = input_tx.send(AppEvent::MouseDrag {
                            col: mouse_event.column,
                            row: mouse_event.row,
                        });
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        let _ = input_tx.send(AppEvent::MouseUp {
                            col: mouse_event.column,
                            row: mouse_event.row,
                        });
                    }
                    _ => {}
                },
                Event::Resize(cols, rows) => {
                    let _ = input_tx.send(AppEvent::Resize { cols, rows });
                }
                _ => {}
            }
        }
    });

    while let Some(event) = rx.recv().await {
        let should_continue = app.update(event)?;
        if !should_continue {
            break;
        }
        app.render(out)?;
    }

    Ok(())
}
