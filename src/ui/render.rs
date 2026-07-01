//! 渲染逻辑 —— 将 `App` 状态绘制到终端。
//!
//! 包含侧边栏、Tab 栏、消息列表、输入框、状态栏、命令面板的绘制函数。
//! 所有渲染函数为纯函数，接收 Frame 和 App 引用，不修改状态。
//!
//! Claude Code 风格设计：
//! - 气泡式消息渲染，用户和助手左右分列
//! - 代码块带语法高亮和复制提示
//! - 工具调用可折叠卡片
//! - Markdown 解析（标题、列表、代码、粗体、斜体）
//! - 现代极简风格，呼吸感十足

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use super::layout::{calculate, AppLayout};
use super::theme::Theme;
use crate::api::types::Role;
use crate::app::{App, Focus, ToolCallStatus};

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let layout = calculate(area, app.sidebar_visible, app.command_palette_open, 26);
    let theme = super::theme::get_theme(&app.theme);

    if let Some(sidebar) = layout.sidebar {
        render_sidebar(frame, app, sidebar, &theme);
    }
    render_main_area(frame, app, &layout, &theme);
    render_command_palette(frame, app, &layout, &theme);
    render_confirmation_dialog(frame, app, &theme);
}

fn render_sidebar(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(theme.border_color))
        .style(Style::default().bg(theme.sidebar_bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(9),
            Constraint::Min(0),
        ])
        .split(inner);

    render_sidebar_header(frame, app, chunks[0], theme);
    render_sidebar_usage(frame, app, chunks[1], theme);
    render_sidebar_agents(frame, app, chunks[2], theme);
}

