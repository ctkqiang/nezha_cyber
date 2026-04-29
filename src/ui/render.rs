//! 渲染逻辑 —— 将 `App` 状态绘制到终端。
//!
//! 包含侧边栏、Tab 栏、消息列表、输入框、状态栏、命令面板的绘制函数。
//! 所有渲染函数为纯函数，接收 Frame 和 App 引用，不修改状态。

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::layout::{AppLayout, calculate};
use super::theme::Theme;
use crate::app::{App, Focus};

/// 主题对象
use crate::app::ToolCallStatus;

/// 全局渲染入口 —— 调用各子渲染函数
pub fn render(frame: &mut Frame, app: &App) {
    let theme = super::theme::get_theme(&app.theme);
    let area = frame.area();
    let layout = calculate(area, app.sidebar_visible, app.command_palette_open, 25);

    let base_style = Style::default().bg(theme.bg).fg(theme.fg);
    frame.render_widget(Block::new().style(base_style), area);

    render_sidebar(frame, app, &layout, &theme);
    render_main_area(frame, app, &layout, &theme);
    render_command_palette(frame, app, &layout, &theme);
}

/// 绘制左侧侧边栏：会话列表、Token 用量、Agent 列表
fn render_sidebar(frame: &mut Frame, app: &App, layout: &AppLayout, theme: &Theme) {
    let Some(sidebar_area) = layout.sidebar else {
        return;
    };

    let sidebar_block = Block::default()
        .borders(Borders::RIGHT)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(theme.border_color))
        .style(Style::default().bg(theme.sidebar_bg).fg(theme.sidebar_fg));

    let inner = sidebar_block.inner(sidebar_area);
    frame.render_widget(sidebar_block, sidebar_area);

    let sections = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(3),
            ratatui::layout::Constraint::Length(6),
            ratatui::layout::Constraint::Min(0),
        ])
        .split(inner);

    render_sidebar_header(frame, app, sections[0], theme);
    render_sidebar_usage(frame, app, sections[1], theme);
    render_sidebar_agents(frame, app, sections[2], theme);
}

/// 侧边栏头部：项目名称 + 快捷键提示
fn render_sidebar_header(frame: &mut Frame, _app: &App, area: Rect, theme: &Theme) {
    let lines = vec![
        Line::from(Span::styled(
            " 哪吒网络安全",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            " Ctrl+K 命令面板",
            Style::default().fg(theme.sidebar_fg),
        )),
    ];
    let para = Paragraph::new(Text::from(lines));
    frame.render_widget(para, area);
}

/// 侧边栏 Token 用量统计
fn render_sidebar_usage(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let tab = app.active_tab();
    let total_cost = tab.total_usage.cost(&app.pricing);
    let lines = vec![
        Line::from(Span::styled(
            "── Token 用量 ──",
            Style::default()
                .fg(theme.accent_dim)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("  提示: {}", tab.total_usage.prompt_tokens)),
        Line::from(format!("  生成: {}", tab.total_usage.completion_tokens)),
        Line::from(format!("  总计: {}", tab.total_usage.total_tokens)),
        Line::from(Span::styled(
            format!("  费用: ¥{:.4}", total_cost),
            Style::default().fg(theme.warning_color),
        )),
    ];
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

/// 侧边栏 Agent 列表
fn render_sidebar_agents(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        "── Agent 列表 ──",
        Style::default()
            .fg(theme.accent_dim)
            .add_modifier(Modifier::BOLD),
    ))];
    for agent in &app.agents {
        let prefix = if agent.name == app.active_tab().agent_name {
            " ▶ "
        } else {
            "   "
        };
        let style = if agent.name == app.active_tab().agent_name {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.sidebar_fg)
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, agent.name),
            style,
        )));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

/// 绘制主区域：Tab 栏 + 消息列表 + 输入框 + 状态栏
fn render_main_area(frame: &mut Frame, app: &App, layout: &AppLayout, theme: &Theme) {
    render_tab_bar(frame, app, layout.tab_bar, theme);
    render_messages(frame, app, layout.messages, theme);
    render_input_area(frame, app, layout.input_area, theme);
    render_status_bar(frame, app, layout.status_bar, theme);
}

/// Tab 栏
fn render_tab_bar(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let mut spans: Vec<Span> = Vec::new();
    for (i, tab) in app.tabs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let is_active = i == app.active_tab;
        let style = if is_active {
            Style::default()
                .bg(theme.tab_active_bg)
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().bg(theme.tab_inactive_bg).fg(theme.fg)
        };
        spans.push(Span::styled(format!(" {} ", tab.title), style));
    }

    let line = Line::from(spans);
    frame.render_widget(
        Paragraph::new(Text::from(line)).style(Style::default().bg(theme.tab_inactive_bg)),
        area,
    );
}

