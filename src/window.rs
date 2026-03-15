use std::collections::HashMap;
use std::io::Read as IoRead;

use tokio::sync::mpsc;

use crate::event::AppEvent;
use crate::layout::{LayoutNode, Rect, Split};
use crate::pane::Pane;

pub struct Window {
    pub panes: HashMap<u32, Pane>,
    pub layout: LayoutNode,
    pub active_pane_id: u32,
    pub zoomed_pane_id: Option<u32>,
    pub has_notification: bool,
    pub window_index: usize,
    event_tx: mpsc::UnboundedSender<AppEvent>,
}

impl Window {
    pub fn new(
        first_pane_id: u32,
        cols: u16,
        rows: u16,
        window_index: usize,
        event_tx: mpsc::UnboundedSender<AppEvent>,
    ) -> anyhow::Result<Self> {
        let (pane, reader) = Pane::new(cols, rows, window_index, first_pane_id)?;
        let mut panes = HashMap::new();
        panes.insert(first_pane_id, pane);

        spawn_pty_reader(first_pane_id, reader, event_tx.clone());

        Ok(Self {
            panes,
            layout: LayoutNode::single(first_pane_id),
            active_pane_id: first_pane_id,
            zoomed_pane_id: None,
            has_notification: false,
            window_index,
            event_tx,
        })
    }

    /// アクティブペインのフォアグラウンドプロセス名を取得
    pub fn display_name(&self) -> String {
        if let Some(pane) = self.panes.get(&self.active_pane_id) {
            pane.foreground_process_name()
        } else {
            "shell".to_string()
        }
    }

    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    pub fn contains_pane(&self, pane_id: u32) -> bool {
        self.panes.contains_key(&pane_id)
    }

    pub fn split_active_pane(
        &mut self,
        direction: Split,
        pane_area: Rect,
        new_pane_id: u32,
    ) -> anyhow::Result<()> {
        self.layout
            .split_pane(self.active_pane_id, new_pane_id, direction);

        let rects = self.layout.compute_rects(pane_area);
        let new_rect = rects
            .iter()
            .find(|(id, _)| *id == new_pane_id)
            .map(|(_, r)| *r)
            .unwrap_or(Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            });

        let (pane, reader) = Pane::new(
            new_rect.width,
            new_rect.height,
            self.window_index,
            new_pane_id,
        )?;
        self.panes.insert(new_pane_id, pane);
        spawn_pty_reader(new_pane_id, reader, self.event_tx.clone());

        self.resize_all_panes(pane_area)?;
        self.active_pane_id = new_pane_id;
        Ok(())
    }

    pub fn close_pane(&mut self, pane_id: u32, pane_area: Rect) -> anyhow::Result<bool> {
        if self.zoomed_pane_id == Some(pane_id) {
            self.zoomed_pane_id = None;
        }
        if self.panes.len() <= 1 {
            return Ok(false);
        }

        if let Some(remaining_id) = self.layout.remove_pane(pane_id) {
            self.panes.remove(&pane_id);
            if self.active_pane_id == pane_id {
                self.active_pane_id = remaining_id;
            }
            self.resize_all_panes(pane_area)?;
        }
        Ok(true)
    }

    pub fn resize_all_panes(&mut self, pane_area: Rect) -> anyhow::Result<()> {
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

    pub fn move_focus(&mut self, direction: Direction, pane_area: Rect) {
        if self.zoomed_pane_id.is_some() {
            return;
        }
        let rects = self.layout.compute_rects(pane_area);

        let active_rect = match rects.iter().find(|(id, _)| *id == self.active_pane_id) {
            Some((_, r)) => *r,
            None => return,
        };

        let mut best: Option<(u32, i32)> = None;

        for (id, rect) in &rects {
            if *id == self.active_pane_id {
                continue;
            }

            let (is_valid, distance) = match direction {
                Direction::Up => {
                    let h_overlap = rect.x < active_rect.x + active_rect.width
                        && rect.x + rect.width > active_rect.x;
                    let above = (rect.y + rect.height) <= active_rect.y;
                    (
                        h_overlap && above,
                        active_rect.y as i32 - (rect.y + rect.height) as i32,
                    )
                }
                Direction::Down => {
                    let h_overlap = rect.x < active_rect.x + active_rect.width
                        && rect.x + rect.width > active_rect.x;
                    let below = rect.y >= active_rect.y + active_rect.height;
                    (
                        h_overlap && below,
                        rect.y as i32 - (active_rect.y + active_rect.height) as i32,
                    )
                }
                Direction::Left => {
                    let v_overlap = rect.y < active_rect.y + active_rect.height
                        && rect.y + rect.height > active_rect.y;
                    let left = (rect.x + rect.width) <= active_rect.x;
                    (
                        v_overlap && left,
                        active_rect.x as i32 - (rect.x + rect.width) as i32,
                    )
                }
                Direction::Right => {
                    let v_overlap = rect.y < active_rect.y + active_rect.height
                        && rect.y + rect.height > active_rect.y;
                    let right = rect.x >= active_rect.x + active_rect.width;
                    (
                        v_overlap && right,
                        rect.x as i32 - (active_rect.x + active_rect.width) as i32,
                    )
                }
            };

            if is_valid && (best.is_none() || distance < best.unwrap().1) {
                best = Some((*id, distance));
            }
        }

        if let Some((id, _)) = best {
            self.active_pane_id = id;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
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
