use std::path::PathBuf;
use tokio::io::AsyncBufReadExt;
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use crate::event::AppEvent;

pub fn socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("XMUX_SOCKET_PATH") {
        return PathBuf::from(path);
    }
    std::env::var("XDG_RUNTIME_DIR")
        .map(|dir| PathBuf::from(dir).join("xmux.sock"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/xmux.sock"))
}

pub fn start(tx: mpsc::UnboundedSender<AppEvent>) -> anyhow::Result<()> {
    let path = socket_path();

    // 既存のソケットファイルを削除
    let _ = std::fs::remove_file(&path);

    let listener = UnixListener::bind(&path)?;

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let reader = tokio::io::BufReader::new(stream);
                        let mut lines = reader.lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) {
                                let title = msg
                                    .get("title")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let body = msg
                                    .get("body")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let window = msg
                                    .get("window")
                                    .and_then(|v| v.as_u64())
                                    .map(|v| v as usize);
                                let _ = tx.send(AppEvent::ExternalNotification {
                                    title,
                                    body,
                                    window,
                                });
                            }
                        }
                    });
                }
                Err(_) => break,
            }
        }
    });

    Ok(())
}

pub fn cleanup() {
    let path = socket_path();
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_default() {
        // XMUX_SOCKET_PATH が未設定の場合のテスト
        unsafe { std::env::remove_var("XMUX_SOCKET_PATH") };
        let path = socket_path();
        let path_str = path.to_string_lossy();
        assert!(path_str.ends_with("xmux.sock"));
    }

    #[test]
    fn socket_path_override() {
        unsafe { std::env::set_var("XMUX_SOCKET_PATH", "/tmp/test-xmux.sock") };
        let path = socket_path();
        assert_eq!(path, PathBuf::from("/tmp/test-xmux.sock"));
        unsafe { std::env::remove_var("XMUX_SOCKET_PATH") };
    }
}
