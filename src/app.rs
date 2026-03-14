use std::collections::HashMap;
use std::io::Read as IoRead;

use crossterm::terminal;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::event::AppEvent;
use crate::input::{Action, InputHandler};
use crate::layout::{LayoutNode, Rect, Split};
use crate::pane::Pane;
use crate::render;
use crate::sidebar::SidebarState;

pub struct App {
    pub panes: HashMap<u32, Pane>,
    pub layout: LayoutNode,
    pub sidebar: SidebarState,
    pub active_pane_id: u32,
    pub next_pane_id: u32,
    input_handler: InputHandler,
    event_tx: mpsc::UnboundedSender<AppEvent>,
}

impl App {
    pub fn new(event_tx: mpsc::UnboundedSender<AppEvent>, config: &Config) -> anyhow::Result<Self> {
        let (term_cols, term_rows) = terminal::size()?;
        let sidebar = SidebarState::new();
        let pane_cols = term_cols.saturating_sub(sidebar.effective_width());

        let (pane, reader) = Pane::new(0, pane_cols, term_rows)?;
        let pane_id = pane.id;

        let mut panes = HashMap::new();
        panes.insert(pane_id, pane);

        let layout = LayoutNode::single(pane_id);

        let app = App {
            panes,
            layout,
            sidebar,
            active_pane_id: pane_id,
            next_pane_id: 1,
            input_handler: InputHandler::new(config),
            event_tx: event_tx.clone(),
        };

        // PTY出力の監視タスクを起動
        spawn_pty_reader(pane_id, reader, event_tx);

        Ok(app)
    }

    pub fn update(&mut self, event: AppEvent) -> anyhow::Result<bool> {
        // PTY出力イベントは先に処理
        match &event {
            AppEvent::PtyOutput { pane_id, data } => {
                if let Some(pane) = self.panes.get_mut(pane_id) {
                    pane.process_output(data);
                }
                return Ok(true);
            }
            AppEvent::PtyExit { pane_id } => {
                return self.close_pane(*pane_id);
            }
            AppEvent::Resize { cols, rows } => {
                self.handle_resize(*cols, *rows)?;
                return Ok(true);
            }
            _ => {}
        }

        let action = self.input_handler.handle(&event);

        match action {
            Action::ForwardToPty(data) => {
                if let Some(pane) = self.panes.get_mut(&self.active_pane_id) {
                    pane.write_to_pty(&data)?;
                }
            }
            Action::SplitVertical => {
                self.split_active_pane(Split::Vertical)?;
            }
            Action::SplitHorizontal => {
                self.split_active_pane(Split::Horizontal)?;
            }
            Action::FocusUp => self.move_focus(Direction::Up),
            Action::FocusDown => self.move_focus(Direction::Down),
            Action::FocusLeft => self.move_focus(Direction::Left),
            Action::FocusRight => self.move_focus(Direction::Right),
            Action::ClosePane => {
                let id = self.active_pane_id;
                return self.close_pane(id);
            }
            Action::ToggleSidebar => {
                self.sidebar.toggle();
                let (cols, rows) = terminal::size()?;
                self.handle_resize(cols, rows)?;
            }
            Action::Quit => return Ok(false),
            Action::ResizeUp | Action::ResizeDown | Action::ResizeLeft | Action::ResizeRight => {
                // 将来実装: ペインリサイズ
            }
            Action::None => {}
        }

        Ok(true)
    }

    fn split_active_pane(&mut self, direction: Split) -> anyhow::Result<()> {
        let new_id = self.next_pane_id;
        self.next_pane_id += 1;

        // 新しいペインのサイズを計算
        let (term_cols, term_rows) = terminal::size()?;
        let pane_area = Rect {
            x: self.sidebar.effective_width(),
            y: 0,
            width: term_cols.saturating_sub(self.sidebar.effective_width()),
            height: term_rows,
        };

        // レイアウトを分割
        self.layout
            .split_pane(self.active_pane_id, new_id, direction);

        // 分割後のサイズを計算
        let rects = self.layout.compute_rects(pane_area);

        // 新ペインを作成
        let new_rect = rects
            .iter()
            .find(|(id, _)| *id == new_id)
            .map(|(_, r)| *r)
            .unwrap_or(Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            });