fn render_sidebar_header(frame: &mut Frame, _app: &App, area: Rect, theme: &Theme) {
    let lines = vec![
        Line::from(Span::styled(
            "  ⚡ 哪吒网络安全",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  v0.1.0 · 红队魔丸",
            Style::default().fg(theme.sidebar_fg),
        )),
    ];
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn render_sidebar_usage(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let tab = app.active_tab();
    let total_cost = tab.total_usage.cost(&app.pricing);
    let balance_str = if app.remaining_balance > 0.0 {
        format!("¥{:.2}", app.remaining_balance)
    } else {
        "查询中…".into()
    };

    let prompt_pct = if tab.total_usage.total_tokens > 0 {
        (tab.total_usage.prompt_tokens as f64 / tab.total_usage.total_tokens as f64 * 100.0)
            as usize
    } else {
        50
    };

    let lines = vec![
        Line::from(Span::styled(
            "── Token 用量 ──",
            Style::default()
                .fg(theme.accent_dim)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("  余额: ", Style::default().fg(theme.sidebar_fg)),
            Span::styled(
                &balance_str,
                Style::default()
                    .fg(theme.success_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  提示: ", Style::default().fg(theme.sidebar_fg)),
            Span::raw(format!("{}", tab.total_usage.prompt_tokens)),
        ]),
        Line::from(vec![
            Span::styled("  生成: ", Style::default().fg(theme.sidebar_fg)),
            Span::raw(format!("{}", tab.total_usage.completion_tokens)),
        ]),
        Line::from(vec![
            Span::styled("  总计: ", Style::default().fg(theme.sidebar_fg)),
            Span::raw(format!("{}", tab.total_usage.total_tokens)),
        ]),
        Line::from(vec![
            Span::styled("  费用: ", Style::default().fg(theme.sidebar_fg)),
            Span::styled(
                format!("¥{:.4}", total_cost),
                Style::default().fg(theme.warning_color),
            ),
        ]),
        Line::from(usage_bar(prompt_pct, 20, theme)),
    ];
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn usage_bar(percent: usize, width: usize, theme: &Theme) -> Vec<Span<'static>> {
    let filled = (percent * width / 100).min(width);
    let empty = width - filled;
    let mut spans = Vec::new();
    spans.push(Span::styled(" ▐", Style::default().fg(theme.border_color)));
    spans.push(Span::styled(
        "█".repeat(filled),
        Style::default().fg(theme.accent),
    ));
    spans.push(Span::styled(
        "░".repeat(empty),
        Style::default().fg(theme.border_color),
    ));
    spans.push(Span::styled("▌ ", Style::default().fg(theme.border_color)));
    spans
}

fn render_sidebar_agents(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        "── Agent 列表 ──",
        Style::default()
            .fg(theme.accent_dim)
            .add_modifier(Modifier::BOLD),
    ))];

    let agent_icons = ["🔥", "🔍", "🎯", "🎭", "💣", "⬆️ ", "🔄", "🔬", "🎖️"];

    for (i, agent) in app.agents.iter().enumerate() {
        if i >= 9 {
            break;
        }
        let is_active = agent.name == app.active_tab().agent_name;
        let icon = agent_icons.get(i).copied().unwrap_or("  ");
        let style = if is_active {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
                .bg(theme.tab_active_bg)
        } else {
            Style::default().fg(theme.sidebar_fg)
        };
        let prefix = if is_active { "▶" } else { " " };
        lines.push(Line::from(Span::styled(
            format!(" {}{} {}  ", prefix, icon, agent.name),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Ctrl+1~9 切换 Agent",
        Style::default().fg(theme.accent_dim),
    )));
    lines.push(Line::from(Span::styled(
        "  Ctrl+Tab 下一个",
        Style::default().fg(theme.accent_dim),
    )));
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn render_main_area(frame: &mut Frame, app: &App, layout: &AppLayout, theme: &Theme) {
    render_tab_bar(frame, app, layout.tab_bar, theme);
    render_messages(frame, app, layout.messages, theme);
    render_input_area(frame, app, layout.input_area, theme);
    render_status_bar(frame, app, layout.status_bar, theme);
}

fn render_tab_bar(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::raw("  "));

    for (i, tab) in app.tabs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(theme.border_color)));
        }
        let is_active = i == app.active_tab;
        let style = if is_active {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.sidebar_fg)
        };
        let indicator = if is_active { "●" } else { "○" };
        let close_btn = if app.tabs.len() > 1 { " ✕" } else { "" };
        spans.push(Span::styled(
            format!(" {} {}{} ", indicator, tab.title, close_btn),
            style,
        ));
    }

    let line = Line::from(spans);
    let bottom_border = Line::from(Span::styled(
        "─".repeat(area.width as usize),
        Style::default().fg(theme.border_color),
    ));

    frame.render_widget(
        Paragraph::new(Text::from(vec![line, bottom_border])).style(Style::default().bg(theme.bg)),
        area,
    );
}

