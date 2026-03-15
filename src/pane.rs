use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::Write;

pub struct Pane {
    pub name: String,
    pub pty_master: Box<dyn MasterPty + Send>,
    pub writer: Box<dyn Write + Send>,
    pub child: Box<dyn Child + Send + Sync>,
    pub parser: vt100::Parser,
    pub cols: u16,
    pub rows: u16,
    pub has_notification: bool,
}

impl Pane {
    pub fn new(
        cols: u16,
        rows: u16,
        window_index: usize,
        pane_id: u32,
    ) -> anyhow::Result<(Self, Box<dyn std::io::Read + Send>)> {
        let pty_system = native_pty_system();

        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut cmd = CommandBuilder::new(&shell);
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
        cmd.cwd(&cwd);
        cmd.env("XMUX_WINDOW", (window_index + 1).to_string()); // 1-indexed
        cmd.env("XMUX_PANE", pane_id.to_string());

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let parser = vt100::Parser::new(rows, cols, 0);

        let pane = Pane {
            name: shell.rsplit('/').next().unwrap_or("shell").to_string(),
            pty_master: pair.master,
            writer,
            child,
            parser,
            cols,
            rows,
            has_notification: false,
        };

        Ok((pane, reader))
    }

    pub fn write_to_pty(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.writer.write_all(data)?;
        Ok(())
    }

    pub fn process_output(&mut self, data: &[u8]) {
        self.parser.process(data);
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> anyhow::Result<()> {
        self.cols = cols;
        self.rows = rows;
        self.pty_master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        self.parser.set_size(rows, cols);
        Ok(())
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// PTYのフォアグラウンドプロセス名を返す
    pub fn foreground_process_name(&self) -> String {
        if let Some(pid) = self.child.process_id() {
            if let Some(name) = get_foreground_process_name(pid) {
                return name;
            }
        }
        self.name.clone()
    }
}

/// /proc/<pid>/stat からフォアグラウンドプロセスグループのコマンド名を取得
fn get_foreground_process_name(child_pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{}/stat", child_pid)).ok()?;
    // pid (comm) state ppid pgrp session tty_nr tpgid ...
    // tpgid はfield index 7 (')' の後から数えて index 5)
    let after_comm = stat.find(')')? + 2;
    let rest = &stat[after_comm..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let tpgid: u32 = fields.get(5)?.parse().ok()?;
    if tpgid == 0 {
        return None;
    }
    let comm = std::fs::read_to_string(format!("/proc/{}/comm", tpgid)).ok()?;
    Some(comm.trim().to_string())
}
