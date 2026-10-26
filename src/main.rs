//! 哪吒网络安全 TUI —— 程序入口与事件循环。
//!
//! 使用 Elm Architecture (Model-Update-View) 模式：
//! 1. 初始化终端 (raw mode + alternate screen + bracketed paste)
//! 2. 加载 Agent 配置
//! 3. 创建 App 状态与 mpsc 通道
//! 4. 进入主循环：读取事件 → update 状态 → 按需重绘界面
//!
//! 交互要点：
//! - 输入框支持光标移动、按词跳转、多行编辑与历史重放
//! - `Enter` 发送、`Ctrl+J` / `Alt+Enter` 换行
//! - `Esc` 取消生成、关闭弹窗；`F1` 帮助；`F2` 切换鼠标捕获
//! - 仅在有变化或动画时重绘，空闲时几乎不占 CPU

use std::env;
use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::{
    cursor::SetCursorStyle,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
    },
};
use nezha_cyber::action::{Action, ToolCallConfirmation};
use nezha_cyber::agent::config::{AgentConfig, AgentsConfig, AppConfig};
use nezha_cyber::agent::prompts;
use nezha_cyber::app::{update as app_update, App, Focus};
use nezha_cyber::command;
use nezha_cyber::input;
use nezha_cyber::tools::{
    extract_file_text, parse_at_references, redteam_tool_definitions, scan_files_for_autocomplete,
    tool_definitions,
};
use nezha_cyber::ui::layout::{
    calculate, sidebar_agent_hit, tab_bar_hit, TabHit, SIDEBAR_WIDTH_PCT,
};
use nezha_cyber::ui::render;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Position, Rect};
use ratatui::Terminal;
use tokio::sync::mpsc;

/// 默认的 Agent 配置列表（内嵌，确保程序可独立运行）
/// 包含完整的红队技能体系：哪吒默认助手 + 各专业领域专家
fn default_agents() -> Vec<AgentConfig> {
    let file_tools = tool_definitions();
    let redteam_tools = redteam_tool_definitions();

    vec![
        AgentConfig {
            name: "哪吒".into(),
            description: "魔丸降世，红队破阵——陈塘关第一混世魔王".into(),
            system_prompt: prompts::NEZHA_REDTEAM_SYSTEM.into(),
            model: "deepseek-v4-pro".into(),
            tools: redteam_tools.clone(),
        },
        AgentConfig {
            name: "代码审计专家".into(),
            description: "源代码安全审计与漏洞分析".into(),
            system_prompt: prompts::CODE_AUDITOR_SYSTEM.into(),
            model: "deepseek-v4-pro".into(),
            tools: file_tools.clone(),
        },
        AgentConfig {
            name: "威胁情报分析师".into(),
            description: "威胁情报收集与 APT 攻击分析".into(),
            system_prompt: prompts::THREAT_INTEL_SYSTEM.into(),
            model: "deepseek-v4-pro".into(),
            tools: redteam_tools.clone(),
        },
        AgentConfig {
            name: "社会工程专家".into(),
            description: "钓鱼攻击与人员渗透".into(),
            system_prompt: prompts::SOCIAL_ENGINEERING_SYSTEM.into(),
            model: "deepseek-v4-pro".into(),
            tools: redteam_tools.clone(),
        },
        AgentConfig {
            name: "漏洞利用专家".into(),
            description: "漏洞研究与 Exploit 开发".into(),
            system_prompt: prompts::EXPLOIT_DEVELOPER_SYSTEM.into(),
            model: "deepseek-v4-pro".into(),
            tools: redteam_tools.clone(),
        },
        AgentConfig {
            name: "权限提升专家".into(),
            description: "本地提权与域控渗透".into(),
            system_prompt: prompts::PRIVILEGE_ESCALATION_SYSTEM.into(),
            model: "deepseek-v4-pro".into(),
            tools: redteam_tools.clone(),
        },
        AgentConfig {
            name: "横向移动专家".into(),
            description: "内网渗透与域环境攻击".into(),
            system_prompt: prompts::LATERAL_MOVEMENT_SYSTEM.into(),
            model: "deepseek-v4-pro".into(),
            tools: redteam_tools.clone(),
        },
        AgentConfig {
            name: "逆向分析专家".into(),
            description: "恶意软件逆向与沙箱分析".into(),
            system_prompt: prompts::MALWARE_ANALYSIS_SYSTEM.into(),
            model: "deepseek-v4-pro".into(),
            tools: redteam_tools.clone(),
        },
        AgentConfig {
            name: "红队指挥官".into(),
            description: "全程攻击规划与行动协调".into(),
            system_prompt: prompts::REDTEAM_COMMANDER_SYSTEM.into(),
            model: "deepseek-v4-pro".into(),
            tools: redteam_tools,
        },
    ]
}

