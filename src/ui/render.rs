//! 渲染逻辑 —— 将 `App` 状态绘制到终端。
//!
//! 设计目标：
//! - 信息分层清晰：会话 → 消息 → 输入 → 状态，一眼能找到重点
//! - 键盘 / 鼠标都可操作，所有重要操作都有可见提示
//! - 中文、Emoji、宽字符安全对齐，窄终端也不会溢出
//! - 只重绘需要重绘的帧，空闲时几乎不占 CPU
//!
//! 所有渲染函数接收 `Frame` 与 `App`，不执行网络或持久化副作用。

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};
use unicode_width::UnicodeWidthStr;

use super::layout::{
    agent_rows_available, agent_window, calculate, calculate_for, tab_bar_labels, AppLayout,
    MAX_INPUT_ROWS, SIDEBAR_HEADER_ROWS, SIDEBAR_USAGE_ROWS, SIDEBAR_WIDTH_PCT,
};
use super::theme::Theme;
use crate::api::types::{Message, Role};
use crate::app::{App, BalanceStatus, CompletionKind, Focus, StatusLevel, Tab, ToolCallStatus};
use crate::input;
use crate::text;

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    // 先探测主区域宽度，再根据输入内容计算输入框高度。
    let probe = calculate(
        area,
        app.sidebar_visible,
        app.command_palette_open,
        SIDEBAR_WIDTH_PCT,
    );
    let input_rows = input_rows_for(app, probe.main_area.width);
    let layout = calculate_for(
        area,
        app.sidebar_visible,
        app.command_palette_open,
        SIDEBAR_WIDTH_PCT,
        input_rows,
    );
    let theme = super::theme::get_theme(&app.theme);

    frame.render_widget(
        Block::default().style(Style::default().bg(theme.bg).fg(theme.fg)),
        area,
    );

    if let Some(sidebar) = layout.sidebar {
        render_sidebar(frame, app, sidebar, &theme);
    }
    render_tab_bar(frame, app, layout.tab_bar, &theme);
    render_messages(frame, app, layout.messages, &theme);
    render_input_area(frame, app, layout.input_area, &theme);
    render_status_bar(frame, app, layout.status_bar, &theme);
    render_completion_popup(frame, app, &layout, &theme);
    render_command_palette(frame, app, &layout, &theme);
    render_help_overlay(frame, app, &theme);
    render_confirmation_dialog(frame, app, &theme);
}

/// 输入框内容区需要显示的行数（1-8）。
pub fn input_rows_for(app: &App, main_width: u16) -> u16 {
    let inner_width = main_width.saturating_sub(2) as usize;
    let text_width = inner_width.saturating_sub(2).max(1);
    let tab = app.active_tab();
    let layout = input::visual_layout(&tab.input_buffer, tab.input_cursor, text_width);
    (layout.rows.len() as u16).clamp(1, MAX_INPUT_ROWS)
}

fn agent_icon(index: usize) -> &'static str {
    const ICONS: [&str; 9] = ["🔥", "🔍", "🎯", "🎭", "💣", "🚀", "🔄", "🔬", "🏅"];
    ICONS.get(index).copied().unwrap_or("✨")
}

fn agent_icon_padded(index: usize) -> String {
    let icon = agent_icon(index);
    let width = text::display_width(icon);
    format!("{}{}", icon, " ".repeat(2usize.saturating_sub(width)))
}

fn agent_icon_by_name(app: &App, name: &str) -> &'static str {
    app.agents
        .iter()
        .position(|agent| agent.name == name)
        .map(agent_icon)
        .unwrap_or("✨")
}

fn format_tokens(tokens: u64) -> String {
    if tokens < 1_000 {
        tokens.to_string()
    } else if tokens < 1_000_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("{:.2}M", tokens as f64 / 1_000_000.0)
    }
}

fn elapsed_seconds(tab: &Tab) -> f32 {
    tab.request_started
        .map(|started| started.elapsed().as_secs_f32())
        .unwrap_or(0.0)
}

// ---------------------------------------------------------------------------
// 侧边栏
// ---------------------------------------------------------------------------

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
            Constraint::Length(SIDEBAR_HEADER_ROWS),
            Constraint::Length(SIDEBAR_USAGE_ROWS),
            Constraint::Min(0),
        ])
        .split(inner);

    render_sidebar_header(frame, chunks[0], theme);
    render_sidebar_usage(frame, app, chunks[1], theme);
    render_sidebar_agents(frame, app, chunks[2], area, theme);
}

fn render_sidebar_header(frame: &mut Frame, area: Rect, theme: &Theme) {
    let lines = vec![
        Line::from(Span::styled(
            " ⚡ 哪吒网络安全",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(" 红队终端 · v{}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(theme.accent_dim),
        )),
        Line::from(""),
    ];
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn render_sidebar_usage(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let tab = app.active_tab();
    let label = Style::default().fg(theme.sidebar_fg);
    let (balance_text, balance_color) = match &app.balance {
        BalanceStatus::Loading => ("查询中…".to_string(), theme.sidebar_fg),
        BalanceStatus::Ready(value) => (format!("¥{:.2}", value), theme.success_color),
        BalanceStatus::Failed(_) => ("查询失败".to_string(), theme.error_color),
    };

    let lines = vec![
        Line::from(Span::styled(
            " ── 用量 ──",
            Style::default()
                .fg(theme.accent_dim)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("  余额  ", label),
            Span::styled(
                balance_text,
                Style::default()
                    .fg(balance_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  上下文  ", label),
            Span::styled(
                format!("{} tok", format_tokens(tab.last_prompt_tokens)),
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled("  累计  ", label),
            Span::styled(
                format!("{} tok", format_tokens(tab.total_usage.total_tokens)),
                Style::default().fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled("  费用  ", label),
            Span::styled(
                format!("¥{:.4}", tab.total_usage.cost(&app.pricing)),
                Style::default().fg(theme.warning_color),
            ),
        ]),
        Line::from(vec![
            Span::styled("  会话  ", label),
            Span::styled(
                match tab.conversation_id {
                    Some(id) => format!("#{}", id),
                    None => "未保存".to_string(),
                },
                Style::default().fg(theme.accent_dim),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            " Ctrl+B 收起 · F1 帮助",
            Style::default().fg(theme.accent_dim),
        )),
    ];
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn render_sidebar_agents(frame: &mut Frame, app: &App, chunk: Rect, sidebar: Rect, theme: &Theme) {
    let active_index = app
        .agents
        .iter()
        .position(|agent| agent.name == app.active_tab().agent_name)
        .unwrap_or(0);
    let rows = agent_rows_available(sidebar);
    let (start, len) = agent_window(app.agents.len(), active_index, rows);

    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        " ── AGENTS ──",
        Style::default()
            .fg(theme.accent_dim)
            .add_modifier(Modifier::BOLD),
    ))];

    let name_width = (chunk.width as usize).saturating_sub(7).max(6);
    for index in start..start + len {
        let agent = &app.agents[index];
        let is_active = index == active_index;
        let style = if is_active {
            Style::default()
                .fg(theme.accent)
                .bg(theme.tab_active_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.sidebar_fg)
        };
        let prefix = if is_active { "▶" } else { " " };
        let name = text::truncate_width(&agent.name, name_width);
        lines.push(Line::from(Span::styled(
            format!(" {}{} {}", prefix, agent_icon_padded(index), name),
            style,
        )));
    }

    lines.push(Line::from(""));
    if let Some(agent) = app.agents.get(active_index) {
        lines.push(Line::from(Span::styled(
            format!(" 可用工具 {} 个", agent.tools.len()),
            Style::default().fg(theme.accent_dim),
        )));
        for line in text::wrap_text(&agent.description, chunk.width as usize, " ")
            .into_iter()
            .take(2)
        {
            lines.push(Line::from(Span::styled(
                line,
                Style::default()
                    .fg(theme.accent_dim)
                    .add_modifier(Modifier::ITALIC),
            )));
        }
    }
    while lines.len() < (chunk.height as usize).saturating_sub(2) {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        " Alt+1..9 切换 Agent",
        Style::default().fg(theme.accent_dim),
    )));
    lines.push(Line::from(Span::styled(
        " Ctrl+Tab 下一个",
        Style::default().fg(theme.accent_dim),
    )));

    frame.render_widget(Paragraph::new(Text::from(lines)), chunk);
}

// ---------------------------------------------------------------------------
// 标签栏 / 消息区
// ---------------------------------------------------------------------------

fn render_tab_bar(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let labels = tab_bar_labels(&app.tabs, app.active_tab, area.width);
    let mut spans: Vec<Span> = vec![Span::raw("  ")];

    for (position, (index, label)) in labels.iter().enumerate() {
        if position > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(theme.border_color)));
        }
        let is_active = *index == app.active_tab;
        let style = if is_active {
            Style::default()
                .fg(theme.accent)
                .bg(theme.tab_active_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.sidebar_fg)
        };
        spans.push(Span::styled(label.clone(), style));
    }

    let used: usize = spans.iter().map(|span| span.content.width()).sum();
    let hint = "Ctrl+N 新建 ";
    let hint_width = text::display_width(hint);
    if used + hint_width + 3 < area.width as usize {
        spans.push(Span::raw(
            " ".repeat(area.width as usize - used - hint_width),
        ));
        spans.push(Span::styled(
            hint,
            Style::default().fg(theme.accent_dim).bg(theme.bg),
        ));
    }

    let title_line = Line::from(spans);
    let border_line = Line::from(Span::styled(
        "─".repeat(area.width as usize),
        Style::default().fg(theme.border_color),
    ));
    frame.render_widget(
        Paragraph::new(Text::from(vec![title_line, border_line]))
            .style(Style::default().bg(theme.bg)),
        area,
    );
}

