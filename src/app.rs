use crossterm::terminal;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::event::AppEvent;
use crate::input::{Action, InputHandler};
use crate::layout::{Rect, Split};
use crate::render;
use crate::sidebar::SidebarState;
use crate::window::{Direction, Window};

/// マウスドラッグ選択の状態
#[derive(Debug, Clone, Copy)]
pub struct Selection {
    pub start_col: u16,
    pub start_row: u16,
    pub end_col: u16,
    pub end_row: u16,
}

impl Selection {
    /// 正規化（start <= end）された範囲を返す
    pub fn normalized(&self) -> (u16, u16, u16, u16) {
        if self.start_row < self.end_row
            || (self.start_row == self.end_row && self.start_col <= self.end_col)
        {
            (self.start_col, self.start_row, self.end_col, self.end_row)
        } else {
            (self.end_col, self.end_row, self.start_col, self.start_row)
        }
    }
}

pub struct App {
    pub windows: Vec<Window>,
    pub active_window_idx: usize,
    pub sidebar: SidebarState,
    pub selection: Option<Selection>,
    pub toast: Option<String>,
    next_pane_id: u32,
    input_handler: InputHandler,
    event_tx: mpsc::UnboundedSender<AppEvent>,
}

impl App {
    pub fn new(event_tx: mpsc::UnboundedSender<AppEvent>, config: &Config) -> anyhow::Result<Self> {
        let (term_cols, term_rows) = terminal::size()?;
        let sidebar = SidebarState::new();
        let pane_cols = term_cols.saturating_sub(sidebar.effective_width());

        let window = Window::new(0, pane_cols, term_rows, 0, event_tx.clone())?;

        Ok(App {
            windows: vec![window],
            active_window_idx: 0,
            sidebar,
            selection: None,
            toast: None,
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
            AppEvent::MouseClick { col, row } => {
                self.selection = None;
                self.handle_mouse_click(*col, *row)?;
                return Ok(true);
            }
            AppEvent::MouseDrag { col, row } => {
                self.handle_mouse_drag(*col, *row);
                return Ok(true);
            }
            AppEvent::MouseUp { col, row } => {
                self.handle_mouse_up(*col, *row);
                return Ok(true);
            }
            AppEvent::ToastExpired => {
                self.toast = None;
                return Ok(true);
            }
            AppEvent::ExternalNotification { window, pane, .. } => {
                match window {
                    Some(win_num) => {
                        // 1-indexed → 0-indexed
                        let idx = win_num.saturating_sub(1);
                        if idx < self.windows.len() {
                            let win = &mut self.windows[idx];
                            // ペイン指定があればペイン単位で通知
                            if let Some(pane_id) = pane {
                                if let Some(p) = win.panes.get_mut(pane_id) {
                                    p.has_notification = true;
                                }
                            }
                            // サイドバー用: アクティブwindow以外ならwindow通知も立てる
                            if idx != self.active_window_idx {
                                win.has_notification = true;
                            }
                        }
                    }
                    None => {
                        for (i, window) in self.windows.iter_mut().enumerate() {
                            if i != self.active_window_idx {
                                window.has_notification = true;
                            }
                        }
                    }
                }
                return Ok(true);
            }
            _ => {}
        }

        let action = self.input_handler.handle(&event);

        match action {
            Action::ForwardToPty(data) => {
                let active_pane_id = self.active_window().active_pane_id;
                if let Some(pane) = self.active_window_mut().panes.get_mut(&active_pane_id) {
                    pane.has_notification = false;
                    pane.write_to_pty(&data)?;
                }
                // 全ペインの通知が消えたらwindow通知もクリア
                if !self
                    .active_window()
                    .panes
                    .values()
                    .any(|p| p.has_notification)
                {
                    self.active_window_mut().has_notification = false;
                }
            }
            Action::SplitVertical => {
                self.active_window_mut().zoomed_pane_id = None;
                let area = self.pane_area()?;
                self.active_window_mut().resize_all_panes(area)?;
                let id = self.alloc_pane_id();
                self.active_window_mut()
                    .split_active_pane(Split::Vertical, area, id)?;
            }
            Action::SplitHorizontal => {
                self.active_window_mut().zoomed_pane_id = None;
                let area = self.pane_area()?;
                self.active_window_mut().resize_all_panes(area)?;
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
            Action::ToggleZoom => {
                let window = self.active_window_mut();
                if window.zoomed_pane_id.is_some() {
                    window.zoomed_pane_id = None;
                    let area = self.pane_area()?;
                    self.active_window_mut().resize_all_panes(area)?;
                } else {
                    let pane_id = window.active_pane_id;
                    window.zoomed_pane_id = Some(pane_id);
                    let area = self.pane_area()?;
                    if let Some(pane) = self.active_window_mut().panes.get_mut(&pane_id) {
                        pane.resize(area.width, area.height)?;
                    }
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
        let idx = self.windows.len();
        let window = Window::new(pane_id, area.width, area.height, idx, self.event_tx.clone())?;
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

    fn handle_mouse_click(&mut self, col: u16, row: u16) -> anyhow::Result<()> {
        let sidebar_width = self.sidebar.effective_width();

        // サイドバー領域のクリック → Window切り替え
        if col < sidebar_width {
            // row=0はヘッダー、row=1は区切り線、row=2以降がWindow一覧
            if row >= 2 {
                let idx = (row - 2) as usize;
                if idx < self.windows.len() {
                    self.active_window_idx = idx;
                    let area = self.pane_area()?;
                    self.active_window_mut().resize_all_panes(area)?;
                }
            }
            return Ok(());
        }

        // ズーム中はペイン選択を無効化
        if self.active_window().zoomed_pane_id.is_some() {
            return Ok(());
        }

        // ペイン領域のクリック → ペイン選択
        let area = self.pane_area()?;
        let window = &self.windows[self.active_window_idx];
        let rects = window.layout.compute_rects(area);
        for (pane_id, rect) in &rects {
            if col >= rect.x
                && col < rect.x + rect.width
                && row >= rect.y
                && row < rect.y + rect.height
            {
                self.active_window_mut().active_pane_id = *pane_id;
                break;
            }
        }

        Ok(())
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

    fn handle_mouse_drag(&mut self, col: u16, row: u16) {
        match self.selection {
            Some(ref mut sel) => {
                sel.end_col = col;
                sel.end_row = row;
            }
            None => {
                self.selection = Some(Selection {
                    start_col: col,
                    start_row: row,
                    end_col: col,
                    end_row: row,
                });
            }
        }
    }

    fn handle_mouse_up(&mut self, col: u16, row: u16) {
        if let Some(ref mut sel) = self.selection {
            sel.end_col = col;
            sel.end_row = row;
        }
        if let Some(sel) = self.selection {
            let text = self.extract_selected_text(sel.normalized());
            if !text.is_empty() {
                copy_to_clipboard(&text);
                self.show_toast("Copied!");
            }
        }
        self.selection = None;
    }

    fn show_toast(&mut self, msg: &str) {
        self.toast = Some(msg.to_string());
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let _ = tx.send(AppEvent::ToastExpired);
        });
    }

    /// 選択範囲のテキストをペインのvt100スクリーンから抽出
    fn extract_selected_text(&self, (sc, sr, ec, er): (u16, u16, u16, u16)) -> String {
        let window = self.active_window();
        let pane_area = match self.pane_area() {
            Ok(a) => a,
            Err(_) => return String::new(),
        };

        // ズームモードならアクティブペインから直接取得
        if let Some(zoomed_id) = window.zoomed_pane_id {
            if let Some(pane) = window.panes.get(&zoomed_id) {
                return extract_text_from_pane(pane, pane_area, sc, sr, ec, er);
            }
        }

        // 選択開始位置が含まれるペインを特定
        let rects = window.layout.compute_rects(pane_area);
        for (pane_id, rect) in &rects {
            if sc >= rect.x && sc < rect.x + rect.width && sr >= rect.y && sr < rect.y + rect.height
            {
                if let Some(pane) = window.panes.get(pane_id) {
                    return extract_text_from_pane(pane, *rect, sc, sr, ec, er);
                }
            }
        }

        String::new()
    }

    pub fn render<W: std::io::Write>(&self, out: &mut W) -> anyhow::Result<()> {
        render::render(out, self)
    }
}

fn extract_text_from_pane(
    pane: &crate::pane::Pane,
    rect: Rect,
    sc: u16,
    sr: u16,
    ec: u16,
    er: u16,
) -> String {
    let screen = pane.screen();
    let mut lines = Vec::new();

    // 選択範囲をペインローカル座標にクランプ
    let clamp_col = |c: u16| c.saturating_sub(rect.x).min(pane.cols.saturating_sub(1));
    let clamp_row = |r: u16| r.saturating_sub(rect.y).min(pane.rows.saturating_sub(1));

    let r_start = clamp_row(sr);
    let r_end = clamp_row(er);

    for r in r_start..=r_end {
        let c_start = if r == r_start { clamp_col(sc) } else { 0 };
        let c_end = if r == r_end {
            clamp_col(ec)
        } else {
            pane.cols.saturating_sub(1)
        };

        let mut line = String::new();
        let mut col = c_start;
        while col <= c_end {
            if let Some(cell) = screen.cell(r, col) {
                let contents = cell.contents();
                if contents.is_empty() {
                    line.push(' ');
                } else {
                    line.push_str(&contents);
                    let w = unicode_width::UnicodeWidthStr::width(contents.as_str());
                    if w > 1 {
                        col += w as u16 - 1;
                    }
                }
            } else {
                line.push(' ');
            }
            col += 1;
        }
        lines.push(line.trim_end().to_string());
    }

    // 末尾の空行を除去
    while lines.last().map_or(false, |l| l.is_empty()) {
        lines.pop();
    }

    lines.join("\n")
}

fn copy_to_clipboard(text: &str) {
    use std::io::Write as IoWrite;
    use std::process::{Command, Stdio};

    // WSL → clip.exe, Wayland → wl-copy, X11 → xclip/xsel
    let candidates: &[&[&str]] = &[
        &["clip.exe"],
        &["wl-copy"],
        &["xclip", "-selection", "clipboard"],
        &["xsel", "--clipboard", "--input"],
    ];

    for cmd in candidates {
        if let Ok(mut child) = Command::new(cmd[0])
            .args(&cmd[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            return;
        }
    }
}
