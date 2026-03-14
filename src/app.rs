use crossterm::terminal;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::event::AppEvent;
use crate::input::{Action, InputHandler};
use crate::layout::{Rect, Split};
use crate::render;
use crate::sidebar::SidebarState;
use crate::window::{Direction, Window};

pub struct App {
    pub windows: Vec<Window>,
    pub active_window_idx: usize,
    pub sidebar: SidebarState,
    next_pane_id: u32,
    input_handler: InputHandler,
    event_tx: mpsc::UnboundedSender<AppEvent>,
}

impl App {
    pub fn new(event_tx: mpsc::UnboundedSender<AppEvent>, config: &Config) -> anyhow::Result<Self> {
        let (term_cols, term_rows) = terminal::size()?;
        let sidebar = SidebarState::new();
        let pane_cols = term_cols.saturating_sub(sidebar.effective_width());

        let window = Window::new(0, pane_cols, term_rows, event_tx.clone())?;

        Ok(App {
            windows: vec![window],
            active_window_idx: 0,
            sidebar,
            next_pane_id: 1,
            input_handler: InputHandler::new(config),
            event_tx,
        })
    }

    fn active_window(&self) -> &Window {
        &self.windows[self.active_window_idx]
    }

    fn active_window_mut(&mut self) -> &mut Window {
        &mut self.windows[self.active_window_idx]
    }

    fn pane_area(&self) -> anyhow::Result<Rect> {
        let (term_cols, term_rows) = terminal::size()?;
        Ok(Rect {
            x: self.sidebar.effective_width(),
            y: 0,
            width: term_cols.saturating_sub(self.sidebar.effective_width()),
            height: term_rows,
        })
    }

    fn alloc_pane_id(&mut self) -> u32 {
        let id = self.next_pane_id;
        self.next_pane_id += 1;
        id
    }

    fn find_window_for_pane(&self, pane_id: u32) -> Option<usize> {
        self.windows.iter().position(|w| w.contains_pane(pane_id))
    }

    pub fn update(&mut self, event: AppEvent) -> anyhow::Result<bool> {
        match &event {
            AppEvent::PtyOutput { pane_id, data } => {
                if let Some(idx) = self.find_window_for_pane(*pane_id) {
                    if let Some(pane) = self.windows[idx].panes.get_mut(pane_id) {
                        pane.process_output(data);
                    }
                }
                return Ok(true);
            }
            AppEvent::PtyExit { pane_id } => {
                return self.handle_pane_exit(*pane_id);
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
                let active_pane_id = self.active_window().active_pane_id;
                if let Some(pane) = self.active_window_mut().panes.get_mut(&active_pane_id) {
                    pane.write_to_pty(&data)?;
                }
            }
            Action::SplitVertical => {
                let area = self.pane_area()?;
                let id = self.alloc_pane_id();
                self.active_window_mut()
                    .split_active_pane(Split::Vertical, area, id)?;
            }
            Action::SplitHorizontal => {
                let area = self.pane_area()?;
                let id = self.alloc_pane_id();
                self.active_window_mut()
                    .split_active_pane(Split::Horizontal, area, id)?;
            }
            Action::FocusUp => {
                let area = self.pane_area()?;
                self.active_window_mut().move_focus(Direction::Up, area);
            }
            Action::FocusDown => {
                let area = self.pane_area()?;
                self.active_window_mut().move_focus(Direction::Down, area);
            }
            Action::FocusLeft => {
                let area = self.pane_area()?;
                self.active_window_mut().move_focus(Direction::Left, area);
            }
            Action::FocusRight => {
                let area = self.pane_area()?;
                self.active_window_mut().move_focus(Direction::Right, area);
            }
            Action::ClosePane => {
                let pane_id = self.active_window().active_pane_id;
                let area = self.pane_area()?;
                let window_survived = self.active_window_mut().close_pane(pane_id, area)?;
                if !window_survived {
                    // Window内の最後のペイン → Window自体を閉じる
                    self.windows.remove(self.active_window_idx);
                    if self.windows.is_empty() {
                        return Ok(false);
                    }
                    if self.active_window_idx >= self.windows.len() {
                        self.active_window_idx = self.windows.len() - 1;
                    }
                    let area = self.pane_area()?;
                    self.active_window_mut().resize_all_panes(area)?;
                }
            }
            Action::ToggleSidebar => {
                self.sidebar.toggle();
                let (cols, rows) = terminal::size()?;
                self.handle_resize(cols, rows)?;
            }
            Action::NewWindow => {
                self.create_new_window()?;
            }
            Action::SwitchWindow(idx) => {
                if idx < self.windows.len() {
                    self.active_window_idx = idx;
                    let area = self.pane_area()?;
                    self.active_window_mut().resize_all_panes(area)?;
                }
            }
            Action::Quit => return Ok(false),
            Action::ResizeUp | Action::ResizeDown | Action::ResizeLeft | Action::ResizeRight => {}
            Action::None => {}
        }

        Ok(true)
    }

    fn create_new_window(&mut self) -> anyhow::Result<()> {
        let area = self.pane_area()?;
        let pane_id = self.alloc_pane_id();
        let window = Window::new(pane_id, area.width, area.height, self.event_tx.clone())?;
        self.windows.push(window);
        self.active_window_idx = self.windows.len() - 1;
        Ok(())
    }

    fn handle_pane_exit(&mut self, pane_id: u32) -> anyhow::Result<bool> {
        let Some(win_idx) = self.find_window_for_pane(pane_id) else {
            return Ok(true);
        };

        let area = self.pane_area()?;
        let window_survived = self.windows[win_idx].close_pane(pane_id, area)?;

        if !window_survived {
            self.windows.remove(win_idx);
            if self.windows.is_empty() {
                return Ok(false);
            }
            if self.active_window_idx >= self.windows.len() {
                self.active_window_idx = self.windows.len() - 1;
            }
            let area = self.pane_area()?;
            self.active_window_mut().resize_all_panes(area)?;
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
        self.active_window_mut().resize_all_panes(pane_area)?;
        Ok(())
    }

    pub fn render<W: std::io::Write>(&self, out: &mut W) -> anyhow::Result<()> {
        render::render(out, self)
    }
}