fn render_messages(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    if app.active_tab().messages.is_empty() {
        app.active_tab_mut().max_scroll = 0;
        render_welcome(frame, app, area, theme);
        return;
    }

    let agent_index = app
        .agents
        .iter()
        .position(|agent| agent.name == app.active_tab().agent_name);
    let tab = &mut app.tabs[app.active_tab];
    let max_width = area.width.saturating_sub(2) as usize;
    let visible = area.height as usize;

    let mut lines: Vec<Line<'static>> = vec![Line::from("")];
    let message_count = tab.messages.len();

    for (index, message) in tab.messages.iter().enumerate() {
        let is_last = index + 1 == message_count;
        let streaming = is_last && tab.is_streaming && message.role == Role::Assistant;
        let has_tool_calls = message
            .tool_calls
            .as_ref()
            .map(|calls| !calls.is_empty())
            .unwrap_or(false);
        let is_thinking = streaming && message.content.is_empty() && !has_tool_calls;

        if is_thinking {
            let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
                [(tab.thinking_ticks as usize / 2) % 10];
            let name = text::truncate_width(&tab.agent_name, 16);
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{} ", spinner), Style::default().fg(theme.accent)),
                Span::styled(
                    format!("{} 正在思考… {:.1}s", name, elapsed_seconds(tab)),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
            lines.push(Line::from(""));
            continue;
        }

        match message.role {
            Role::User => render_user_message(&mut lines, message, max_width, theme),
            Role::Assistant => render_assistant_message(
                &mut lines,
                message,
                tab,
                agent_index,
                streaming,
                max_width,
                theme,
            ),
            Role::System => render_system_message(&mut lines, message, max_width, theme),
            Role::Tool => {
                // 工具卡已经展示了结果时，不再重复渲染一条工具结果消息。
                let shown_in_card = message
                    .tool_call_id
                    .as_ref()
                    .map(|id| tab.pending_tool_calls.contains_key(id))
                    .unwrap_or(false);
                if !shown_in_card {
                    render_tool_result(&mut lines, message, max_width, theme);
                }
            }
        }

        lines.push(Line::from(""));
    }

    let total = lines.len();
    let max_scroll = total.saturating_sub(visible);
    let scroll = if tab.auto_scroll {
        max_scroll
    } else {
        tab.scroll_offset.min(max_scroll)
    };
    tab.max_scroll = max_scroll;

    let has_scrollbar = total > visible;

    if has_scrollbar {
        let content_area = Rect {
            width: area.width.saturating_sub(1),
            ..area
        };
        frame.render_widget(
            Paragraph::new(Text::from(lines)).style(Style::default().bg(theme.bg)),
            content_area,
        );

        let mut state = ScrollbarState::new(total)
            .position(scroll)
            .viewport_content_length(visible);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .track_style(Style::default().fg(theme.border_color).bg(theme.bg))
            .thumb_symbol("▐")
            .thumb_style(Style::default().fg(theme.accent).bg(theme.bg));
        frame.render_stateful_widget(scrollbar, area, &mut state);
    } else {
        frame.render_widget(
            Paragraph::new(Text::from(lines)).style(Style::default().bg(theme.bg)),
            area,
        );
    }
}

fn render_welcome(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let tab = app.active_tab();
    let icon = agent_icon_by_name(app, &tab.agent_name);
    let has_key = !app.client.api_key().trim().is_empty();

    let mut lines: Vec<Line<'static>> = vec![
        Line::from(""),
        Line::from(Span::styled(
            "⚡ 哪吒网络安全",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "红队作战终端 · DeepSeek 驱动",
            Style::default().fg(theme.accent_dim),
        )),
        Line::from(""),
    ];

    let mut info_spans = vec![
        Span::styled("当前 Agent  ", Style::default().fg(theme.accent_dim)),
        Span::styled(
            format!("{} {}", icon, tab.agent_name),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        ),
    ];
    if area.width >= 52 {
        info_spans.push(Span::styled(
            "   模型  ",
            Style::default().fg(theme.accent_dim),
        ));
        info_spans.push(Span::styled(
            tab.model.clone(),
            Style::default().fg(theme.fg),
        ));
    }
    lines.push(Line::from(info_spans));
    lines.push(Line::from(""));

    if area.height < 12 {
        // 小终端只保留身份、当前 Agent 与一句操作提示。
        lines.truncate(5);
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            if has_key {
                "Enter 发送 · / 命令 · F1 帮助"
            } else {
                "未配置 DEEPSEEK_TOKEN · 请在 .env 中设置"
            },
            Style::default().fg(if has_key {
                theme.accent_dim
            } else {
                theme.warning_color
            }),
        )));
    } else if has_key {
        lines.push(Line::from(Span::styled(
            "试试这样开始",
            Style::default()
                .fg(theme.success_color)
                .add_modifier(Modifier::BOLD),
        )));
        for tip in [
            "› 对 example.com 做一次子域枚举与端口扫描",
            "› 审计 src/ 目录中的 SQL 注入与反序列化风险",
            "› @README.md 总结这个项目的攻击面",
        ] {
            lines.push(Line::from(Span::styled(tip, Style::default().fg(theme.fg))));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Ctrl+K 命令面板    F1 快捷键    Ctrl+B 侧边栏",
            Style::default().fg(theme.accent_dim),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "⚠ 尚未配置 API Key",
            Style::default()
                .fg(theme.warning_color)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            "在项目根目录创建 .env，写入：",
            Style::default().fg(theme.fg),
        )));
        lines.push(Line::from(Span::styled(
            "DEEPSEEK_TOKEN=sk-...",
            Style::default().fg(theme.accent),
        )));
        lines.push(Line::from(Span::styled(
            "保存后重新启动程序",
            Style::default().fg(theme.accent_dim),
        )));
    }

    // 整个欢迎区块水平居中（而不是逐行居中），视觉上更整齐。
    let block_width = lines
        .iter()
        .map(|line| line.width())
        .max()
        .unwrap_or(0)
        .min(area.width as usize);
    let left_padding = area.width.saturating_sub(block_width as u16) / 2;
    let top_padding = area
        .height
        .saturating_sub(lines.len() as u16)
        .saturating_div(3) as usize;

    let mut padded: Vec<Line> = (0..top_padding).map(|_| Line::from("")).collect();
    for line in lines {
        let mut spans = vec![Span::raw(" ".repeat(left_padding as usize))];
        spans.extend(line.spans);
        padded.push(Line::from(spans));
    }

    frame.render_widget(
        Paragraph::new(Text::from(padded))
            .style(Style::default().bg(theme.bg))
            .alignment(Alignment::Left),
        area,
    );
}