/// 消息列表：气泡式渲染，用户消息右对齐，助手消息左对齐
fn render_messages(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let tab = app.active_tab();
    let max_width = area.width.saturating_sub(4) as usize;
    let visible = area.height as usize;
    let mut all_lines: Vec<Line<'static>> = Vec::new();
    all_lines.push(Line::from(""));

    for (idx, msg) in tab.messages.iter().enumerate() {
        let is_last = idx == tab.messages.len() - 1;
        let is_thinking = is_last && msg.role == Role::Assistant && msg.content.is_empty();

        if is_thinking {
            let dots = match tab.thinking_ticks % 4 {
                0 => "   ",
                1 => "●  ",
                2 => "●● ",
                _ => "●●●",
            };
            all_lines.push(Line::from(vec![
                Span::styled("  🤔 ", Style::default().fg(theme.accent)),
                Span::styled(
                    format!("哪吒正在思考{}", dots),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
            all_lines.push(Line::from(""));
            continue;
        }

        match msg.role {
            Role::User => render_user_message(&mut all_lines, msg, max_width, theme),
            Role::Assistant => render_assistant_message(&mut all_lines, msg, tab, max_width, theme),
            Role::System => render_system_message(&mut all_lines, msg, max_width, theme),
            Role::Tool => render_tool_result(&mut all_lines, msg, max_width, theme),
        }

        all_lines.push(Line::from(""));
    }

    let total = all_lines.len();
    let max_offset = total.saturating_sub(visible);
    let scroll = if tab.auto_scroll && total > visible {
        max_offset
    } else {
        tab.scroll_offset.min(max_offset)
    };

    let visible_lines: Vec<Line> = all_lines.into_iter().skip(scroll).take(visible).collect();

    let scroll_indicator = if total > visible {
        let pct = if max_offset > 0 {
            (scroll * 100 / max_offset).min(100)
        } else {
            0
        };
        format!(" {}% ", pct)
    } else {
        String::new()
    };

    let para = Paragraph::new(Text::from(visible_lines))
        .style(Style::default().bg(theme.bg))
        .scroll((0, 0));

    frame.render_widget(para, area);

    if !scroll_indicator.is_empty() {
        let indicator_area = Rect {
            x: area.x + area.width - 6,
            y: area.y,
            width: 5,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(scroll_indicator.as_str())
                .style(Style::default().fg(theme.accent_dim).bg(theme.bg))
                .alignment(Alignment::Right),
            indicator_area,
        );
    }
}

fn render_user_message(
    lines: &mut Vec<Line<'static>>,
    msg: &crate::api::types::Message,
    max_width: usize,
    theme: &Theme,
) {
    lines.push(Line::from(vec![
        Span::styled("  👤 ", Style::default().fg(theme.user_msg_color)),
        Span::styled(
            "你",
            Style::default()
                .fg(theme.user_msg_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    if !msg.content.is_empty() {
        let bubble_width = max_width.min(80);
        for raw_line in msg.content.lines() {
            let wrapped = wrap_text(raw_line, bubble_width, "");
            for wl in wrapped {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(wl, Style::default().fg(theme.fg)),
                ]));
            }
        }
    }
}

fn render_assistant_message(
    lines: &mut Vec<Line<'static>>,
    msg: &crate::api::types::Message,
    tab: &crate::app::Tab,
    max_width: usize,
    theme: &Theme,
) {
    lines.push(Line::from(vec![
        Span::styled("  🔥 ", Style::default().fg(theme.accent)),
        Span::styled(
            "哪吒",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    if !msg.content.is_empty() {
        render_markdown(lines, &msg.content, max_width, theme);
    }

    if let Some(tool_calls) = &msg.tool_calls {
        for tc in tool_calls {
            let status_str = match tab.pending_tool_calls.get(&tc.id) {
                Some(ToolCallStatus::Success { .. }) => "✓",
                Some(ToolCallStatus::Failed { .. }) => "✗",
                Some(ToolCallStatus::Running) => "⟳",
                _ => "…",
            };
            let status_color = match tab.pending_tool_calls.get(&tc.id) {
                Some(ToolCallStatus::Success { .. }) => theme.success_color,
                Some(ToolCallStatus::Failed { .. }) => theme.error_color,
                Some(ToolCallStatus::Running) => theme.warning_color,
                _ => theme.accent_dim,
            };

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "┌─────────────────────────────────────┐",
                    Style::default().fg(theme.border_color),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("│ ", Style::default().fg(theme.border_color)),
                Span::styled(
                    format!(" {} ", status_str),
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{}()", tc.function.name),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " ".repeat(
                        40usize
                            .saturating_sub(tc.function.name.width() + 6),
                    ),
                    Style::default().fg(theme.border_color),
                ),
                Span::styled("│", Style::default().fg(theme.border_color)),
            ]));

            if !tc.function.arguments.is_empty() {
                let args_preview = if tc.function.arguments.len() > 100 {
                    format!("{}...", &tc.function.arguments[..100])
                } else {
                    tc.function.arguments.clone()
                };
                let wrapped = wrap_text(&args_preview, 36, "  ");
                for wl in wrapped {
                    let wl_width = wl.width();
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled("│ ", Style::default().fg(theme.border_color)),
                        Span::styled(wl, Style::default().fg(theme.warning_color)),
                        Span::styled(
                            " ".repeat(40usize.saturating_sub(wl_width)),
                            Style::default().fg(theme.border_color),
                        ),
                        Span::styled("│", Style::default().fg(theme.border_color)),
                    ]));
                }
            }

            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "└─────────────────────────────────────┘",
                    Style::default().fg(theme.border_color),
                ),
            ]));

            if let Some(ToolCallStatus::Success { result }) = tab.pending_tool_calls.get(&tc.id) {
                let result_preview = if result.len() > 200 {
                    format!("{}... [共 {} 字符]", &result[..200], result.len())
                } else {
                    result.clone()
                };
                let wrapped = wrap_text(&result_preview, 60, "  ");
                for wl in wrapped {
                    lines.push(Line::from(vec![
                        Span::raw("      "),
                        Span::styled(wl, Style::default().fg(theme.success_color)),
                    ]));
                }
            }
        }
    }
}