/// 加载 Agent 配置 —— 优先从 agents.yaml 读取，失败则使用默认
fn load_agents() -> Vec<AgentConfig> {
    let agents_yaml_path = env::current_dir()
        .map(|p| p.join("agents.yaml"))
        .unwrap_or_default();

    if agents_yaml_path.exists() {
        match std::fs::read_to_string(&agents_yaml_path) {
            Ok(content) => match serde_yaml::from_str::<AgentsConfig>(&content) {
                Ok(config) => {
                    return config.agents;
                }
                Err(e) => {
                    eprintln!("解析 agents.yaml 失败: {}，使用默认配置", e);
                }
            },
            Err(e) => {
                eprintln!("读取 agents.yaml 失败: {}，使用默认配置", e);
            }
        }
    }

    default_agents()
}

/// 加载 AppConfig —— .env → config.yaml → 环境变量 → 默认值
fn load_app_config() -> AppConfig {
    let _ = dotenvy::dotenv();

    let config_path = env::current_dir()
        .map(|p| p.join("config.yaml"))
        .unwrap_or_default();

    let api_key_from_env = || {
        env::var("DEEPSEEK_TOKEN")
            .or_else(|_| env::var("DEEPSEEK_API_KEY"))
            .unwrap_or_default()
    };

    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(mut config) = serde_yaml::from_str::<AppConfig>(&content) {
                // config.yaml 可以只写想覆盖的字段，其余从环境变量与默认值补齐。
                if config.api_key.trim().is_empty() {
                    config.api_key = api_key_from_env();
                }
                if config.api_base.trim().is_empty() {
                    config.api_base = "https://api.deepseek.com".into();
                }
                if config.default_model.trim().is_empty() {
                    config.default_model = "deepseek-v4-pro".into();
                }
                return config;
            }
        }
    }

    AppConfig {
        api_key: api_key_from_env(),
        ..AppConfig::default()
    }
}

// ---------------------------------------------------------------------------
// 键盘处理
// ---------------------------------------------------------------------------

/// 全局快捷键（在输入框之外也生效）。
fn global_action(app: &App, key: &KeyEvent) -> Option<Action> {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('n') if control => Some(Action::NewTab),
        KeyCode::Char('k') if control => Some(Action::OpenCommandPalette),
        KeyCode::Char('b') if control => Some(Action::ToggleSidebar),
        KeyCode::Char('w') if control => Some(Action::CloseTab(app.active_tab)),
        KeyCode::Tab if control => Some(Action::SwitchAgentNext),
        KeyCode::Char(digit)
            if digit.is_ascii_digit() && (control || key.modifiers.contains(KeyModifiers::ALT)) =>
        {
            let value = digit.to_digit(10).unwrap_or(1);
            if value == 0 {
                None
            } else {
                Some(Action::SwitchAgentByIndex(value as usize - 1))
            }
        }
        KeyCode::PageUp if control => Some(Action::SwitchTab(previous_tab(app))),
        KeyCode::PageDown if control => Some(Action::SwitchTab(next_tab(app))),
        _ => None,
    }
}

fn previous_tab(app: &App) -> usize {
    if app.tabs.is_empty() {
        return 0;
    }
    if app.active_tab == 0 {
        app.tabs.len() - 1
    } else {
        app.active_tab - 1
    }
}

