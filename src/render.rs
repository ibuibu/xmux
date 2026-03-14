use crossterm::{cursor, queue, style, terminal};
use std::io::Write;

use crate::app::App;
use crate::layout::{Rect, Split};

pub fn render<W: Write>(out: &mut W, app: &App) -> anyhow::Result<()> {
    let (term_cols, term_rows) = terminal::size()?;

    queue!(out, cursor::Hide, cursor::MoveTo(0, 0))?;

    // サイドバー描画
    let sidebar_width = app.sidebar.effective_width();
    app.sidebar
        .render(out, &app.windows, app.active_window_idx, term_rows)?;

    // ペイン領域の矩形を計算
    let pane_area = Rect {
        x: sidebar_width,
        y: 0,
        width: term_cols.saturating_sub(sidebar_width),
        height: term_rows,
    };

    let window = &app.windows[app.active_window_idx];

    // ズームモード: アクティブペインのみ全画面描画
    if let Some(zoomed_id) = window.zoomed_pane_id {
        if let Some(pane) = window.panes.get(&zoomed_id) {
            render_pane(out, pane, pane_area)?;
            let screen = pane.screen();
            let cursor_pos = screen.cursor_position();
            let cx = pane_area.x + cursor_pos.1;
            let cy = pane_area.y + cursor_pos.0;
            if cx < pane_area.x + pane_area.width && cy < pane_area.y + pane_area.height {
                queue!(out, cursor::MoveTo(cx, cy), cursor::Show)?;
            }
        }
        out.flush()?;
        return Ok(());
    }

    let rects = window.layout.compute_rects(pane_area);

    // 各ペインの描画
    for (pane_id, rect) in &rects {
        if let Some(pane) = window.panes.get(pane_id) {
            render_pane(out, pane, *rect)?;
        }
    }

    // ペイン間のボーダー描画（セル単位でアクティブ判定）
    let borders = window.layout.compute_borders(pane_area);
    let active_rect = rects
        .iter()
        .find(|(id, _)| *id == window.active_pane_id)
        .map(|(_, r)| *r);
    for border in &borders {
        match border.orientation {
            Split::Vertical => {
                for i in 0..border.length {
                    let by = border.y + i;
                    let is_active = active_rect
                        .map(|r| {
                            let touches_x = r.x + r.width == border.x || r.x == border.x + 1;
                            let in_y = by >= r.y && by < r.y + r.height;
                            touches_x && in_y
                        })
                        .unwrap_or(false);
                    let color = if is_active {
                        style::Color::Cyan
                    } else {
                        style::Color::DarkGrey
                    };
                    queue!(
                        out,
                        cursor::MoveTo(border.x, by),
                        style::SetForegroundColor(color),
                        style::Print("│")
                    )?;
                }
            }
            Split::Horizontal => {
                for i in 0..border.length {
                    let bx = border.x + i;
                    let is_active = active_rect
                        .map(|r| {
                            let touches_y = r.y + r.height == border.y || r.y == border.y + 1;
                            let in_x = bx >= r.x && bx < r.x + r.width;
                            touches_y && in_x
                        })
                        .unwrap_or(false);
                    let color = if is_active {
                        style::Color::Cyan
                    } else {
                        style::Color::DarkGrey
                    };
                    queue!(
                        out,
                        cursor::MoveTo(bx, border.y),
                        style::SetForegroundColor(color),
                        style::Print("─")
                    )?;
                }
            }
        }
    }
    queue!(out, style::ResetColor)?;

    // アクティブペインのカーソル位置にカーソルを表示
    if let Some(active_rect) = rects.iter().find(|(id, _)| *id == window.active_pane_id) {
        let rect = active_rect.1;
        if let Some(pane) = window.panes.get(&window.active_pane_id) {
            let screen = pane.screen();
            let cursor_pos = screen.cursor_position();
            let cx = rect.x + cursor_pos.1;
            let cy = rect.y + cursor_pos.0;
            if cx < rect.x + rect.width && cy < rect.y + rect.height {
                queue!(out, cursor::MoveTo(cx, cy), cursor::Show)?;
            }
        }
    }

    out.flush()?;
    Ok(())
}

fn render_pane<W: Write>(out: &mut W, pane: &crate::pane::Pane, rect: Rect) -> anyhow::Result<()> {
    let screen = pane.screen();

    for row in 0..rect.height {
        queue!(out, cursor::MoveTo(rect.x, rect.y + row))?;

        if row < pane.rows {
            let mut col = 0u16;
            while col < rect.width && col < pane.cols {
                let cell = screen.cell(row, col);
                match cell {
                    Some(cell) => {
                        let fg = convert_color(cell.fgcolor());
                        let bg = convert_color(cell.bgcolor());
                        queue!(out, style::SetForegroundColor(fg))?;
                        queue!(out, style::SetBackgroundColor(bg))?;

                        if cell.bold() {
                            queue!(out, style::SetAttribute(style::Attribute::Bold))?;
                        }
                        if cell.underline() {
                            queue!(out, style::SetAttribute(style::Attribute::Underlined))?;
                        }

                        let contents = cell.contents();
                        if contents.is_empty() {
                            queue!(out, style::Print(' '))?;
                        } else {
                            queue!(out, style::Print(&contents))?;
                            let width = unicode_width::UnicodeWidthStr::width(contents.as_str());
                            if width > 1 {
                                col += width as u16 - 1;
                            }
                        }

                        queue!(out, style::SetAttribute(style::Attribute::Reset))?;
                    }
                    None => {
                        queue!(out, style::Print(' '))?;
                    }
                }
                col += 1;
            }
            let remaining = rect.width.saturating_sub(col);
            if remaining > 0 {
                queue!(out, style::Print(" ".repeat(remaining as usize)))?;
            }
        } else {
            queue!(out, style::Print(" ".repeat(rect.width as usize)))?;
        }
    }

    Ok(())
}

fn convert_color(color: vt100::Color) -> style::Color {
    match color {
        vt100::Color::Default => style::Color::Reset,
        vt100::Color::Idx(i) => style::Color::AnsiValue(i),
        vt100::Color::Rgb(r, g, b) => style::Color::Rgb { r, g, b },
    }
}