fn render_system_message(
    lines: &mut Vec<Line<'static>>,
    msg: &crate::api::types::Message,
    max_width: usize,
    theme: &Theme,
) {
    lines.push(Line::from(vec![
        Span::styled("  ⚠ ", Style::default().fg(theme.warning_color)),
        Span::styled(
            "系统",
            Style::default()
                .fg(theme.warning_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    if !msg.content.is_empty() {
        let wrapped = wrap_text(&msg.content, max_width, "");
        for wl in wrapped {
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(wl, Style::default().fg(theme.warning_color)),
            ]));
        }
    }
}

fn render_tool_result(
    lines: &mut Vec<Line<'static>>,
    msg: &crate::api::types::Message,
    max_width: usize,
    theme: &Theme,
) {
    lines.push(Line::from(vec![
        Span::styled("  ✓ ", Style::default().fg(theme.success_color)),
        Span::styled(
            "工具结果",
            Style::default()
                .fg(theme.success_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    if !msg.content.is_empty() {
        let preview = if msg.content.len() > 500 {
            format!(
                "{}... [已截断，共 {} 字符]",
                &msg.content[..500],
                msg.content.len()
            )
        } else {
            msg.content.clone()
        };
        let wrapped = wrap_text(&preview, max_width, "  ");
        for wl in wrapped {
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(wl, Style::default().fg(theme.sidebar_fg)),
            ]));
        }
    }
}

/// 简易 Markdown 渲染：支持标题、粗体、代码、列表、引用
fn render_markdown(lines: &mut Vec<Line<'static>>, content: &str, max_width: usize, theme: &Theme) {
    let mut in_code_block = false;

    for line in content.lines() {
        if line.starts_with("```") {
            if !in_code_block {
                in_code_block = true;
                let code_lang = line.trim_start_matches('`').trim();
                let lang_label = if code_lang.is_empty() {
                    " code ".into()
                } else {
                    format!(" {} ", code_lang)
                };
                let code_lang_len = code_lang.len();
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled("╭──", Style::default().fg(theme.border_color)),
                    Span::styled(
                        lang_label,
                        Style::default()
                            .fg(theme.accent)
                            .bg(theme.border_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "─".repeat(max_width.saturating_sub(code_lang_len + 12)),
                        Style::default().fg(theme.border_color),
                    ),
                    Span::styled("╮", Style::default().fg(theme.border_color)),
                ]));
            } else {
                in_code_block = false;
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        "╰".to_string() + &"─".repeat(max_width.saturating_sub(6)) + "╯",
                        Style::default().fg(theme.border_color),
                    ),
                ]));
            }
            continue;
        }

        if in_code_block {
            let line_owned = line.to_string();
            let line_width = line_owned.width();
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("│ ", Style::default().fg(theme.border_color)),
                Span::styled(line_owned, Style::default().fg(theme.warning_color)),
                Span::styled(
                    " ".repeat(max_width.saturating_sub(line_width + 4)),
                    Style::default().fg(theme.border_color),
                ),
                Span::styled("│", Style::default().fg(theme.border_color)),
            ]));
            continue;
        }

        if line.starts_with("# ") {
            let text = line.trim_start_matches("# ").to_string();
            let text_width = text.width();
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    text,
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    "─".repeat(text_width.min(max_width)),
                    Style::default().fg(theme.accent_dim),
                ),
            ]));
            continue;
        }

        if line.starts_with("## ") {
            let text = line.trim_start_matches("## ").to_string();
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    text,
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            continue;
        }

        if line.starts_with("- ") || line.starts_with("* ") {
            let text = line[2..].to_string();
            let wrapped = wrap_text(&text, max_width.saturating_sub(4), "");
            for (i, wl) in wrapped.iter().enumerate() {
                let bullet = if i == 0 { "•" } else { " " };
                lines.push(Line::from(vec![
                    Span::raw("      "),
                    Span::styled(bullet, Style::default().fg(theme.accent)),
                    Span::raw(" "),
                    Span::styled(wl.clone(), Style::default().fg(theme.fg)),
                ]));
            }
            continue;
        }

        if line.starts_with("> ") {
            let text = line.trim_start_matches("> ").to_string();
            let wrapped = wrap_text(&text, max_width.saturating_sub(4), "");
            for wl in wrapped {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled("┃ ", Style::default().fg(theme.accent_dim)),
                    Span::styled(
                        wl,
                        Style::default()
                            .fg(theme.sidebar_fg)
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]));
            }
            continue;
        }

        if line.is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        let wrapped = wrap_text(line, max_width, "");
        for wl in wrapped {
            let processed = render_inline_markdown(&wl, theme);
            let mut line_spans = vec![Span::raw("    ")];
            line_spans.extend(processed);
            lines.push(Line::from(line_spans));
        }
    }
}