fn render_user_message(
    lines: &mut Vec<Line<'static>>,
    message: &Message,
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

    if message.content.is_empty() {
        return;
    }

    let bubble_width = max_width.saturating_sub(3).clamp(20, 96);
    for raw_line in message.content.lines() {
        for wrapped in text::wrap_text(raw_line, bubble_width, "") {
            lines.push(Line::from(vec![
                Span::styled("   ▏ ", Style::default().fg(theme.user_msg_color)),
                Span::styled(wrapped, Style::default().fg(theme.fg)),
            ]));
        }
    }
}

fn render_assistant_message(
    lines: &mut Vec<Line<'static>>,
    message: &Message,
    tab: &Tab,
    agent_index: Option<usize>,
    streaming: bool,
    max_width: usize,
    theme: &Theme,
) {
    let icon = agent_index.map(agent_icon).unwrap_or("✨");
    let name = text::truncate_width(&tab.agent_name, 20);
    lines.push(Line::from(vec![
        Span::styled(format!("  {} ", icon), Style::default().fg(theme.accent)),
        Span::styled(
            name,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    if !message.content.is_empty() {
        render_markdown(lines, &message.content, max_width, theme);
        if streaming {
            if let Some(last) = lines.last_mut() {
                last.push_span(Span::styled("▌", Style::default().fg(theme.accent)));
            }
        }
    }

    if let Some(tool_calls) = &message.tool_calls {
        render_tool_cards(lines, tool_calls, tab, max_width, theme);
    }
}

fn render_tool_cards(
    lines: &mut Vec<Line<'static>>,
    tool_calls: &[crate::api::types::ToolCall],
    tab: &Tab,
    max_width: usize,
    theme: &Theme,
) {
    for call in tool_calls {
        let status = tab.pending_tool_calls.get(&call.id);
        let (icon, label, color) = match status {
            Some(ToolCallStatus::Success { .. }) => ("✓", "完成", theme.success_color),
            Some(ToolCallStatus::Failed { .. }) => ("✕", "已跳过", theme.error_color),
            Some(ToolCallStatus::Running) => ("⟳", "执行中", theme.warning_color),
            Some(ToolCallStatus::Pending) => ("…", "等待确认", theme.warning_color),
            _ => ("·", "已记录", theme.accent_dim),
        };

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("{} ", icon),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}()", call.function.name),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {}", label), Style::default().fg(color)),
        ]));

        let args = call.function.arguments.trim();
        if !args.is_empty() {
            let compact = args.replace('\n', " ");
            let wrapped = text::wrap_text(&compact, max_width.saturating_sub(6).max(10), "");
            for (index, line) in wrapped.iter().take(3).enumerate() {
                let prefix = if index == 0 { "     ⌘ " } else { "       " };
                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(theme.border_color)),
                    Span::styled(line.clone(), Style::default().fg(theme.accent_dim)),
                ]));
            }
            if wrapped.len() > 3 {
                lines.push(Line::from(Span::styled(
                    format!("       … 参数共 {} 行", wrapped.len()),
                    Style::default().fg(theme.border_color),
                )));
            }
        }

        match status {
            Some(ToolCallStatus::Success { result }) => {
                let cleaned = strip_code_fence(result);
                let preview: Vec<&str> = cleaned.lines().take(5).collect();
                for (index, line) in preview.iter().enumerate() {
                    let prefix = if index == 0 { "     ↳ " } else { "       " };
                    let line = text::truncate_width(line, max_width.saturating_sub(8).max(10));
                    lines.push(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(theme.success_color)),
                        Span::styled(line, Style::default().fg(theme.sidebar_fg)),
                    ]));
                }
                let total_lines = cleaned.lines().count();
                if total_lines > preview.len() {
                    lines.push(Line::from(Span::styled(
                        format!("       … 结果共 {} 行（已截断显示）", total_lines),
                        Style::default().fg(theme.border_color),
                    )));
                }
            }
            Some(ToolCallStatus::Failed { error }) => {
                lines.push(Line::from(vec![
                    Span::styled("     ↳ ", Style::default().fg(theme.error_color)),
                    Span::styled(error.clone(), Style::default().fg(theme.error_color)),
                ]));
            }
            _ => {}
        }
    }
}

