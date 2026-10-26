//! 布局计算 —— 将终端区域分割为侧边栏、聊天区、输入栏等子区域。
//!
//! 所有布局计算为纯函数，不持有状态，仅根据输入参数返回 `ratatui::layout::Rect`。

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use unicode_width::UnicodeWidthStr;

use crate::app::Tab;
use crate::text;

/// 侧边栏头部高度
pub const SIDEBAR_HEADER_ROWS: u16 = 3;
/// 侧边栏用量区高度
pub const SIDEBAR_USAGE_ROWS: u16 = 8;
/// 侧边栏底部说明与快捷键高度
pub const SIDEBAR_FOOTER_ROWS: u16 = 6;
/// 默认侧边栏宽度百分比
pub const SIDEBAR_WIDTH_PCT: u16 = 26;
/// 屏幕过窄时自动隐藏侧边栏，优先保证聊天区可用
pub const MIN_SIDEBAR_SCREEN_WIDTH: u16 = 76;
/// 输入框内容区最大高度（不含边框）
pub const MAX_INPUT_ROWS: u16 = 8;

/// 整体布局分割结果
pub struct AppLayout {
    pub sidebar: Option<Rect>,
    pub main_area: Rect,
    pub tab_bar: Rect,
    pub messages: Rect,
    pub input_area: Rect,
    pub status_bar: Rect,
    pub command_palette: Option<Rect>,
}

/// 计算整体应用布局（输入框固定 1 行内容）。
pub fn calculate(
    area: Rect,
    sidebar_visible: bool,
    command_palette_open: bool,
    sidebar_width_pct: u16,
) -> AppLayout {
    calculate_for(
        area,
        sidebar_visible,
        command_palette_open,
        sidebar_width_pct,
        1,
    )
}

