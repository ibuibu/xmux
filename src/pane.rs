use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::Write;
use std::path::PathBuf;

pub struct Pane {
    pub id: u32,
    pub name: String,
    pub pty_master: Box<dyn MasterPty + Send>,
    pub writer: Box<dyn Write + Send>,
    pub child: Box<dyn Child + Send + Sync>,
    pub parser: vt100::Parser,
    pub cwd: PathBuf,
    pub cols: u16,
    pub rows: u16,
}

impl Pane {
    pub fn new(
        id: u32,
        cols: u16,
        rows: u16,
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
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        cmd.cwd(&cwd);

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let parser = vt100::Parser::new(rows, cols, 0);

        let pane = Pane {
            id,
            name: shell.rsplit('/').next().unwrap_or("shell").to_string(),
            pty_master: pair.master,
            writer,
            child,
            parser,
            cwd,
            cols,
            rows,
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
}