fn render_system_message(
    lines: &mut Vec<Line<'static>>,
    message: &Message,
    max_width: usize,
    theme: &Theme,
) {
    lines.push(Line::from(vec![
        Span::styled("  ℹ ", Style::default().fg(theme.accent_dim)),
        Span::styled(
            "系统",
            Style::default()
                .fg(theme.accent_dim)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    for raw in message.content.lines() {
        for wrapped in text::wrap_text(raw, max_width.saturating_sub(3), "") {
            lines.push(Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(wrapped, Style::default().fg(theme.sidebar_fg)),
            ]));
        }
    }
}

/// 去掉工具返回内容外层的 Markdown 代码围栏，卡片中只展示正文。
fn strip_code_fence(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let body = rest.split_once('\n').map(|(_, body)| body).unwrap_or(rest);
        if let Some(inner) = body.trim_end().strip_suffix("```") {
            return inner.trim_end().to_string();
        }
    }
    trimmed.to_string()
}

fn render_tool_result(
    lines: &mut Vec<Line<'static>>,
    message: &Message,
    max_width: usize,
    theme: &Theme,
) {
    lines.push(Line::from(vec![
        Span::styled("  ↳ ", Style::default().fg(theme.success_color)),
        Span::styled(
            "工具结果",
            Style::default()
                .fg(theme.success_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    let cleaned = strip_code_fence(&message.content);
    let preview: Vec<&str> = cleaned.lines().take(6).collect();
    for line in &preview {
        let line = text::truncate_width(line, max_width.saturating_sub(4).max(10));
        lines.push(Line::from(vec![
            Span::styled("     ", Style::default()),
            Span::styled(line, Style::default().fg(theme.sidebar_fg)),
        ]));
    }
    let total = cleaned.lines().count();
    if total > preview.len() {
        lines.push(Line::from(Span::styled(
            format!("     … 共 {} 行", total),
            Style::default().fg(theme.border_color),
        )));
    }
}

// ---------------------------------------------------------------------------
// Markdown
// ---------------------------------------------------------------------------

fn render_markdown(lines: &mut Vec<Line<'static>>, content: &str, max_width: usize, theme: &Theme) {
    let mut in_code_block = false;
    let mut code_box_width = 40usize;

    for line in content.lines() {
        if let Some(lang) = line.strip_prefix("```") {
            if !in_code_block {
                in_code_block = true;
                code_box_width = max_width.saturating_sub(3).clamp(20, 100);
                let lang = lang.trim();
                let label = if lang.is_empty() { "code" } else { lang };
                let label = text::truncate_width(label, code_box_width.saturating_sub(8));
                let head = format!("╭─ {} ", label);
                let fill = code_box_width.saturating_sub(text::display_width(&head) + 1);
                lines.push(Line::from(vec![
                    Span::styled("   ", Style::default()),
                    Span::styled(
                        format!("{}{}╮", head, "─".repeat(fill)),
                        Style::default().fg(theme.border_color),
                    ),
                ]));
            } else {
                in_code_block = false;
                lines.push(Line::from(vec![
                    Span::styled("   ", Style::default()),
                    Span::styled(
                        format!("╰{}╯", "─".repeat(code_box_width.saturating_sub(2))),
                        Style::default().fg(theme.border_color),
                    ),
                ]));
            }
            continue;
        }

        if in_code_block {
            let inner_width = code_box_width.saturating_sub(4);
            for wrapped_line in text::wrap_text(line, inner_width.max(10), "") {
                let line_width = text::display_width(&wrapped_line);
                lines.push(Line::from(vec![
                    Span::styled("   ", Style::default()),
                    Span::styled("│ ", Style::default().fg(theme.border_color)),
                    Span::styled(wrapped_line, Style::default().fg(theme.warning_color)),
                    Span::styled(
                        format!("{} │", " ".repeat(inner_width.saturating_sub(line_width))),
                        Style::default().fg(theme.border_color),
                    ),
                ]));
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("### ") {
            lines.push(Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(
                    rest.to_string(),
                    Style::default()
                        .fg(theme.accent_dim)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            continue;
        }

        if let Some(rest) = line.strip_prefix("## ") {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(
                    rest.to_string(),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            continue;
        }

        if let Some(rest) = line.strip_prefix("# ") {
            lines.push(Line::from(""));
            let title = rest.to_string();
            let underline_width = title.width().min(max_width.saturating_sub(3));
            lines.push(Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(
                    title,
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(
                    "─".repeat(underline_width),
                    Style::default().fg(theme.accent_dim),
                ),
            ]));
            continue;
        }

        if line.starts_with("- ") || line.starts_with("* ") || line.starts_with("+ ") {
            let content = &line[2..];
            let wrapped = text::wrap_text(content, max_width.saturating_sub(5), "");
            for (index, wrapped_line) in wrapped.iter().enumerate() {
                let bullet = if index == 0 { "•" } else { " " };
                lines.push(Line::from(vec![
                    Span::styled("     ", Style::default()),
                    Span::styled(bullet, Style::default().fg(theme.accent)),
                    Span::styled(" ", Style::default()),
                    Span::styled(wrapped_line.clone(), Style::default().fg(theme.fg)),
                ]));
            }
            continue;
        }

        if let Some((number, rest)) = ordered_list_item(line) {
            let wrapped = text::wrap_text(rest, max_width.saturating_sub(7), "");
            for (index, wrapped_line) in wrapped.iter().enumerate() {
                let marker = if index == 0 {
                    format!("{}.", number)
                } else {
                    "  ".to_string()
                };
                lines.push(Line::from(vec![
                    Span::styled("     ", Style::default()),
                    Span::styled(marker, Style::default().fg(theme.accent)),
                    Span::styled(" ", Style::default()),
                    Span::styled(wrapped_line.clone(), Style::default().fg(theme.fg)),
                ]));
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("> ") {
            let wrapped = text::wrap_text(rest, max_width.saturating_sub(5), "");
            for wrapped_line in wrapped {
                lines.push(Line::from(vec![
                    Span::styled("   ", Style::default()),
                    Span::styled("┃ ", Style::default().fg(theme.accent_dim)),
                    Span::styled(
                        wrapped_line,
                        Style::default()
                            .fg(theme.sidebar_fg)
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]));
            }
            continue;
        }

        if line.trim() == "---" || line.trim() == "***" {
            lines.push(Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(
                    "─".repeat(max_width.saturating_sub(4)),
                    Style::default().fg(theme.border_color),
                ),
            ]));
            continue;
        }

        if line.trim().is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        for wrapped in text::wrap_text(line, max_width.saturating_sub(3), "") {
            let mut spans = vec![Span::styled("   ", Style::default())];
            spans.extend(render_inline_markdown(&wrapped, theme));
            lines.push(Line::from(spans));
        }
    }
}

fn ordered_list_item(line: &str) -> Option<(u32, &str)> {
    let digits: String = line.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = &line[digits.len()..];
    let rest = rest.strip_prefix(". ")?;
    Some((digits.parse().ok()?, rest))
}

/// 行内 Markdown：**粗体**、`代码`、*斜体*、[链接](url)
fn render_inline_markdown(text: &str, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        let bold = remaining.find("**").map(|index| (index, "**"));
        let code = remaining.find('`').map(|index| (index, "`"));
        let link = remaining.find('[').map(|index| (index, "["));
        let italic = remaining.find('*').map(|index| (index, "*"));

        let next = [bold, code, link, italic]
            .into_iter()
            .flatten()
            .min_by_key(|(index, _)| *index);

        let Some((index, marker)) = next else {
            spans.push(Span::styled(
                remaining.to_string(),
                Style::default().fg(theme.fg),
            ));
            break;
        };

        if index > 0 {
            spans.push(Span::styled(
                remaining[..index].to_string(),
                Style::default().fg(theme.fg),
            ));
        }

        let after = &remaining[index + marker.len()..];
        match marker {
            "**" => {
                if let Some(end) = after.find("**") {
                    spans.push(Span::styled(
                        after[..end].to_string(),
                        Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
                    ));
                    remaining = &after[end + 2..];
                } else {
                    spans.push(Span::styled("**", Style::default().fg(theme.fg)));
                    remaining = after;
                }
            }
            "`" => {
                if let Some(end) = after.find('`') {
                    spans.push(Span::styled(
                        format!(" {} ", &after[..end]),
                        Style::default()
                            .fg(theme.warning_color)
                            .bg(theme.tab_active_bg),
                    ));
                    remaining = &after[end + 1..];
                } else {
                    spans.push(Span::styled("`", Style::default().fg(theme.fg)));
                    remaining = after;
                }
            }
            "[" => {
                if let Some(close) = after.find(']') {
                    let label = &after[..close];
                    let rest = &after[close + 1..];
                    if let Some(url) = rest.strip_prefix('(') {
                        if let Some(end) = url.find(')') {
                            spans.push(Span::styled(
                                label.to_string(),
                                Style::default()
                                    .fg(theme.accent)
                                    .add_modifier(Modifier::UNDERLINED),
                            ));
                            spans.push(Span::styled(
                                format!(" ({})", &url[..end]),
                                Style::default().fg(theme.accent_dim),
                            ));
                            remaining = &url[end + 1..];
                            continue;
                        }
                    }
                    spans.push(Span::styled("[", Style::default().fg(theme.fg)));
                    remaining = after;
                } else {
                    spans.push(Span::styled("[", Style::default().fg(theme.fg)));
                    remaining = after;
                }
            }
            "*" => {
                if let Some(end) = after.find('*') {
                    if end > 0 {
                        spans.push(Span::styled(
                            after[..end].to_string(),
                            Style::default().fg(theme.fg).add_modifier(Modifier::ITALIC),
                        ));
                        remaining = &after[end + 1..];
                    } else {
                        spans.push(Span::styled("*", Style::default().fg(theme.fg)));
                        remaining = after;
                    }
                } else {
                    spans.push(Span::styled("*", Style::default().fg(theme.fg)));
                    remaining = after;
                }
            }
            _ => {
                remaining = after;
            }
        }
    }

    spans
}

// ---------------------------------------------------------------------------
// 输入框 / 补全
// ---------------------------------------------------------------------------

fn render_input_area(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let tab = app.active_tab();
    let is_focused = app.focus == Focus::ChatInput && !app.command_palette_open && !app.help_open;
    let border_color = if is_focused {
        theme.accent
    } else {
        theme.border_color
    };

    let hint = if tab.is_streaming {
        format!(" ⏳ 生成中 {:.1}s · Esc 取消 ", elapsed_seconds(tab))
    } else if tab.input_buffer.trim_start().starts_with('/') {
        " Enter 执行命令 · Tab 补全 · Esc 清空 ".to_string()
    } else if area.width < 58 {
        " Enter 发送 ".to_string()
    } else if area.width < 78 {
        " Enter 发送 · Ctrl+J 换行 · / 命令 ".to_string()
    } else {
        " Enter 发送 · Ctrl+J 换行 · @ 文件 · / 命令 · F1 帮助 ".to_string()
    };
    let hint = text::truncate_width(&hint, area.width.saturating_sub(3) as usize);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            " ❯ 输入 ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(hint, Style::default().fg(theme.accent_dim)))
        .style(Style::default().bg(theme.input_bg));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text_width = inner.width.saturating_sub(2).max(1) as usize;
    let layout = input::visual_layout(&tab.input_buffer, tab.input_cursor, text_width);
    let visible = inner.height as usize;
    let scroll = if layout.rows.len() > visible {
        (layout.cursor_row + 1).saturating_sub(visible)
    } else {
        0
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    if tab.input_buffer.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("❯ ", Style::default().fg(theme.accent)),
            Span::styled(
                "输入消息，或输入 / 查看命令…",
                Style::default().fg(theme.accent_dim),
            ),
        ]));
    } else {
        for (row, (start, end)) in layout.rows.iter().enumerate().skip(scroll).take(visible) {
            let prompt = if row == 0 { "❯ " } else { "  " };
            let prompt_style = if row == 0 {
                Style::default().fg(theme.accent)
            } else {
                Style::default().fg(theme.accent_dim)
            };
            let content = tab.input_buffer[*start..*end].to_string();
            lines.push(Line::from(vec![
                Span::styled(prompt, prompt_style),
                Span::styled(content, Style::default().fg(theme.fg)),
            ]));
        }
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(theme.input_bg)),
        inner,
    );

    if is_focused {
        let row = layout.cursor_row.saturating_sub(scroll) as u16;
        let x = inner.x + 2 + layout.cursor_column as u16;
        let y = inner.y + row;
        if x < inner.x + inner.width && y < inner.y + inner.height {
            frame.set_cursor_position((x, y));
        }
    }
}

fn render_completion_popup(frame: &mut Frame, app: &App, layout: &AppLayout, theme: &Theme) {
    let Some(completion) = &app.completion else {
        return;
    };
    if app.command_palette_open || app.help_open || completion.items.is_empty() {
        return;
    }

    let needed = (completion.items.len() + 2) as u16;
    let available_above = layout
        .input_area
        .y
        .saturating_sub(layout.messages.y + 1)
        .max(1);
    let height = needed.min(8).min(available_above);
    let width = layout
        .main_area
        .width
        .saturating_sub(4)
        .clamp(16, 64)
        .min(layout.main_area.width);
    let x = layout.input_area.x + 2;
    let y = layout.input_area.y.saturating_sub(height);
    let rect = Rect::new(x, y, width, height);

    let title = match completion.kind {
        CompletionKind::File => " @ 文件补全 ",
        CompletionKind::Command => " ⌘ 命令补全 ",
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent_dim))
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " Tab 补全 · ↑↓ 选择 · Esc 关闭 ",
            Style::default().fg(theme.accent_dim),
        ))
        .style(Style::default().bg(theme.input_bg));

    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    let inner = Rect {
        x: rect.x + 1,
        y: rect.y + 1,
        width: rect.width.saturating_sub(2),
        height: rect.height.saturating_sub(2),
    };
    let selected = completion.selected.min(completion.items.len() - 1);
    let rows = inner.height as usize;
    let scroll = selected.saturating_sub(rows.saturating_sub(1));

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (index, item) in completion.items.iter().enumerate().skip(scroll).take(rows) {
        let is_selected = index == selected;
        let style = if is_selected {
            Style::default().fg(theme.accent).bg(theme.tab_active_bg)
        } else {
            Style::default().fg(theme.fg).bg(theme.input_bg)
        };
        let prefix = if is_selected { "▶ " } else { "  " };
        let label_width = inner.width as usize;
        let label = text::truncate_width(&item.label, label_width.saturating_sub(4));
        let mut spans = vec![
            Span::styled(prefix, style),
            Span::styled(label, style.add_modifier(Modifier::BOLD)),
        ];
        if !item.detail.is_empty() {
            let detail_width = inner
                .width
                .saturating_sub(2 + text::display_width(&item.label) as u16 + 2);
            if detail_width > 8 {
                spans.push(Span::styled("  ", style));
                spans.push(Span::styled(
                    text::truncate_width(&item.detail, detail_width as usize),
                    Style::default()
                        .fg(theme.accent_dim)
                        .bg(style.bg.unwrap_or(theme.input_bg)),
                ));
            }
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

// ---------------------------------------------------------------------------
// 状态栏 / 命令面板 / 帮助 / 确认框
// ---------------------------------------------------------------------------

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let tab = app.active_tab();
    let (icon, color, text) = if app.status_ttl > 0 && !app.status_message.is_empty() {
        let (icon, color) = match app.status_level {
            StatusLevel::Info => ("•", theme.accent_dim),
            StatusLevel::Success => ("✓", theme.success_color),
            StatusLevel::Warning => ("!", theme.warning_color),
            StatusLevel::Error => ("✕", theme.error_color),
        };
        (icon, color, app.status_message.clone())
    } else if tab.is_streaming {
        (
            "◐",
            theme.accent,
            format!("生成中 {:.1}s · Esc 取消", elapsed_seconds(tab)),
        )
    } else if !tab.auto_scroll {
        (
            "⏸",
            theme.warning_color,
            "已暂停跟随 · Alt+End 回到底部".to_string(),
        )
    } else {
        ("●", theme.success_color, "就绪".to_string())
    };

    // 暂停跟随时优先用暂停图标提示，避免和状态图标叠在一起。
    let (icon, color) = if !tab.auto_scroll && app.status_ttl > 0 {
        ("⏸", theme.warning_color)
    } else {
        (icon, color)
    };

    let cost = tab.total_usage.cost(&app.pricing);
    let right_text = if area.width >= 64 {
        format!(
            "{} · Tab {}/{} · ¥{:.4}",
            text::truncate_width(&tab.model, 18),
            app.active_tab + 1,
            app.tabs.len(),
            cost
        )
    } else if area.width >= 46 {
        format!(
            "{} · {}/{}",
            text::truncate_width(&tab.model, 16),
            app.active_tab + 1,
            app.tabs.len()
        )
    } else {
        text::truncate_width(&tab.model, 16)
    };

    let total_width = area.width as usize;
    let right_width = text::display_width(&right_text) + 2;
    let left_budget = total_width.saturating_sub(right_width).max(6);
    let left_text = text::truncate_width(&text, left_budget.saturating_sub(3));
    let left_width = 2 + text::display_width(&left_text);
    let padding = total_width.saturating_sub(left_width + right_width);

    let line = Line::from(vec![
        Span::styled(" ", Style::default().bg(theme.tab_inactive_bg)),
        Span::styled(
            format!("{} ", icon),
            Style::default().fg(color).bg(theme.tab_inactive_bg),
        ),
        Span::styled(
            left_text,
            Style::default().fg(theme.fg).bg(theme.tab_inactive_bg),
        ),
        Span::styled(
            " ".repeat(padding),
            Style::default().bg(theme.tab_inactive_bg),
        ),
        Span::styled(
            right_text,
            Style::default()
                .fg(theme.accent_dim)
                .bg(theme.tab_inactive_bg),
        ),
        Span::styled(" ", Style::default().bg(theme.tab_inactive_bg)),
    ]);

    frame.render_widget(Paragraph::new(Text::from(vec![line])), area);
}

fn render_command_palette(frame: &mut Frame, app: &App, layout: &AppLayout, theme: &Theme) {
    let Some(base_area) = layout.command_palette else {
        return;
    };

    let filtered = crate::command::filter(&app.command_palette_input);
    // 面板高度跟随结果数量收缩，避免只有一条结果时出现大片空白。
    let height = (filtered.len() as u16 + 4)
        .clamp(6, 18)
        .min(layout.main_area.height);
    let palette_area = Rect {
        height,
        y: base_area.y.min(
            layout
                .main_area
                .y
                .saturating_add(layout.main_area.height.saturating_sub(height)),
        ),
        ..base_area
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme.accent))
        .title(Span::styled(
            " ⌘ 命令面板 ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " Enter 运行 · Tab 补全 · Esc 关闭 ",
            Style::default().fg(theme.accent_dim),
        ))
        .style(Style::default().bg(theme.input_bg));

    frame.render_widget(Clear, palette_area);
    frame.render_widget(block, palette_area);

    let inner = Rect {
        x: palette_area.x + 1,
        y: palette_area.y + 1,
        width: palette_area.width.saturating_sub(2),
        height: palette_area.height.saturating_sub(2),
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner);

    let input_text = format!("❯ {}", app.command_palette_input);
    frame.render_widget(
        Paragraph::new(input_text.as_str()).style(Style::default().fg(theme.fg).bg(theme.input_bg)),
        chunks[0],
    );

    let selected = app
        .command_palette_selected
        .min(filtered.len().saturating_sub(1));
    let rows = chunks[1].height as usize;
    let scroll = if filtered.len() > rows {
        selected.saturating_sub(rows / 2)
    } else {
        0
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "  没有匹配的命令 · 输入 /help 查看全部",
            Style::default().fg(theme.accent_dim),
        )));
    } else {
        for (index, spec) in filtered.iter().enumerate().skip(scroll).take(rows) {
            let is_selected = index == selected;
            let base = if is_selected {
                Style::default().fg(theme.fg).bg(theme.tab_active_bg)
            } else {
                Style::default().fg(theme.sidebar_fg).bg(theme.input_bg)
            };
            let name_style = if is_selected {
                Style::default()
                    .fg(theme.accent)
                    .bg(theme.tab_active_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.accent).bg(theme.input_bg)
            };
            let usage = spec.usage();
            let desc_width =
                (chunks[1].width as usize).saturating_sub(4 + text::display_width(&usage) + 2);
            let mut spans = vec![
                Span::styled(if is_selected { "▶ " } else { "  " }, base),
                Span::styled(format!("{} ", spec.icon), name_style),
                Span::styled(usage, name_style),
            ];
            if desc_width > 8 {
                spans.push(Span::styled("  ", base));
                spans.push(Span::styled(
                    text::truncate_width(spec.desc, desc_width),
                    base,
                ));
            }
            lines.push(Line::from(spans));
        }
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), chunks[1]);

    let cursor_x = chunks[0].x + 2 + text::display_width(&app.command_palette_input) as u16;
    let cursor_y = chunks[0].y;
    if cursor_x < palette_area.x + palette_area.width
        && cursor_y < palette_area.y + palette_area.height
    {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

fn render_help_overlay(frame: &mut Frame, app: &App, theme: &Theme) {
    if !app.help_open {
        return;
    }

    let heading = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let key = Style::default()
        .fg(theme.warning_color)
        .add_modifier(Modifier::BOLD);
    let desc = Style::default().fg(theme.fg);
    let dim = Style::default().fg(theme.accent_dim);

    let lines: Vec<Line> = vec![
        Line::from(Span::styled("导航", heading)),
        Line::from(vec![
            Span::styled("  Ctrl+N 新建会话      ", key),
            Span::styled("Ctrl+W 关闭会话", key),
            Span::styled("      Ctrl+K 命令面板", key),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+B 侧边栏        ", key),
            Span::styled("Ctrl+PgUp/PgDn 切换会话", key),
        ]),
        Line::from(vec![
            Span::styled("  Alt+1..9 / Ctrl+Tab   ", key),
            Span::styled("切换 Agent", desc),
        ]),
        Line::from(""),
        Line::from(Span::styled("输入", heading)),
        Line::from(vec![
            Span::styled("  Enter 发送           ", key),
            Span::styled("Ctrl+J / Alt+Enter 换行", key),
        ]),
        Line::from(vec![
            Span::styled("  ← → 移动光标         ", key),
            Span::styled("Ctrl+← → 按词移动", key),
        ]),
        Line::from(vec![
            Span::styled("  Home / End 行首行尾   ", key),
            Span::styled("Ctrl+A / Ctrl+E 同义", key),
        ]),
        Line::from(vec![
            Span::styled("  ↑ ↓ 历史 / 滚动       ", key),
            Span::styled("Ctrl+U 清空输入", key),
        ]),
        Line::from(vec![
            Span::styled("  @ 引用文件            ", key),
            Span::styled("/ 斜杠命令    Tab 补全", key),
        ]),
        Line::from(""),
        Line::from(Span::styled("会话与显示", heading)),
        Line::from(vec![
            Span::styled("  PgUp / PgDn 翻页      ", key),
            Span::styled("Alt+Home/End 会话首尾 · Esc 取消生成", key),
        ]),
        Line::from(vec![
            Span::styled("  F1 帮助               ", key),
            Span::styled("F2 鼠标捕获 / 允许复制文本", key),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+C 取消 / 退出    ", key),
            Span::styled("Y / N 确认或跳过工具调用", key),
        ]),
        Line::from(""),
        Line::from(Span::styled("命令", heading)),
        Line::from(Span::styled(
            "  /new /close /clear /save /history /load /fav",
            dim,
        )),
        Line::from(Span::styled(
            "  /theme /model /agent /export /help /quit",
            dim,
        )),
    ];

    let area = frame.area();
    let width = area.width.saturating_sub(2).clamp(20, 76).min(area.width);
    let height = (lines.len() as u16 + 2)
        .min(area.height.saturating_sub(2))
        .max(8)
        .min(area.height);
    let rect = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme.accent))
        .title(Span::styled(
            " ⌨ 快捷键与帮助 ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " F1 / Esc 关闭 ",
            Style::default().fg(theme.accent_dim),
        ))
        .style(Style::default().bg(theme.input_bg));

    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    let inner = Rect {
        x: rect.x + 2,
        y: rect.y + 1,
        width: rect.width.saturating_sub(4),
        height: rect.height.saturating_sub(2),
    };

    let mut visible = lines;
    if visible.len() > inner.height as usize {
        visible.truncate(inner.height as usize);
    }
    frame.render_widget(
        Paragraph::new(Text::from(visible)).style(Style::default().bg(theme.input_bg)),
        inner,
    );
}