/// 计算整体应用布局，输入框高度随内容增长。
///
/// # 参数
/// - `area`: 终端可用区域
/// - `sidebar_visible`: 侧边栏是否展开
/// - `command_palette_open`: 命令面板是否打开
/// - `sidebar_width_pct`: 侧边栏占宽度百分比（0-100）
/// - `input_content_rows`: 输入框内容行数（不含边框，1-8）
pub fn calculate_for(
    area: Rect,
    sidebar_visible: bool,
    command_palette_open: bool,
    sidebar_width_pct: u16,
    input_content_rows: u16,
) -> AppLayout {
    let sidebar_width =
        if sidebar_visible && sidebar_width_pct > 0 && area.width >= MIN_SIDEBAR_SCREEN_WIDTH {
            ((area.width as f32 * (sidebar_width_pct as f32 / 100.0)) as u16).clamp(24, 38)
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

    let content_rows = input_content_rows.clamp(1, MAX_INPUT_ROWS);
    let main_vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(content_rows + 2),
            Constraint::Length(1),
        ])
        .split(main_rect);

    let command_palette_rect = if command_palette_open {
        let palette_width = main_rect.width.min(68);
        let palette_height = main_rect.height.saturating_sub(2).min(18);
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

/// 生成标签栏中每个标签显示的文字。
///
/// 渲染层与鼠标命中测试共用此函数，保证文字宽度与点击区域一致。
pub fn tab_bar_labels(tabs: &[Tab], active: usize, width: u16) -> Vec<(usize, String)> {
    if tabs.is_empty() {
        return Vec::new();
    }

    // 标签太多时只显示一个围绕当前标签的窗口，避免挤出屏幕。
    let max_visible = ((width.saturating_sub(2)) / 12).max(1) as usize;
    let visible_count = max_visible.min(tabs.len());
    let first = active
        .saturating_sub(visible_count / 2)
        .min(tabs.len().saturating_sub(visible_count));
    let visible = &tabs[first..first + visible_count];

    let separators = 3 * visible_count.saturating_sub(1) as u16;
    let available = width.saturating_sub(2 + separators) as usize;
    let per_tab = (available / visible_count).max(8);
    let show_close = tabs.len() > 1;

    visible
        .iter()
        .enumerate()
        .map(|(offset, tab)| {
            let index = first + offset;
            let is_active = index == active;
            let close_width = if is_active && show_close { 6 } else { 4 };
            let title_width = per_tab.saturating_sub(close_width).clamp(4, 24);
            let title = text::truncate_width(&tab.title, title_width);
            let indicator = if tab.is_streaming {
                "◐"
            } else if is_active {
                "●"
            } else {
                "○"
            };
            let close = if is_active && show_close { " ✕" } else { "" };
            (index, format!(" {} {}{} ", indicator, title, close))
        })
        .collect()
}

/// 标签栏点击区域
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabHit {
    Select(usize),
    Close(usize),
}

/// 判断横坐标落在哪个标签上。
pub fn tab_bar_hit(tabs: &[Tab], active: usize, area: Rect, x: u16) -> Option<TabHit> {
    let labels = tab_bar_labels(tabs, active, area.width);
    let mut cursor = area.x + 2;

    for (index, label) in labels {
        let width = label.width() as u16;
        if x >= cursor && x < cursor + width {
            let is_active = index == active;
            if is_active && tabs.len() > 1 && x + 2 >= cursor + width {
                return Some(TabHit::Close(index));
            }
            return Some(TabHit::Select(index));
        }
        cursor = cursor.saturating_add(width + 3);
    }
    None
}

/// 侧边栏 Agent 列表可显示的行数。
pub fn agent_rows_available(sidebar: Rect) -> usize {
    sidebar
        .height
        .saturating_sub(SIDEBAR_HEADER_ROWS + SIDEBAR_USAGE_ROWS + 1 + SIDEBAR_FOOTER_ROWS)
        as usize
}

/// 计算 Agent 列表的可视窗口（start, len），保证当前 Agent 可见。
pub fn agent_window(count: usize, active: usize, rows: usize) -> (usize, usize) {
    if count == 0 || rows == 0 {
        return (0, 0);
    }
    if count <= rows {
        return (0, count);
    }
    let start = active
        .saturating_sub(rows.saturating_sub(1))
        .min(count - rows);
    (start, rows)
}

/// Agent 列表第一行在屏幕上的纵坐标。
pub fn agent_list_start(sidebar: Rect) -> u16 {
    sidebar.y + SIDEBAR_HEADER_ROWS + SIDEBAR_USAGE_ROWS + 1
}

/// 判断纵坐标落在哪个 Agent 上。
pub fn sidebar_agent_hit(
    agent_count: usize,
    active: usize,
    sidebar: Rect,
    y: u16,
) -> Option<usize> {
    let first = agent_list_start(sidebar);
    if y < first {
        return None;
    }
    let rows = agent_rows_available(sidebar);
    let (start, len) = agent_window(agent_count, active, rows);
    let offset = (y - first) as usize;
    if offset < len {
        Some(start + offset)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Tab;

    fn test_area() -> Rect {
        Rect::new(0, 0, 120, 40)
    }

    fn tabs(count: usize) -> Vec<Tab> {
        (0..count)
            .map(|i| Tab::new(i, format!("会话 {}", i + 1), "deepseek-chat", "哪吒"))
            .collect()
    }

    #[test]
    fn calculate_sidebar_visible_produces_sidebar_rect() {
        let area = test_area();
        let layout = calculate(area, true, false, 25);
        assert!(layout.sidebar.is_some());
        let sidebar = layout.sidebar.unwrap();
        assert!(sidebar.width > 0);
        assert!(sidebar.width < area.width);
    }

    #[test]
    fn calculate_sidebar_hidden_produces_none() {
        let area = test_area();
        let layout = calculate(area, false, false, 25);
        assert!(layout.sidebar.is_none());
    }

    #[test]
    fn calculate_hides_sidebar_on_narrow_screen() {
        let area = Rect::new(0, 0, 60, 24);
        let layout = calculate(area, true, false, 25);
        assert!(layout.sidebar.is_none());
    }

    #[test]
    fn calculate_main_area_fills_remaining_width() {
        let area = test_area();
        let layout = calculate(area, true, false, 25);
        let sidebar_width = area.width as f32 * 0.25;
        let expected_main = area.width - sidebar_width as u16;
        assert_eq!(layout.main_area.width, expected_main);
    }

    #[test]
    fn calculate_tab_bar_is_two_rows() {
        let area = test_area();
        let layout = calculate(area, true, false, 25);
        assert_eq!(layout.tab_bar.height, 2);
    }

    #[test]
    fn calculate_input_area_is_three_rows_for_single_line() {
        let area = test_area();
        let layout = calculate(area, true, false, 25);
        assert_eq!(layout.input_area.height, 3);
    }

    #[test]
    fn calculate_input_area_grows_with_content() {
        let area = test_area();
        let layout = calculate_for(area, true, false, 25, 4);
        assert_eq!(layout.input_area.height, 6);
    }

    #[test]
    fn calculate_input_area_is_capped() {
        let area = test_area();
        let layout = calculate_for(area, true, false, 25, 50);
        assert_eq!(layout.input_area.height, MAX_INPUT_ROWS + 2);
    }

    #[test]
    fn calculate_status_bar_is_one_row() {
        let area = test_area();
        let layout = calculate(area, true, false, 25);
        assert_eq!(layout.status_bar.height, 1);
    }

    #[test]
    fn calculate_messages_area_has_min_height() {
        let area = test_area();
        let layout = calculate(area, true, false, 25);
        assert!(layout.messages.height >= 3);
    }

    #[test]
    fn calculate_command_palette_closed_returns_none() {
        let area = test_area();
        let layout = calculate(area, true, false, 25);
        assert!(layout.command_palette.is_none());
    }

    #[test]
    fn calculate_command_palette_open_returns_rect() {
        let area = test_area();
        let layout = calculate(area, true, true, 25);
        assert!(layout.command_palette.is_some());
    }

    #[test]
    fn calculate_command_palette_width_capped_at_68() {
        let area = Rect::new(0, 0, 200, 60);
        let layout = calculate(area, false, true, 25);
        let palette = layout.command_palette.unwrap();
        assert_eq!(palette.width, 68);
    }

    #[test]
    fn calculate_command_palette_with_narrow_screen() {
        let area = Rect::new(0, 0, 80, 24);
        let layout = calculate(area, false, true, 25);
        let palette = layout.command_palette.unwrap();
        assert_eq!(palette.width, 68.min(area.width));
    }

    #[test]
    fn calculate_with_zero_sidebar_pct() {
        let area = test_area();
        let layout = calculate(area, true, false, 0);
        assert!(layout.sidebar.is_none());
    }

    #[test]
    fn tab_labels_include_all_tabs() {
        let tabs = tabs(3);
        let labels = tab_bar_labels(&tabs, 0, 120);
        assert_eq!(labels.len(), 3);
        assert!(labels[0].1.contains('●'));
        assert!(labels[1].1.contains('○'));
    }

    #[test]
    fn tab_hit_test_selects_clicked_tab() {
        let tabs = tabs(2);
        let area = Rect::new(0, 0, 120, 2);
        let labels = tab_bar_labels(&tabs, 0, 120);
        let second_start = 2 + labels[0].1.width() as u16 + 3;
        assert_eq!(
            tab_bar_hit(&tabs, 0, area, second_start + 2),
            Some(TabHit::Select(1))
        );
    }

    #[test]
    fn tab_hit_test_detects_close_button() {
        let tabs = tabs(2);
        let area = Rect::new(0, 0, 120, 2);
        let labels = tab_bar_labels(&tabs, 0, 120);
        let first_width = labels[0].1.width() as u16;
        assert_eq!(
            tab_bar_hit(&tabs, 0, area, 2 + first_width - 1),
            Some(TabHit::Close(0))
        );
    }

    #[test]
    fn agent_window_keeps_active_visible() {
        assert_eq!(agent_window(20, 15, 5), (11, 5));
        assert_eq!(agent_window(20, 0, 5), (0, 5));
        assert_eq!(agent_window(3, 1, 5), (0, 3));
    }

    #[test]
    fn sidebar_agent_hit_maps_rows() {
        let area = Rect::new(0, 0, 30, 40);
        let first = agent_list_start(area);
        assert_eq!(sidebar_agent_hit(5, 0, area, first), Some(0));
        assert_eq!(sidebar_agent_hit(5, 0, area, first + 2), Some(2));
        assert_eq!(sidebar_agent_hit(5, 0, area, first - 1), None);
    }
}