/// 行内 Markdown：**粗体**、`代码`
fn render_inline_markdown(text: &str, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if let Some(start) = remaining.find("**") {
            if start > 0 {
                spans.push(Span::styled(
                    remaining[..start].to_string(),
                    Style::default().fg(theme.fg),
                ));
            }
            let after_start = &remaining[start + 2..];
            if let Some(end) = after_start.find("**") {
                spans.push(Span::styled(
                    after_start[..end].to_string(),
                    Style::default()
                        .fg(theme.fg)
                        .add_modifier(Modifier::BOLD),
                ));
                remaining = &after_start[end + 2..];
            } else {
                spans.push(Span::styled(
                    "**".to_string(),
                    Style::default().fg(theme.fg),
                ));
                remaining = after_start;
            }
            continue;
        }

        if let Some(start) = remaining.find('`') {
            if start > 0 {
                spans.push(Span::styled(
                    remaining[..start].to_string(),
                    Style::default().fg(theme.fg),
                ));
            }
            let after_start = &remaining[start + 1..];
            if let Some(end) = after_start.find('`') {
                spans.push(Span::styled(
                    format!(" {} ", &after_start[..end]),
                    Style::default().fg(theme.warning_color).bg(theme.border_color),
                ));
                remaining = &after_start[end + 1..];
            } else {
                spans.push(Span::styled("`".to_string(), Style::default().fg(theme.fg)));
                remaining = after_start;
            }
            continue;
        }

        spans.push(Span::styled(
            remaining.to_string(),
            Style::default().fg(theme.fg),
        ));
        break;
    }

    spans
}