fn render_confirmation_dialog(frame: &mut Frame, app: &App, theme: &Theme) {
    let Some(dialog) = &app.pending_confirmation else {
        return;
    };

    let area = frame.area();
    let width = area.width.saturating_sub(4).clamp(32, 72).min(area.width);
    let inner_width = width.saturating_sub(4) as usize;

    let mut content: Vec<Line<'static>> = Vec::new();
    for line in text::wrap_text(&dialog.description, inner_width, "") {
        content.push(Line::from(Span::styled(
            line,
            Style::default().fg(theme.fg),
        )));
    }

    content.push(Line::from(""));
    content.push(Line::from(vec![
        Span::styled("工具  ", Style::default().fg(theme.accent_dim)),
        Span::styled(
            dialog.name.clone(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    let pretty_args = serde_json::from_str::<serde_json::Value>(&dialog.args)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| dialog.args.clone());
    let arg_lines: Vec<&str> = pretty_args.lines().collect();
    let shown = arg_lines.len().min(8);
    for line in arg_lines.iter().take(shown) {
        content.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                text::truncate_width(line, inner_width.saturating_sub(2)),
                Style::default().fg(theme.accent_dim),
            ),
        ]));
    }
    if arg_lines.len() > shown {
        content.push(Line::from(Span::styled(
            format!("  … 参数共 {} 行", arg_lines.len()),
            Style::default().fg(theme.border_color),
        )));
    }

    if !app.confirmation_queue.is_empty() {
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(
            format!("还有 {} 个工具调用待确认", app.confirmation_queue.len()),
            Style::default().fg(theme.warning_color),
        )));
    }
    content.push(Line::from(""));

    let height = (content.len() as u16 + 4)
        .min(area.height.saturating_sub(4))
        .max(6)
        .min(area.height);
    let rect = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    );

    let position = if app.confirmation_queue.is_empty() {
        String::new()
    } else {
        format!(" 1/{}", app.confirmation_queue.len() + 1)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme.warning_color))
        .title(Span::styled(
            format!(" ⚠ 工具调用确认{} ", position),
            Style::default()
                .fg(theme.warning_color)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme.input_bg));

    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    let inner = Rect {
        x: rect.x + 2,
        y: rect.y + 1,
        width: rect.width.saturating_sub(4),
        height: rect.height.saturating_sub(2),
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(inner);

    let available = chunks[0].height as usize;
    let mut body: Vec<Line> = Vec::new();
    if content.len() > available {
        body.extend(content.into_iter().take(available.saturating_sub(1)));
        body.push(Line::from(Span::styled(
            "…",
            Style::default().fg(theme.border_color),
        )));
    } else {
        body.extend(content);
    }

    frame.render_widget(
        Paragraph::new(Text::from(body)).style(Style::default().bg(theme.input_bg)),
        chunks[0],
    );

    let buttons = Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            " [Y] 执行 ",
            Style::default()
                .fg(theme.success_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" / ", Style::default().fg(theme.border_color)),
        Span::styled(" [N] 跳过 ", Style::default().fg(theme.error_color)),
        Span::styled(" / ", Style::default().fg(theme.border_color)),
        Span::styled(" [Esc] 取消 ", Style::default().fg(theme.accent_dim)),
    ]);
    frame.render_widget(Paragraph::new(Text::from(vec![buttons])), chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::get_theme;

    #[test]
    fn render_inline_markdown_plain_text() {
        let theme = get_theme("default");
        let spans = render_inline_markdown("hello world", &theme);
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn render_inline_markdown_bold() {
        let theme = get_theme("default");
        let spans = render_inline_markdown("**bold**", &theme);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "bold");
    }

    #[test]
    fn render_inline_markdown_code() {
        let theme = get_theme("default");
        let spans = render_inline_markdown("`code`", &theme);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].content.contains("code"));
    }

    #[test]
    fn render_inline_markdown_italic() {
        let theme = get_theme("default");
        let spans = render_inline_markdown("*italic*", &theme);
        assert_eq!(spans[0].content, "italic");
    }

    #[test]
    fn render_inline_markdown_link() {
        let theme = get_theme("default");
        let spans = render_inline_markdown("[docs](https://example.com)", &theme);
        assert!(spans.iter().any(|span| span.content.contains("docs")));
        assert!(spans
            .iter()
            .any(|span| span.content.contains("https://example.com")));
    }

    #[test]
    fn ordered_list_item_parses_numbers() {
        assert_eq!(ordered_list_item("3. do it"), Some((3, "do it")));
        assert_eq!(ordered_list_item("not a list"), None);
    }

    #[test]
    fn format_tokens_scales() {
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(2_500_000), "2.50M");
    }

    #[test]
    fn input_rows_grow_for_multiline() {
        let config = crate::agent::config::AppConfig {
            api_base: "https://api.deepseek.com".into(),
            api_key: "sk-test".into(),
            default_model: "deepseek-chat".into(),
            default_pricing: Some(crate::agent::config::DefaultPricing {
                prompt_price_per_m: 2.0,
                completion_price_per_m: 8.0,
            }),
        };
        let agents = vec![crate::agent::config::AgentConfig {
            name: "测试".into(),
            description: "测试".into(),
            system_prompt: "测试".into(),
            model: "deepseek-chat".into(),
            tools: vec![],
        }];
        let mut app = crate::app::App::new(agents, config);
        assert_eq!(input_rows_for(&app, 100), 1);
        app.active_tab_mut().input_buffer = "a\nb\nc".into();
        assert_eq!(input_rows_for(&app, 100), 3);
    }
}

