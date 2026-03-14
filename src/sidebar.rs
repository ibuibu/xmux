use crossterm::{cursor, queue, style};
use std::io::Write;

use crate::window::Window;

pub const SIDEBAR_WIDTH: u16 = 22;

pub struct SidebarState {
    pub width: u16,
    pub collapsed: bool,
}

impl SidebarState {
    pub fn new() -> Self {
        Self {
            width: SIDEBAR_WIDTH,
            collapsed: false,
        }
    }

    pub fn effective_width(&self) -> u16 {
        if self.collapsed { 0 } else { self.width }
    }

    pub fn toggle(&mut self) {
        self.collapsed = !self.collapsed;
    }

    pub fn render<W: Write>(
        &self,
        out: &mut W,
        windows: &[Window],
        active_window_idx: usize,
        terminal_height: u16,
    ) -> anyhow::Result<()> {
        if self.collapsed {
            return Ok(());
        }

        let width = self.width as usize;

        // ヘッダー
        queue!(out, cursor::MoveTo(0, 0))?;
        queue!(out, style::SetAttribute(style::Attribute::Bold))?;
        let header = format!(" {:^w$}", "xmux", w = width - 1);
        queue!(out, style::Print(&header[..header.len().min(width)]))?;
        queue!(out, style::SetAttribute(style::Attribute::Reset))?;

        // 区切り線
        queue!(out, cursor::MoveTo(0, 1))?;
        let separator: String = "─".repeat(width - 1);
        queue!(
            out,
            style::SetForegroundColor(style::Color::DarkGrey),
            style::Print(&separator),
            style::ResetColor
        )?;

        // Window一覧
        for (i, window) in windows.iter().enumerate() {
            let row = 2 + i as u16;
            if row >= terminal_height {
                break;
            }

            queue!(out, cursor::MoveTo(0, row))?;

            let is_active = i == active_window_idx;
            let marker = if is_active { "►" } else { " " };

            if is_active {
                queue!(
                    out,
                    style::SetForegroundColor(style::Color::Green),
                    style::SetAttribute(style::Attribute::Bold)
                )?;
            } else {
                queue!(out, style::SetForegroundColor(style::Color::White))?;
            }

            let name = window.display_name();
            let pane_count = window.pane_count();
            let num = i + 1; // 1-indexed

            let line = if pane_count > 1 {
                format!(" {} {} {} [{}]", marker, num, name, pane_count)
            } else {
                format!(" {} {} {}", marker, num, name)
            };

            let line = if line.len() < width - 1 {
                format!("{:<w$}", line, w = width - 1)
            } else {
                line[..width - 1].to_string()
            };
            queue!(out, style::Print(&line))?;

            queue!(
                out,
                style::SetAttribute(style::Attribute::Reset),
                style::ResetColor
            )?;
        }

        // Window一覧の後ろの行をクリア
        let list_end = 2 + windows.len() as u16;
        for row in list_end..terminal_height {
            queue!(out, cursor::MoveTo(0, row))?;
            let blank = " ".repeat(width - 1);
            queue!(out, style::Print(&blank))?;
        }

        // サイドバーの右端にボーダー描画
        let border_x = self.width - 1;
        queue!(out, style::SetForegroundColor(style::Color::DarkGrey))?;
        for row in 0..terminal_height {
            queue!(out, cursor::MoveTo(border_x, row), style::Print("│"))?;
        }
        queue!(out, style::ResetColor)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sidebar_has_default_width() {
        let sidebar = SidebarState::new();
        assert_eq!(sidebar.width, SIDEBAR_WIDTH);
        assert!(!sidebar.collapsed);
    }

    #[test]
    fn effective_width_when_expanded() {
        let sidebar = SidebarState::new();
        assert_eq!(sidebar.effective_width(), SIDEBAR_WIDTH);
    }

    #[test]
    fn effective_width_when_collapsed() {
        let mut sidebar = SidebarState::new();
        sidebar.collapsed = true;
        assert_eq!(sidebar.effective_width(), 0);
    }

    #[test]
    fn toggle_changes_collapsed_state() {
        let mut sidebar = SidebarState::new();
        assert!(!sidebar.collapsed);
        sidebar.toggle();
        assert!(sidebar.collapsed);
        sidebar.toggle();
        assert!(!sidebar.collapsed);
    }

    #[test]
    fn render_collapsed_writes_nothing() {
        let mut sidebar = SidebarState::new();
        sidebar.collapsed = true;
        let mut buf = Vec::new();
        sidebar.render(&mut buf, &[], 0, 24).unwrap();
        assert!(buf.is_empty());
    }
}
