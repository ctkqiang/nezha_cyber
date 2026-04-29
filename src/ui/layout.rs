//! 布局计算 —— 将终端区域分割为侧边栏、聊天区、输入栏等子区域。
//!
//! 所有布局计算为纯函数，不持有状态，仅根据输入参数返回 `ratatui::layout::Rect`。

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// 整体布局分割结果
pub struct AppLayout {
    pub sidebar: Option<Rect>,
    #[allow(dead_code)]
    pub main_area: Rect,
    pub tab_bar: Rect,
    pub messages: Rect,
    pub input_area: Rect,
    pub status_bar: Rect,
    pub command_palette: Option<Rect>,
}

/// 计算整体应用布局
///
/// # 参数
/// - `area`: 终端可用区域
/// - `sidebar_visible`: 侧边栏是否展开
/// - `command_palette_open`: 命令面板是否打开
/// - `sidebar_width_pct`: 侧边栏占宽度百分比（0-100）
pub fn calculate(
    area: Rect,
    sidebar_visible: bool,
    command_palette_open: bool,
    sidebar_width_pct: u16,
) -> AppLayout {
    let sidebar_width = if sidebar_visible {
        (area.width as f32 * (sidebar_width_pct as f32 / 100.0)) as u16
    } else {
        0
    };

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(if sidebar_width > 0 {
            vec![Constraint::Length(sidebar_width), Constraint::Min(40)]
        } else {
            vec![Constraint::Min(40)]
        })
        .split(area);

    let (sidebar_rect, main_rect) = if horizontal.len() == 2 {
        (Some(horizontal[0]), horizontal[1])
    } else {
        (None, horizontal[0])
    };

    let main_vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(main_rect);

    let command_palette_rect = if command_palette_open {
        let palette_width = main_rect.width.min(60);
        let palette_height = main_rect.height.min(12);
        let x = main_rect.x + (main_rect.width.saturating_sub(palette_width)) / 2;
        let y = main_rect.y + (main_rect.height.saturating_sub(palette_height)) / 3;
        Some(Rect::new(x, y, palette_width, palette_height))
    } else {
        None
    };

    AppLayout {
        sidebar: sidebar_rect,
        main_area: main_rect,
        tab_bar: main_vertical[0],
        messages: main_vertical[1],
        input_area: main_vertical[2],
        status_bar: main_vertical[3],
        command_palette: command_palette_rect,
    }
}