#[cfg(test)]
mod layout_smoke_tests {
    use super::*;
    use crate::agent::config::{AgentConfig, AppConfig, DefaultPricing};
    use crate::api::types::{Message, Role, ToolCall, ToolFunction};
    use crate::app::{App, Completion, CompletionItem, PendingConfirmation};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn app() -> App {
        let config = AppConfig {
            api_base: "https://api.deepseek.com".into(),
            api_key: "sk-test".into(),
            default_model: "deepseek-chat".into(),
            default_pricing: Some(DefaultPricing {
                prompt_price_per_m: 2.0,
                completion_price_per_m: 8.0,
            }),
        };
        let agents = vec![AgentConfig {
            name: "哪吒".into(),
            description: "测试 Agent".into(),
            system_prompt: "测试".into(),
            model: "deepseek-chat".into(),
            tools: vec![],
        }];
        App::new(agents, config)
    }

    fn draw(app: &mut App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, app))
            .expect("render should not panic");
        let buffer = terminal.backend().buffer();
        let area = *buffer.area();
        (0..area.height)
            .map(|y| {
                let mut line = String::new();
                let mut skip = 0u16;
                for x in 0..area.width {
                    if skip > 0 {
                        skip -= 1;
                        continue;
                    }
                    let symbol = buffer[(x, y)].symbol();
                    line.push_str(symbol);
                    let width = unicode_width::UnicodeWidthStr::width(symbol) as u16;
                    if width > 1 {
                        skip = width - 1;
                    }
                }
                line
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn welcome_renders_on_small_terminal() {
        let mut app = app();
        let screen = draw(&mut app, 40, 12);
        assert!(screen.contains("哪吒网络安全"));
        assert!(screen.contains("Enter 发送"));
    }

    #[test]
    fn messages_and_tool_cards_render() {
        let mut app = app();
        let user = Message::user("扫描 10.0.0.1");
        let mut assistant = Message::assistant("## 结果\n- 22 端口开放\n\n```\nssh\n```");
        assistant.tool_calls = Some(vec![ToolCall {
            id: "call_1".into(),
            call_type: "function".into(),
            function: ToolFunction {
                name: "run_nmap".into(),
                arguments: r#"{"target":"10.0.0.1"}"#.into(),
            },
        }]);
        app.active_tab_mut().messages = vec![
            user,
            assistant,
            Message::tool("call_1", "22/tcp open ssh\n80/tcp open http"),
        ];
        app.active_tab_mut().pending_tool_calls.insert(
            "call_1".into(),
            ToolCallStatus::Success {
                result: "22/tcp open ssh".into(),
            },
        );

        let screen = draw(&mut app, 100, 30);
        assert!(screen.contains("run_nmap"));
        assert!(screen.contains("22 端口开放"));
        // 结果已经显示在工具卡里，不应再重复一条「工具结果」消息。
        assert!(!screen.contains("工具结果"));
    }

    #[test]
    fn pending_tool_call_shows_waiting_label() {
        let mut app = app();
        let mut assistant = Message::assistant("");
        assistant.tool_calls = Some(vec![ToolCall {
            id: "call_pending".into(),
            call_type: "function".into(),
            function: ToolFunction {
                name: "read_file".into(),
                arguments: r#"{"path":"README.md"}"#.into(),
            },
        }]);
        app.active_tab_mut().messages = vec![assistant];
        app.active_tab_mut()
            .pending_tool_calls
            .insert("call_pending".into(), ToolCallStatus::Pending);

        let screen = draw(&mut app, 100, 30);
        assert!(screen.contains("等待确认"));
    }

    #[test]
    fn streaming_state_renders_thinking_indicator() {
        let mut app = app();
        let id = uuid::Uuid::new_v4();
        app.active_tab_mut().messages.push(Message {
            id,
            role: Role::Assistant,
            content: String::new(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
        app.active_tab_mut().is_streaming = true;
        app.active_tab_mut().request_started = Some(std::time::Instant::now());

        let screen = draw(&mut app, 100, 30);
        assert!(screen.contains("正在思考"));
        assert!(screen.contains("Esc 取消"));
    }

    #[test]
    fn command_palette_renders() {
        let mut app = app();
        app.command_palette_open = true;
        app.command_palette_input = "th".into();
        let screen = draw(&mut app, 100, 30);
        assert!(screen.contains("命令面板"));
        assert!(screen.contains("/theme"));
    }

    #[test]
    fn help_overlay_renders() {
        let mut app = app();
        app.help_open = true;
        let screen = draw(&mut app, 100, 30);
        assert!(screen.contains("快捷键与帮助"));
        assert!(screen.contains("Ctrl+J"));
    }

    #[test]
    fn completion_popup_renders() {
        let mut app = app();
        app.active_tab_mut().input_buffer = "@RE".into();
        app.active_tab_mut().input_cursor = 3;
        app.completion = Some(Completion {
            kind: CompletionKind::File,
            items: vec![CompletionItem {
                label: "README.md".into(),
                detail: String::new(),
            }],
            selected: 0,
        });

        let screen = draw(&mut app, 100, 30);
        assert!(screen.contains("@ 文件补全"));
        assert!(screen.contains("README.md"));
    }

    #[test]
    fn confirmation_dialog_renders() {
        let mut app = app();
        app.pending_confirmation = Some(PendingConfirmation {
            tab_id: 0,
            call_id: "call_1".into(),
            name: "read_file".into(),
            args: r#"{"path":"README.md"}"#.into(),
            description: "读取文件 README.md".into(),
        });

        let screen = draw(&mut app, 100, 30);
        assert!(screen.contains("工具调用确认"));
        assert!(screen.contains("read_file"));
        assert!(screen.contains("[Y] 执行"));
    }

    #[test]
    fn narrow_terminal_with_long_input_does_not_panic() {
        let mut app = app();
        app.active_tab_mut().input_buffer = "你好".repeat(80);
        app.active_tab_mut().input_cursor = app.active_tab().input_buffer.len();
        let screen = draw(&mut app, 50, 20);
        assert!(screen.contains("输入"));
    }
}

#[cfg(test)]
mod tool_result_tests {
    use super::strip_code_fence;

    #[test]
    fn strips_markdown_fences() {
        assert_eq!(
            strip_code_fence("```README.md\nline1\nline2\n```"),
            "line1\nline2"
        );
    }

    #[test]
    fn leaves_plain_result_untouched() {
        assert_eq!(strip_code_fence("  22/tcp open ssh  "), "22/tcp open ssh");
    }
}