/// 消息列表区域
fn render_messages(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let tab = app.active_tab();
    let mut lines: Vec<Line> = Vec::new();

    for msg in &tab.messages {
        let (role_label, color) = match msg.role {
            crate::api::types::Role::User => ("你", theme.user_msg_color),
            crate::api::types::Role::Assistant => ("哪吒", theme.assistant_msg_color),
            crate::api::types::Role::System => ("系统", theme.warning_color),
            crate::api::types::Role::Tool => ("工具", theme.success_color),
        };

        lines.push(Line::from(Span::styled(
            format!("[{}]", role_label),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )));

        for content_line in msg.content.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", content_line),
                Style::default().fg(theme.fg),
            )));
        }

        if let Some(tool_calls) = &msg.tool_calls {
            for tc in tool_calls {
                let status = tab.pending_tool_calls.get(&tc.id);
                let status_str = match status {
                    Some(ToolCallStatus::Success { .. }) => "✓",
                    Some(ToolCallStatus::Failed { .. }) => "✗",
                    Some(ToolCallStatus::Running) => "…",
                    _ => "○",
                };
                lines.push(Line::from(Span::styled(
                    format!(
                        "  🔧 {} {} {}",
                        status_str, tc.function.name, tc.function.arguments
                    ),
                    Style::default().fg(theme.warning_color),
                )));
            }
        }

        lines.push(Line::from(""));
    }

    let total_lines = lines.len();
    let visible = area.height as usize;
    let max_offset = total_lines.saturating_sub(visible);
    let mut scroll = tab.scroll_offset.min(max_offset);

    if tab.auto_scroll && total_lines > visible {
        scroll = max_offset;
    }

    let messages_block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(theme.bg));

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(messages_block)
            .scroll((scroll as u16, 0)),
        area,
    );
}

/// 输入区域
fn render_input_area(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let is_focused = app.focus == Focus::ChatInput;
    let border_style = if is_focused {
        Style::default().fg(theme.accent)
    } else {
        Style::default().fg(theme.input_border)
    };

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Span::styled(
            " 输入 (Enter 发送 | Ctrl+J 换行 | @ 附件) ",
            Style::default().fg(theme.accent_dim),
        ))
        .style(Style::default().bg(theme.input_bg));

    let input_text = &app.active_tab().input_buffer;
    frame.render_widget(Paragraph::new(input_text.as_str()).block(input_block), area);
}

/// 状态栏
fn render_status_bar(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let left = Span::styled(
        format!(" {} ", app.status_message),
        Style::default().fg(theme.fg),
    );
    let right = Span::styled(
        format!(
            " 模型: {} | 主题: {} | Tab {}/{} ",
            app.active_tab().model,
            app.theme,
            app.active_tab + 1,
            app.tabs.len(),
        ),
        Style::default().fg(theme.accent_dim),
    );

    let mut line = Line::default();
    line.push_span(left);
    let left_width = UnicodeWidthStr::width(app.status_message.as_str()) + 2;
    let right_width = UnicodeWidthStr::width(
        format!(
            " 模型: {} | 主题: {} | Tab {}/{} ",
            app.active_tab().model,
            app.theme,
            app.active_tab + 1,
            app.tabs.len(),
        )
        .as_str(),
    ) + 2;

    let padding = area.width as usize;
    let space_needed = padding.saturating_sub(left_width + right_width);
    if space_needed > 0 {
        line.push_span(Span::raw(" ".repeat(space_needed)));
    }
    line.push_span(right);

    frame.render_widget(
        Paragraph::new(Text::from(line))
            .style(Style::default().bg(theme.tab_inactive_bg).fg(theme.fg)),
        area,
    );
}

/// 命令面板（Ctrl+K）
fn render_command_palette(frame: &mut Frame, app: &App, layout: &AppLayout, theme: &Theme) {
    let Some(palette_area) = layout.command_palette else {
        return;
    };

    let palette_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme.accent))
        .title(Span::styled(
            " 命令面板 ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme.input_bg));

    let inner = palette_block.inner(palette_area);

    let all_commands = vec![
        ("/model", "切换模型  —  /model deepseek-v4-pro"),
        ("/theme", "切换主题  —  /theme daylight"),
        ("/new", "新建标签页"),
        ("/close", "关闭当前标签页"),
        ("/compact", "压缩上下文"),
        ("/fork", "复制当前会话"),
        ("/export", "导出对话"),
        ("Ctrl+N", "新建标签页 (快捷键)"),
        ("Ctrl+B", "折叠侧边栏 (快捷键)"),
        ("Ctrl+K", "切换命令面板 (快捷键)"),
    ];

    let filter = app.command_palette_input.to_lowercase();
    let filtered: Vec<&(&str, &str)> = all_commands
        .iter()
        .filter(|(cmd, desc)| {
            let combined = format!("{} {}", cmd, desc).to_lowercase();
            filter.is_empty() || combined.contains(&filter)
        })
        .collect();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(inner);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(theme.accent))
        .title(Span::styled(
            " 输入命令 ",
            Style::default().fg(theme.accent),
        ));
    let input_text = format!("> {}", app.command_palette_input);
    frame.render_widget(Paragraph::new(input_text).block(input_block), chunks[0]);

    let lines: Vec<Line> = filtered
        .iter()
        .map(|(cmd, desc)| {
            Line::from(vec![
                Span::styled(
                    format!(" {}", cmd),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(desc.to_string(), Style::default().fg(theme.sidebar_fg)),
            ])
        })
        .collect();

    let list_block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(theme.input_bg));
    let text = if lines.is_empty() {
        Text::from(Line::from(Span::styled(
            " 无匹配命令",
            Style::default().fg(theme.warning_color),
        )))
    } else {
        Text::from(lines)
    };

    frame.render_widget(Paragraph::new(text).block(list_block), chunks[1]);
    frame.render_widget(palette_block, palette_area);
}