fn render_input_area(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let is_focused = app.focus == Focus::ChatInput;
    let border_color = if is_focused { theme.accent } else { theme.border_color };

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            " 💬 输入消息 ",
            Style::default().fg(theme.accent),
        ))
        .title_bottom(Span::styled(
            " Enter 发送 · Ctrl+J 换行 · @ 附件 · Ctrl+K 命令 ",
            Style::default().fg(theme.accent_dim),
        ))
        .style(Style::default().bg(theme.input_bg));

    let inner = input_block.inner(area);

    let input_text = &app.active_tab().input_buffer;

    let mut input_lines: Vec<Line<'static>> = Vec::new();
    for (i, line_text) in input_text.lines().enumerate() {
        let line_num = format!(" {:>2} ", i + 1);
        let mut spans = vec![Span::styled(
            line_num,
            Style::default().fg(theme.accent_dim).bg(theme.sidebar_bg),
        )];
        spans.push(Span::raw(format!(" {}", line_text)));
        input_lines.push(Line::from(spans));
    }

    if input_lines.is_empty() {
        input_lines.push(Line::from(vec![
            Span::styled("  1 ", Style::default().fg(theme.accent_dim).bg(theme.sidebar_bg)),
            Span::raw(" "),
        ]));
    }

    frame.render_widget(input_block, area);
    frame.render_widget(
        Paragraph::new(Text::from(input_lines))
            .style(Style::default().bg(theme.input_bg)),
        inner,
    );

    if is_focused {
        let input_len = app.active_tab().input_buffer.len();
        let cursor_x = inner.x + 4 + (input_len % (inner.width as usize - 4)) as u16;
        let cursor_y = inner.y + (input_len / (inner.width as usize - 4)) as u16;
        if cursor_y < area.y + area.height && cursor_x < area.x + area.width {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let tab = app.active_tab();
    let agent_icon = match app.agents.iter().position(|a| a.name == tab.agent_name) {
        Some(0) => "🔥",
        Some(1) => "🔍",
        Some(2) => "🎯",
        Some(3) => "🎭",
        Some(4) => "💣",
        Some(5) => "⬆️ ",
        Some(6) => "🔄",
        Some(7) => "🔬",
        Some(8) => "🎖️",
        _ => "✨",
    };

    let left = vec![
        Span::styled(" ", Style::default().bg(theme.tab_inactive_bg)),
        Span::styled(
            format!("{} {}", agent_icon, tab.agent_name),
            Style::default()
                .fg(theme.accent)
                .bg(theme.tab_inactive_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default().bg(theme.tab_inactive_bg)),
        Span::styled(
            format!("{} 消息", tab.messages.len()),
            Style::default().fg(theme.fg).bg(theme.tab_inactive_bg),
        ),
        Span::styled(
            "  │  ",
            Style::default()
                .fg(theme.border_color)
                .bg(theme.tab_inactive_bg),
        ),
        Span::styled(
            &app.status_message,
            Style::default()
                .fg(theme.sidebar_fg)
                .bg(theme.tab_inactive_bg),
        ),
    ];

    let right = vec![
        Span::styled(
            format!(
                "{:.2}k tokens ",
                tab.total_usage.total_tokens as f64 / 1000.0
            ),
            Style::default()
                .fg(theme.accent_dim)
                .bg(theme.tab_inactive_bg),
        ),
        Span::styled(
            "│",
            Style::default()
                .fg(theme.border_color)
                .bg(theme.tab_inactive_bg),
        ),
        Span::styled(
            format!(" {} ", tab.model),
            Style::default().fg(theme.accent).bg(theme.tab_inactive_bg),
        ),
        Span::styled(
            "│",
            Style::default()
                .fg(theme.border_color)
                .bg(theme.tab_inactive_bg),
        ),
        Span::styled(
            format!(" {} 主题 ", app.theme),
            Style::default()
                .fg(theme.sidebar_fg)
                .bg(theme.tab_inactive_bg),
        ),
        Span::styled(
            "│",
            Style::default()
                .fg(theme.border_color)
                .bg(theme.tab_inactive_bg),
        ),
        Span::styled(
            format!(" Tab {}/{} ", app.active_tab + 1, app.tabs.len()),
            Style::default().fg(theme.fg).bg(theme.tab_inactive_bg),
        ),
        Span::styled(" ", Style::default().bg(theme.tab_inactive_bg)),
    ];

    let left_line = Line::from(left);
    let right_line = Line::from(right);

    let total_width = area.width as usize;
    let left_width: usize = left_line
        .spans
        .iter()
        .map(|s| s.content.width())
        .sum();
    let right_width: usize = right_line
        .spans
        .iter()
        .map(|s| s.content.width())
        .sum();

    let padding = total_width.saturating_sub(left_width + right_width);

    let mut full_line = left_line.clone();
    full_line.push_span(Span::styled(
        " ".repeat(padding),
        Style::default().bg(theme.tab_inactive_bg),
    ));
    for span in right_line.spans {
        full_line.push_span(span);
    }

    frame.render_widget(Paragraph::new(Text::from(vec![full_line])), area);
}

fn render_command_palette(frame: &mut Frame, app: &App, layout: &AppLayout, theme: &Theme) {
    let Some(palette_area) = layout.command_palette else {
        return;
    };

    let palette_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme.accent))
        .title(Span::styled(
            " ⌕ 命令面板 ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme.input_bg));

    let inner = palette_block.inner(palette_area);

    let all_commands = vec![
        ("🔄 /new", "新建标签页", "创建一个新的对话标签"),
        ("❌ /close", "关闭当前标签页", "关闭当前正在使用的标签"),
        (
            "🤖 /agent",
            "切换智能体",
            "/agent 哪吒｜代码审计专家｜威胁情报分析师...",
        ),
        (
            "🎨 /theme",
            "切换主题",
            "/theme dracula｜tokyo-night｜catppuccin｜nord...",
        ),
        (
            "⚙️ /model",
            "切换模型",
            "/model deepseek-v4-pro｜deepseek-v4-flash",
        ),
        ("💾 /save", "保存对话", "将当前对话保存到记忆库"),
        ("📂 /load", "加载对话", "/load <id> 从记忆库加载对话"),
        ("📋 /history", "历史对话", "列出所有已保存的对话"),
        ("⭐ /fav", "收藏对话", "收藏/取消收藏当前对话"),
        ("⬆️  Ctrl+P", "上一条命令", "历史记录上一条"),
        ("⬇️  Ctrl+N", "下一条命令", "历史记录下一条"),
        ("📑 Ctrl+1~9", "切换 Agent", "快速切换到对应位置的智能体"),
        ("🔄 Ctrl+Tab", "循环切换 Agent", "切换到下一个智能体"),
        ("📌 Ctrl+B", "折叠侧边栏", "显示或隐藏左侧侧边栏"),
    ];

    let filter = app.command_palette_input.to_lowercase();
    let filtered: Vec<&(&str, &str, &str)> = all_commands
        .iter()
        .filter(|(cmd, desc, detail)| {
            let combined = format!("{} {} {}", cmd, desc, detail).to_lowercase();
            filter.is_empty() || combined.contains(&filter)
        })
        .collect();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(inner);

    let input_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme.border_color));

    let prompt = "> ";
    let input_text = format!("{}{}", prompt, app.command_palette_input);
    frame.render_widget(
        Paragraph::new(input_text.as_str())
            .block(input_block)
            .style(Style::default().fg(theme.fg).bg(theme.input_bg)),
        chunks[0],
    );

    let list_area = chunks[1];
    let max_items = list_area.height as usize;
    let selected = 0usize.min(filtered.len().saturating_sub(1));
    let scroll = if filtered.len() > max_items {
        selected.saturating_sub(max_items / 2)
    } else {
        0
    };

    let mut list_lines: Vec<Line<'static>> = Vec::new();
    for (i, (cmd, desc, _detail)) in filtered.iter().enumerate().skip(scroll).take(max_items) {
        let is_selected = i == selected;
        let style = if is_selected {
            Style::default().fg(theme.fg).bg(theme.tab_active_bg)
        } else {
            Style::default().fg(theme.sidebar_fg).bg(theme.input_bg)
        };
        let prefix = if is_selected { "▶ " } else { "  " };
        list_lines.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(
                cmd.to_string(),
                if is_selected {
                    Style::default()
                        .fg(theme.accent)
                        .bg(theme.tab_active_bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.accent).bg(theme.input_bg)
                },
            ),
            Span::styled("  ", style),
            Span::styled(desc.to_string(), style),
        ]));
    }

    if list_lines.is_empty() {
        list_lines.push(Line::from(Span::styled(
            "  没有找到匹配的命令",
            Style::default().fg(theme.accent_dim),
        )));
    }

    frame.render_widget(Paragraph::new(Text::from(list_lines)), list_area);
    frame.render_widget(palette_block, palette_area);

    let cursor_x = chunks[0].x + 2 + app.command_palette_input.len() as u16;
    let cursor_y = chunks[0].y + 1;
    if cursor_y < palette_area.y + palette_area.height {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

fn render_confirmation_dialog(frame: &mut Frame, app: &App, theme: &Theme) {
    let Some(ref dialog) = app.pending_confirmation else {
        return;
    };

    let title = "工具调用确认";
    let desc = &dialog.description;

    let area = frame.area();
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let lines_count = (desc.len() / (dialog_width as usize - 4)).max(2) + 6;
    let dialog_height = lines_count.min(area.height.saturating_sub(4) as usize) as u16;

    let dialog_area = Rect {
        x: area.x + (area.width - dialog_width) / 2,
        y: area.y + (area.height - dialog_height) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme.warning_color))
        .title(Span::styled(
            format!(" ⚠ {} ", title),
            Style::default()
                .fg(theme.warning_color)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme.input_bg));

    let inner = block.inner(dialog_area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(inner);

    frame.render_widget(Clear, dialog_area);
    frame.render_widget(block, dialog_area);

    let wrapped = wrap_text(desc, inner.width.saturating_sub(2) as usize, "");
    let mut msg_lines: Vec<Line<'static>> = Vec::new();
    for wl in wrapped {
        msg_lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(wl, Style::default().fg(theme.fg)),
        ]));
    }

    frame.render_widget(Paragraph::new(Text::from(msg_lines)), chunks[0]);

    let buttons = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            " [Y] 确认 ",
            Style::default()
                .fg(theme.success_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" [N] 取消 ", Style::default().fg(theme.error_color)),
        Span::styled(" (默认: 取消)", Style::default().fg(theme.accent_dim)),
    ]);

    frame.render_widget(Paragraph::new(Text::from(vec![buttons])), chunks[1]);
}