fn next_tab(app: &App) -> usize {
    if app.tabs.is_empty() {
        return 0;
    }
    (app.active_tab + 1) % app.tabs.len()
}

/// 处理键盘输入 —— 区分全局快捷键、文本编辑与弹窗。
fn process_key(app: &mut App, key: KeyEvent) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }

    // Ctrl+C 始终可用：生成中先取消，空闲时退出。
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if app.active_tab().is_streaming {
            return Some(Action::CancelStream);
        }
        return Some(Action::Shutdown);
    }

    // 帮助层：Esc / F1 / Enter 关闭，其余按键忽略，避免误触。
    if app.help_open {
        return if matches!(
            key.code,
            KeyCode::Esc | KeyCode::F(1) | KeyCode::Enter | KeyCode::Char('q')
        ) {
            Some(Action::ToggleHelp)
        } else {
            None
        };
    }

    // 工具确认框优先于输入框，避免 Y/N 被当成聊天内容。
    if let Some(confirmation) = app.pending_confirmation.clone() {
        return match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                Some(Action::ConfirmToolCall(ToolCallConfirmation {
                    tab_id: confirmation.tab_id,
                    call_id: confirmation.call_id,
                    name: confirmation.name,
                    args: confirmation.args,
                }))
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                Some(Action::RejectToolCall {
                    tab_id: confirmation.tab_id,
                    call_id: confirmation.call_id,
                })
            }
            _ => None,
        };
    }

    // 命令面板独占键盘，Ctrl+N / Ctrl+P 在面板内用于上下移动。
    if app.command_palette_open {
        return process_command_palette_key(app, key);
    }

    if key.code == KeyCode::F(1) {
        return Some(Action::ToggleHelp);
    }

    if key.code == KeyCode::Esc {
        if app.active_tab().is_streaming {
            return Some(Action::CancelStream);
        }
        if app.completion.is_some() {
            app.completion = None;
            return None;
        }
        if !app.active_tab().input_buffer.is_empty() {
            app.active_tab_mut().clear_input();
            return None;
        }
        app.status_message.clear();
        app.status_ttl = 0;
        return None;
    }

    if let Some(action) = global_action(app, &key) {
        return Some(action);
    }

    // 补全弹窗打开时，方向键与 Tab 用于选择而不是历史/滚动。
    if app.completion.is_some() {
        match key.code {
            KeyCode::Up => {
                move_completion(app, -1);
                return None;
            }
            KeyCode::Down => {
                move_completion(app, 1);
                return None;
            }
            KeyCode::Tab => {
                app.apply_completion();
                return None;
            }
            KeyCode::Enter
                if app
                    .completion
                    .as_ref()
                    .map(|completion| completion.kind == nezha_cyber::app::CompletionKind::File)
                    .unwrap_or(false) =>
            {
                app.apply_completion();
                return None;
            }
            KeyCode::Enter
                if app
                    .completion
                    .as_ref()
                    .map(|completion| completion.kind == nezha_cyber::app::CompletionKind::Command)
                    .unwrap_or(false) =>
            {
                let current = app.active_tab().input_buffer.trim().to_string();
                let is_exact = current.len() > 1 && command::find(&current).is_some();
                if !is_exact {
                    app.apply_completion();
                    return None;
                }
            }
            _ => {}
        }
    }

    // 发送 / 换行
    if key.code == KeyCode::Enter {
        if key.modifiers.contains(KeyModifiers::SHIFT) || key.modifiers.contains(KeyModifiers::ALT)
        {
            insert_newline(app);
            return None;
        }
        return submit_action(app);
    }
    if key.code == KeyCode::Char('j') && key.modifiers.contains(KeyModifiers::CONTROL) {
        insert_newline(app);
        return None;
    }

    match (key.code, key.modifiers) {
        (KeyCode::Tab, _) => {
            complete_at_cursor(app);
            None
        }
        (KeyCode::Char(ch), modifiers)
            if modifiers == KeyModifiers::NONE || modifiers == KeyModifiers::SHIFT =>
        {
            {
                let tab = app.active_tab_mut();
                input::insert_char(&mut tab.input_buffer, &mut tab.input_cursor, ch);
            }
            after_edit(app);
            None
        }
        (KeyCode::Backspace, modifiers) => {
            {
                let tab = app.active_tab_mut();
                if modifiers.contains(KeyModifiers::ALT)
                    || modifiers.contains(KeyModifiers::CONTROL)
                {
                    input::delete_word_backward(&mut tab.input_buffer, &mut tab.input_cursor);
                } else {
                    input::backspace(&mut tab.input_buffer, &mut tab.input_cursor);
                }
            }
            after_edit(app);
            None
        }
        (KeyCode::Delete, _) => {
            {
                let tab = app.active_tab_mut();
                input::delete_forward(&mut tab.input_buffer, &mut tab.input_cursor);
            }
            after_edit(app);
            None
        }
        (KeyCode::Left, modifiers) => {
            {
                let tab = app.active_tab_mut();
                if word_move(modifiers) {
                    input::move_word_left(&tab.input_buffer, &mut tab.input_cursor);
                } else {
                    input::move_left(&tab.input_buffer, &mut tab.input_cursor);
                }
            }
            after_edit(app);
            None
        }
        (KeyCode::Right, modifiers) => {
            {
                let tab = app.active_tab_mut();
                if word_move(modifiers) {
                    input::move_word_right(&tab.input_buffer, &mut tab.input_cursor);
                } else {
                    input::move_right(&tab.input_buffer, &mut tab.input_cursor);
                }
            }
            after_edit(app);
            None
        }
        (KeyCode::Home, modifiers) if modifiers.contains(KeyModifiers::ALT) => {
            Some(Action::ScrollToTop)
        }
        (KeyCode::End, modifiers) if modifiers.contains(KeyModifiers::ALT) => {
            Some(Action::AutoScroll)
        }
        (KeyCode::Home, modifiers) => {
            {
                let tab = app.active_tab_mut();
                if modifiers.contains(KeyModifiers::CONTROL) {
                    tab.input_cursor = 0;
                } else {
                    input::line_start(&tab.input_buffer, &mut tab.input_cursor);
                }
            }
            after_edit(app);
            None
        }
        (KeyCode::End, modifiers) => {
            {
                let tab = app.active_tab_mut();
                if modifiers.contains(KeyModifiers::CONTROL) {
                    tab.input_cursor = tab.input_buffer.len();
                } else {
                    input::line_end(&tab.input_buffer, &mut tab.input_cursor);
                }
            }
            after_edit(app);
            None
        }
        (KeyCode::Char('a'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            {
                let tab = app.active_tab_mut();
                input::line_start(&tab.input_buffer, &mut tab.input_cursor);
            }
            after_edit(app);
            None
        }
        (KeyCode::Char('e'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            {
                let tab = app.active_tab_mut();
                input::line_end(&tab.input_buffer, &mut tab.input_cursor);
            }
            after_edit(app);
            None
        }
        (KeyCode::Char('u'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            app.active_tab_mut().clear_input();
            app.completion = None;
            None
        }
        (KeyCode::Char('d'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            if app.active_tab().input_buffer.is_empty() {
                Some(Action::Shutdown)
            } else {
                {
                    let tab = app.active_tab_mut();
                    input::delete_forward(&mut tab.input_buffer, &mut tab.input_cursor);
                }
                after_edit(app);
                None
            }
        }
        (KeyCode::Up, _) => {
            let tab = app.active_tab_mut();
            if tab.input_buffer.contains('\n') {
                input::move_up_line(&tab.input_buffer, &mut tab.input_cursor);
                None
            } else if tab.input_buffer.is_empty() || tab.history_index < tab.history.len() {
                if tab.history_prev() {
                    app.refresh_completion();
                }
                None
            } else {
                Some(Action::ScrollUp)
            }
        }
        (KeyCode::Down, _) => {
            let tab = app.active_tab_mut();
            if tab.input_buffer.contains('\n') {
                input::move_down_line(&tab.input_buffer, &mut tab.input_cursor);
                None
            } else if tab.history_index < tab.history.len() {
                tab.history_next();
                app.refresh_completion();
                None
            } else {
                Some(Action::ScrollDown)
            }
        }
        (KeyCode::PageUp, _) => Some(Action::ScrollPageUp),
        (KeyCode::PageDown, _) => Some(Action::ScrollPageDown),
        _ => None,
    }
}

fn word_move(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::ALT)
}

/// 命令面板键盘处理。
fn process_command_palette_key(app: &mut App, key: KeyEvent) -> Option<Action> {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => Some(Action::CommandPaletteSubmit(
            app.command_palette_input.clone(),
        )),
        (KeyCode::Esc, _) => Some(Action::OpenCommandPalette),
        (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
            move_palette_selection(app, -1);
            None
        }
        (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
            move_palette_selection(app, 1);
            None
        }
        (KeyCode::Tab, _) => {
            let filtered = command::filter(&app.command_palette_input);
            if let Some(spec) = filtered.get(app.command_palette_selected) {
                app.command_palette_input = format!("{} ", spec.name);
                app.command_palette_selected = 0;
            }
            None
        }
        (KeyCode::Char(ch), modifiers)
            if modifiers == KeyModifiers::NONE || modifiers == KeyModifiers::SHIFT =>
        {
            app.command_palette_input.push(ch);
            app.command_palette_selected = 0;
            None
        }
        (KeyCode::Backspace, _) => {
            app.command_palette_input.pop();
            app.command_palette_selected = 0;
            None
        }
        (KeyCode::Char('u'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            app.command_palette_input.clear();
            app.command_palette_selected = 0;
            None
        }
        _ => None,
    }
}

fn move_palette_selection(app: &mut App, delta: isize) {
    let len = command::filter(&app.command_palette_input).len();
    if len == 0 {
        app.command_palette_selected = 0;
        return;
    }
    let current = app.command_palette_selected.min(len - 1) as isize;
    app.command_palette_selected = (current + delta).rem_euclid(len as isize) as usize;
}

fn move_completion(app: &mut App, delta: isize) {
    if let Some(completion) = app.completion.as_mut() {
        if completion.items.is_empty() {
            return;
        }
        let len = completion.items.len() as isize;
        let current = completion.selected.min(len as usize - 1) as isize;
        completion.selected = (current + delta).rem_euclid(len) as usize;
    }
}

/// 输入内容变化后的收尾：退出历史浏览状态并刷新补全。
fn after_edit(app: &mut App) {
    let history_len = app.active_tab().history.len();
    app.active_tab_mut().history_index = history_len;
    app.refresh_completion();
}

fn insert_newline(app: &mut App) {
    {
        let tab = app.active_tab_mut();
        input::insert_newline(&mut tab.input_buffer, &mut tab.input_cursor);
    }
    after_edit(app);
}

/// Tab 补全：当补全弹窗未建立时，做一次兜底补全。
fn complete_at_cursor(app: &mut App) {
    app.refresh_completion();
    if app.completion.is_some() {
        app.apply_completion();
        return;
    }

    // 兜底：直接找 @ 引用并补全唯一匹配 / 公共前缀
    let (text, cursor) = {
        let tab = app.active_tab();
        (tab.input_buffer.clone(), tab.input_cursor)
    };
    let Some(query) = input::active_at_query(&text, cursor) else {
        return;
    };
    let matches = scan_files_for_autocomplete(&query.prefix);
    if matches.is_empty() {
        return;
    }
    let completion = if matches.len() == 1 {
        matches[0].clone()
    } else {
        let common = input::common_prefix(&matches);
        if common.is_empty() || common == query.prefix {
            matches[0].clone()
        } else {
            common
        }
    };

    {
        let tab = app.active_tab_mut();
        input::apply_at_completion(
            &mut tab.input_buffer,
            &mut tab.input_cursor,
            &query,
            &completion,
        );
    }
    after_edit(app);
}

/// 提交当前输入：斜杠命令本地执行，其余内容带上 @ 附件后发送给模型。
fn submit_action(app: &mut App) -> Option<Action> {
    let raw = app.active_tab().input_buffer.clone();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.starts_with('/') {
        let command = trimmed.to_string();
        {
            let tab = app.active_tab_mut();
            tab.clear_input();
        }
        app.completion = None;
        return Some(Action::CommandPaletteSubmit(command));
    }

    if app.client.api_key().trim().is_empty() {
        app.set_error("未配置 DEEPSEEK_TOKEN · 在 .env 中设置后重启");
        return None;
    }

    let (cleaned, files) = parse_at_references(&raw);
    let mut final_input = cleaned.trim().to_string();

    if !files.is_empty() {
        let mut attachment_text = String::from("── 附件内容 ──\n");
        for file in &files {
            match extract_file_text(Path::new(file)) {
                Ok(text) => {
                    attachment_text.push_str(&format!("\n[{}]\n{}\n", file, text));
                }
                Err(error) => {
                    attachment_text.push_str(&format!("\n[{}] 读取失败: {}\n", file, error));
                    app.set_error(format!("{} · {}", file, error));
                }
            }
        }
        attachment_text.push_str("\n── 附件结束 ──\n\n");
        final_input = format!("{}{}", attachment_text, final_input);
    }

    if final_input.trim().is_empty() {
        return None;
    }
    Some(Action::SubmitMessage(final_input))
}

/// 粘贴文本时保留换行，不触发发送。
fn paste_into_input(app: &mut App, text: &str) {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    {
        let tab = app.active_tab_mut();
        input::insert_str(&mut tab.input_buffer, &mut tab.input_cursor, &normalized);
    }
    after_edit(app);
}

// ---------------------------------------------------------------------------
// 鼠标 / 终端
// ---------------------------------------------------------------------------

fn handle_mouse_click(app: &mut App, size: (u16, u16), column: u16, row: u16) -> bool {
    // 帮助层是模态的：点击任意位置关闭。
    if app.help_open {
        app_update(app, Action::ToggleHelp);
        return true;
    }
    // 命令面板与工具确认框不接受背景点击，避免误触。
    if app.command_palette_open || app.pending_confirmation.is_some() {
        return false;
    }

    let area = Rect::new(0, 0, size.0, size.1);
    let layout = calculate(
        area,
        app.sidebar_visible,
        app.command_palette_open,
        SIDEBAR_WIDTH_PCT,
    );

    if row == layout.tab_bar.y {
        if let Some(hit) = tab_bar_hit(&app.tabs, app.active_tab, layout.tab_bar, column) {
            match hit {
                TabHit::Select(index) => {
                    app_update(app, Action::SwitchTab(index));
                }
                TabHit::Close(index) => {
                    app_update(app, Action::CloseTab(index));
                }
            }
            return true;
        }
    }

    if let Some(sidebar) = layout.sidebar {
        if column >= sidebar.x && column < sidebar.x + sidebar.width {
            let active_agent = app
                .agents
                .iter()
                .position(|agent| agent.name == app.active_tab().agent_name)
                .unwrap_or(0);
            if let Some(index) = sidebar_agent_hit(app.agents.len(), active_agent, sidebar, row) {
                app_update(app, Action::SwitchAgentByIndex(index));
                return true;
            }
        }
    }

    if layout.input_area.contains(Position::new(column, row)) {
        app.focus = Focus::ChatInput;
        app.help_open = false;
        return true;
    }

    false
}

fn set_mouse_capture(enabled: bool) -> anyhow::Result<()> {
    let mut stdout = std::io::stdout();
    if enabled {
        execute!(stdout, EnableMouseCapture)?;
    } else {
        execute!(stdout, DisableMouseCapture)?;
    }
    Ok(())
}

fn restore_terminal() -> anyhow::Result<()> {
    let _ = disable_raw_mode();
    let mut stdout = std::io::stdout();
    let _ = execute!(
        stdout,
        DisableBracketedPaste,
        DisableMouseCapture,
        SetCursorStyle::DefaultUserShape,
        LeaveAlternateScreen
    );
    Ok(())
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        original(info);
    }));
}

