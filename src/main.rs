mod app;
mod config;
mod event;
mod input;
mod layout;
mod pane;
mod render;
mod sidebar;

use std::io::{Write, stdout};

use crossterm::event::{Event, EventStream, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use app::App;
use config::Config;
use event::AppEvent;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load();
    let mut stdout = stdout();

    terminal::enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;

    let result = run(&mut stdout, &config).await;

    // クリーンアップ
    execute!(stdout, LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;

    result
}

async fn run<W: Write>(out: &mut W, config: &Config) -> anyhow::Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    let mut app = App::new(tx.clone(), config)?;
    app.render(out)?;

    // crossterm EventStreamからキー入力を読み取るタスク
    let input_tx = tx.clone();
    tokio::spawn(async move {
        let mut reader = EventStream::new();
        while let Some(Ok(event)) = reader.next().await {
            match event {
                Event::Key(key_event) => {
                    // Press イベントのみ処理（Release/Repeat無視）
                    if key_event.kind == KeyEventKind::Press {
                        let _ = input_tx.send(AppEvent::KeyInput(key_event));
                    }
                }
                Event::Resize(cols, rows) => {
                    let _ = input_tx.send(AppEvent::Resize { cols, rows });
                }
                _ => {}
            }
        }
    });

    // メインイベントループ
    while let Some(event) = rx.recv().await {
        let should_continue = app.update(event)?;
        if !should_continue {
            break;
        }
        app.render(out)?;
    }

    Ok(())
}