/// 按宽度断行（支持中文，按字符宽度计算）。
///
/// 每行前添加 prefix 前缀，用于列表/引用的缩进对齐。
fn wrap_text(text: &str, max_width: usize, prefix: &str) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    let effective_width = max_width.saturating_sub(prefix.width());
    let effective_width = effective_width.max(10);

    let mut result = Vec::new();
    for para_line in text.lines() {
        if para_line.is_empty() {
            result.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut current_width = 0;
        for ch in para_line.chars() {
            let w = ch.to_string().width();
            if current_width + w > effective_width && !current.is_empty() {
                result.push(format!("{}{}", prefix, current));
                current.clear();
                current_width = 0;
            }
            current.push(ch);
            current_width += w;
        }
        if !current.is_empty() || result.is_empty() {
            result.push(format!("{}{}", prefix, current));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_text_empty_string_returns_one_line() {
        let result = wrap_text("", 80, "");
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn wrap_text_short_line_no_wrap() {
        let result = wrap_text("hello", 80, "");
        assert_eq!(result, vec!["hello"]);
    }

    #[test]
    fn wrap_text_long_line_wraps() {
        let text = "a".repeat(100);
        let result = wrap_text(&text, 40, "");
        assert!(result.len() > 1);
        for line in &result {
            assert!(line.len() <= 40 || line.len() == 1);
        }
    }

    #[test]
    fn wrap_text_with_prefix_indents() {
        let result = wrap_text("test", 40, "  ");
        assert_eq!(result, vec!["  test"]);
    }

    #[test]
    fn wrap_text_multiline_input() {
        let text = "line1\nline2\nline3";
        let result = wrap_text(text, 40, "");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn render_inline_markdown_plain_text() {
        let theme = super::super::theme::get_theme("default");
        let spans = render_inline_markdown("hello world", &theme);
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn render_inline_markdown_bold() {
        let theme = super::super::theme::get_theme("default");
        let spans = render_inline_markdown("**bold**", &theme);
        assert!(spans.len() >= 1);
    }

    #[test]
    fn render_inline_markdown_code() {
        let theme = super::super::theme::get_theme("default");
        let spans = render_inline_markdown("`code`", &theme);
        assert!(spans.len() >= 1);
    }
}