/// 保存尚未入库的会话。
fn auto_save(app: &mut App) {
    let auto_save = app
        .memory
        .get_preference("auto_save")
        .ok()
        .flatten()
        .unwrap_or_else(|| "on_exit".into());
    if auto_save != "on_exit" && auto_save != "always" {
        return;
    }

    for tab in &app.tabs {
        if tab.messages.is_empty() || tab.conversation_id.is_some() {
            continue;
        }
        let messages: Vec<_> = tab
            .messages
            .iter()
            .filter(|message| message.role != nezha_cyber::api::types::Role::System)
            .cloned()
            .collect();
        let _ = app
            .memory
            .save_conversation(&tab.title, &tab.agent_name, &tab.model, &messages);
    }
    let _ = app.memory.trim_old_conversations();
}

fn print_help() {
    println!(
        "哪吒网络安全 —— DeepSeek 驱动的红队辅助 TUI

用法:
  nezha_cyber [选项]

选项:
  -h, --help       显示帮助
  -V, --version    显示版本
      --no-mouse   启动时关闭鼠标捕获（可直接用鼠标选中文本，F2 可切换）

配置:
  DEEPSEEK_TOKEN   必填，DeepSeek API Key（也可写入 .env）
  config.yaml      可选，应用配置
  agents.yaml      可选，自定义 Agent

常用快捷键:
  Enter 发送 / 执行命令        Ctrl+J / Alt+Enter 换行
  Ctrl+K 命令面板              F1 帮助
  Alt+1..9 / Ctrl+Tab 切换 Agent
  Ctrl+N 新建会话 / Ctrl+W 关闭会话
  Esc 取消生成 / 关闭弹窗      F2 切换鼠标捕获
  @ 引用文件（支持子目录）      / 斜杠命令"
    );
}