        let (pane, reader) = Pane::new(new_id, new_rect.width, new_rect.height)?;
        self.panes.insert(new_id, pane);
        spawn_pty_reader(new_id, reader, self.event_tx.clone());

        // 既存ペインのリサイズ
        self.resize_all_panes(pane_area)?;

        self.active_pane_id = new_id;
        Ok(())
    }

    fn close_pane(&mut self, pane_id: u32) -> anyhow::Result<bool> {
        // 最後の1ペインなら終了
        if self.panes.len() <= 1 {
            return Ok(false);
        }

        if let Some(remaining_id) = self.layout.remove_pane(pane_id) {
            self.panes.remove(&pane_id);
            if self.active_pane_id == pane_id {
                self.active_pane_id = remaining_id;
            }

            let (cols, rows) = terminal::size()?;
            self.handle_resize(cols, rows)?;
        }

        Ok(true)
    }

    fn handle_resize(&mut self, cols: u16, rows: u16) -> anyhow::Result<()> {
        let pane_area = Rect {
            x: self.sidebar.effective_width(),
            y: 0,
            width: cols.saturating_sub(self.sidebar.effective_width()),
            height: rows,
        };
        self.resize_all_panes(pane_area)?;
        Ok(())
    }

    fn resize_all_panes(&mut self, pane_area: Rect) -> anyhow::Result<()> {
        let rects = self.layout.compute_rects(pane_area);
        for (pane_id, rect) in &rects {
            if let Some(pane) = self.panes.get_mut(pane_id) {
                if rect.width > 0 && rect.height > 0 {
                    pane.resize(rect.width, rect.height)?;
                }
            }
        }
        Ok(())
    }

    fn move_focus(&mut self, direction: Direction) {
        let (term_cols, term_rows) = terminal::size().unwrap_or((80, 24));
        let pane_area = Rect {
            x: self.sidebar.effective_width(),
            y: 0,
            width: term_cols.saturating_sub(self.sidebar.effective_width()),
            height: term_rows,
        };

        let rects = self.layout.compute_rects(pane_area);

        // 現在のアクティブペインの矩形を見つける
        let active_rect = match rects.iter().find(|(id, _)| *id == self.active_pane_id) {
            Some((_, r)) => *r,
            None => return,
        };

        // 方向に基づいて最も近いペインを探す
        let mut best: Option<(u32, i32)> = None;
        let active_cx = active_rect.x as i32 + active_rect.width as i32 / 2;
        let active_cy = active_rect.y as i32 + active_rect.height as i32 / 2;

        for (id, rect) in &rects {
            if *id == self.active_pane_id {
                continue;
            }
            let cx = rect.x as i32 + rect.width as i32 / 2;
            let cy = rect.y as i32 + rect.height as i32 / 2;

            let is_valid = match direction {
                Direction::Up => cy < active_cy,
                Direction::Down => cy > active_cy,
                Direction::Left => cx < active_cx,
                Direction::Right => cx > active_cx,
            };

            if is_valid {
                let dist = (cx - active_cx).abs() + (cy - active_cy).abs();
                if best.is_none() || dist < best.unwrap().1 {
                    best = Some((*id, dist));
                }
            }
        }

        if let Some((id, _)) = best {
            self.active_pane_id = id;
        }
    }

    pub fn render<W: std::io::Write>(&self, out: &mut W) -> anyhow::Result<()> {
        render::render(out, self)
    }
}

enum Direction {
    Up,
    Down,
    Left,
    Right,
}

fn spawn_pty_reader(
    pane_id: u32,
    mut reader: Box<dyn IoRead + Send>,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(AppEvent::PtyExit { pane_id });
                    break;
                }
                Ok(n) => {
                    let _ = tx.send(AppEvent::PtyOutput {
                        pane_id,
                        data: buf[..n].to_vec(),
                    });
                }
            }
        }
    });
}