/// 程序入口 —— 初始化终端、加载配置、启动事件循环
fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return Ok(());
    }
    if args.iter().any(|arg| arg == "-V" || arg == "--version") {
        println!("nezha_cyber {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let no_mouse = args.iter().any(|arg| arg == "--no-mouse");

    dotenvy::dotenv().ok();

    let agents = load_agents();
    let app_config = load_app_config();

    install_panic_hook();
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
    if !no_mouse {
        execute!(stdout, EnableMouseCapture)?;
    }
    // 部分终端不支持标题/光标样式，失败不影响主流程
    let _ = execute!(
        stdout,
        SetTitle("哪吒网络安全 · 红队终端"),
        SetCursorStyle::SteadyBar
    );

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let rt = tokio::runtime::Runtime::new()?;
    let (tx, mut rx) = mpsc::unbounded_channel::<Action>();

    let mut app = App::new(agents, app_config);
    app.mouse_capture = !no_mouse;

    // 余额查询放到后台，先让界面出现，避免网络慢时白屏。
    {
        let client = app.client.clone();
        let balance_tx = tx.clone();
        rt.spawn(async move {
            let result = match client.check_balance().await {
                Ok(info) => {
                    let total: f64 = info
                        .balance_infos
                        .iter()
                        .filter_map(|balance| balance.total_balance.parse::<f64>().ok())
                        .sum();
                    if info.is_available {
                        Ok(total)
                    } else {
                        Err("账户不可用或余额不足".to_string())
                    }
                }
                Err(error) => Err(error),
            };
            let _ = balance_tx.send(Action::BalanceUpdated(result));
        });
    }

    let tick_duration = Duration::from_millis(100);
    let mut last_tick = Instant::now();
    let mut dirty = true;

    let result = rt.block_on(async {
        loop {
            let mut updated = false;
            while let Ok(action) = rx.try_recv() {
                if app_update(&mut app, action) {
                    updated = true;
                }
            }
            if updated {
                dirty = true;
            }

            if dirty {
                terminal.draw(|frame| render::render(frame, &mut app))?;
                dirty = false;
            }

            if app.should_quit {
                auto_save(&mut app);
                break Ok::<_, anyhow::Error>(());
            }

            let timeout = tick_duration.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) => {
                        if key.kind != KeyEventKind::Release && key.code == KeyCode::F(2) {
                            app.mouse_capture = !app.mouse_capture;
                            let enabled = app.mouse_capture;
                            set_mouse_capture(enabled)?;
                            app.set_status(if enabled {
                                "鼠标捕获已开启 · 可点击标签 / Agent"
                            } else {
                                "鼠标捕获已关闭 · 可用鼠标选中文本"
                            });
                            dirty = true;
                            continue;
                        }

                        let action = process_key(&mut app, key);
                        dirty = true;

                        if let Some(action) = action {
                            let is_submit = matches!(&action, Action::SubmitMessage(_));
                            let tool_tab_id = match &action {
                                Action::ConfirmToolCall(confirmation) => Some(confirmation.tab_id),
                                Action::RejectToolCall { tab_id, .. } => Some(*tab_id),
                                _ => None,
                            };

                            if app_update(&mut app, action) {
                                dirty = true;
                            }

                            if is_submit {
                                app.send_stream_chat_request(tx.clone());
                            }

                            // 工具执行完毕后自动把结果回传给模型，
                            // 让「思考 → 调用工具 → 总结」形成闭环。
                            // 注意：使用工具所属标签页，而不是当前活动标签页，
                            // 这样后台标签页的工具调用也能正确继续。
                            if let Some(tool_tab_id) = tool_tab_id {
                                let same_tab_pending = app
                                    .pending_confirmation
                                    .as_ref()
                                    .map(|confirmation| confirmation.tab_id == tool_tab_id)
                                    .unwrap_or(false)
                                    || app
                                        .confirmation_queue
                                        .iter()
                                        .any(|confirmation| confirmation.tab_id == tool_tab_id);
                                if !same_tab_pending {
                                    let (rounds, busy) = app
                                        .tabs
                                        .iter()
                                        .find(|tab| tab.id == tool_tab_id)
                                        .map(|tab| (tab.tool_rounds, tab.is_streaming))
                                        .unwrap_or((6, true));
                                    if rounds >= 6 {
                                        app.set_warning(
                                            "工具调用轮次过多 · 已停止自动继续，请手动确认下一步",
                                        );
                                    } else if !busy {
                                        if let Some(tab) =
                                            app.tabs.iter_mut().find(|tab| tab.id == tool_tab_id)
                                        {
                                            tab.tool_rounds += 1;
                                        }
                                        app.send_stream_chat_request_for(tool_tab_id, tx.clone());
                                    }
                                }
                            }
                        }
                    }
                    Event::Paste(text) => {
                        paste_into_input(&mut app, &text);
                        dirty = true;
                    }
                    Event::Resize(_width, _height) => {
                        app_update(&mut app, Action::Resize(0, 0));
                        dirty = true;
                    }
                    Event::Mouse(mouse) => match mouse.kind {
                        MouseEventKind::ScrollUp => {
                            for _ in 0..3 {
                                app_update(&mut app, Action::ScrollUp);
                            }
                            dirty = true;
                        }
                        MouseEventKind::ScrollDown => {
                            for _ in 0..3 {
                                app_update(&mut app, Action::ScrollDown);
                            }
                            dirty = true;
                        }
                        MouseEventKind::Down(MouseButton::Left) => {
                            let size = terminal.size()?;
                            if handle_mouse_click(
                                &mut app,
                                (size.width, size.height),
                                mouse.column,
                                mouse.row,
                            ) {
                                dirty = true;
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }

            if last_tick.elapsed() >= tick_duration {
                let animating = app.needs_animation();
                app_update(&mut app, Action::Tick);
                if animating || app.needs_animation() {
                    dirty = true;
                }
                last_tick = Instant::now();
            }
        }
    });

    restore_terminal()?;
    result
}
