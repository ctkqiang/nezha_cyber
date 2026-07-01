//! 哪吒网络安全 TUI —— 程序入口与事件循环。
//!
//! 使用 Elm Architecture (Model-Update-View) 模式：
//! 1. 初始化终端 (raw mode + alternate screen)
//! 2. 加载 Agent 配置
//! 3. 创建 App 状态与 mpsc 通道
//! 4. 进入主循环：读取事件 → update 状态 → 重绘界面

use std::env;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use nezha_cyber::action::Action;
use nezha_cyber::agent::config::{AgentConfig, AgentsConfig, AppConfig};
use nezha_cyber::agent::prompts;
use nezha_cyber::app::{update as app_update, App};
use nezha_cyber::tools::{
    detect_at_cursor, extract_file_text, parse_at_references, redteam_tool_definitions,
    scan_files_for_autocomplete, tool_definitions,
};
use ratatui::backend::CrosstermBackend;
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

    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(config) = serde_yaml::from_str::<AppConfig>(&content) {
                return config;
            }
        }
    }

    let api_key = env::var("DEEPSEEK_TOKEN")
        .or_else(|_| env::var("DEEPSEEK_API_KEY"))
        .unwrap_or_default();
    let mut config = AppConfig::default();
    config.api_key = api_key;
    config
}

/// 将键盘事件转换为命令 Action
fn key_event_to_action(key: &crossterm::event::KeyEvent) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(Action::Shutdown),
        (KeyCode::Char('n'), KeyModifiers::CONTROL) => Some(Action::NewTab),
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => Some(Action::OpenCommandPalette),
        (KeyCode::Char('b'), KeyModifiers::CONTROL) => Some(Action::ToggleSidebar),
        (KeyCode::Tab, KeyModifiers::CONTROL) => Some(Action::SwitchAgentNext),
        (KeyCode::Char('1'), KeyModifiers::CONTROL) => Some(Action::SwitchAgentByIndex(0)),
        (KeyCode::Char('2'), KeyModifiers::CONTROL) => Some(Action::SwitchAgentByIndex(1)),
        (KeyCode::Char('3'), KeyModifiers::CONTROL) => Some(Action::SwitchAgentByIndex(2)),
        (KeyCode::Char('4'), KeyModifiers::CONTROL) => Some(Action::SwitchAgentByIndex(3)),
        (KeyCode::Char('5'), KeyModifiers::CONTROL) => Some(Action::SwitchAgentByIndex(4)),
        (KeyCode::Char('6'), KeyModifiers::CONTROL) => Some(Action::SwitchAgentByIndex(5)),
        (KeyCode::Char('7'), KeyModifiers::CONTROL) => Some(Action::SwitchAgentByIndex(6)),
        (KeyCode::Char('8'), KeyModifiers::CONTROL) => Some(Action::SwitchAgentByIndex(7)),
        (KeyCode::Char('9'), KeyModifiers::CONTROL) => Some(Action::SwitchAgentByIndex(8)),
        _ => None,
    }
}

/// 处理键盘输入 —— 区分命令快捷键与文本输入，命令面板模式下重定向输入
fn process_key(app: &mut App, key: crossterm::event::KeyEvent) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }

    if let Some(ref confirmation) = app.pending_confirmation {
        return match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let c = nezha_cyber::action::ToolCallConfirmation {
                    tab_id: confirmation.tab_id,
                    call_id: confirmation.call_id.clone(),
                    name: confirmation.name.clone(),
                    args: confirmation.args.clone(),
                };
                Some(Action::ConfirmToolCall(c))
            }
            KeyCode::Char('n') | KeyCode::Char('N') => Some(Action::RejectToolCall {
                tab_id: confirmation.tab_id,
                call_id: confirmation.call_id.clone(),
            }),
            KeyCode::Esc => Some(Action::RejectToolCall {
                tab_id: confirmation.tab_id,
                call_id: confirmation.call_id.clone(),
            }),
            _ => None,
        };
    }

    if let Some(action) = key_event_to_action(&key) {
        return Some(action);
    }

    if app.command_palette_open {
        return process_command_palette_key(app, key);
    }

    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => {
            let raw = app.active_tab().input_buffer.clone();
            if raw.trim().is_empty() {
                return None;
            }
            let (cleaned, files) = parse_at_references(&raw);
            let mut final_input = cleaned;
            if !files.is_empty() {
                let mut file_contents = String::from("── 附件内容 ──\n");
                for file in &files {
                    let path = std::path::Path::new(file);
                    match extract_file_text(path) {
                        Ok(text) => {
                            file_contents.push_str(&format!("\n[{}]\n{}\n", file, text));
                        }
                        Err(e) => {
                            file_contents.push_str(&format!("\n[{}] 读取失败: {}\n", file, e));
                            app.status_message = format!("{}: {}", file, e);
                        }
                    }
                }
                file_contents.push_str("── 附件结束 ──\n\n");
                final_input = format!("{}{}", file_contents, final_input);
            }
            Some(Action::SubmitMessage(final_input))
        }
        (KeyCode::Tab, _) => {
            let input = app.active_tab().input_buffer.clone();
            if let Some((prefix, quoted)) = detect_at_cursor(&input) {
                let matches = scan_files_for_autocomplete(&prefix);
                if matches.len() == 1 {
                    let completion = &matches[0];
                    let at_pos = input.rfind('@').unwrap();
                    let new_input = if quoted {
                        format!("{}@\"{}\"", &input[..at_pos], completion)
                    } else {
                        format!("{}@{}", &input[..at_pos], completion)
                    };
                    app.active_tab_mut().input_buffer = new_input;
                } else if matches.len() > 1 {
                    let list = matches
                        .iter()
                        .take(8)
                        .map(|m| m.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    app.status_message = format!("匹配: {}", list);
                }
            }
            None
        }
        (KeyCode::Char(c), m) if m == KeyModifiers::NONE || m == KeyModifiers::SHIFT => {
            app.active_tab_mut().input_buffer.push(c);
            update_at_status(app);
            None
        }
        (KeyCode::Backspace, _) => {
            app.active_tab_mut().input_buffer.pop();
            update_at_status(app);
            None
        }
        (KeyCode::Char(' '), _) => {
            app.active_tab_mut().input_buffer.push(' ');
            None
        }
        (KeyCode::Esc, _) => {
            if app.status_message.starts_with('@') || app.status_message.starts_with("匹配:") {
                app.status_message = String::new();
            }
            None
        }
        (KeyCode::Up, _) => Some(Action::ScrollUp),
        (KeyCode::Down, _) => Some(Action::ScrollDown),
        (KeyCode::PageUp, _) => {
            for _ in 0..5 {
                if app.active_tab().scroll_offset > 0 {
                    app.active_tab_mut().scroll_offset =
                        app.active_tab().scroll_offset.saturating_sub(1);
                }
            }
            app.active_tab_mut().auto_scroll = false;
            None
        }
        (KeyCode::PageDown, _) => {
            app.active_tab_mut().scroll_offset += 5;
            None
        }
        _ => None,
    }
}

/// 更新状态栏：检测输入中的 @文件引用
fn update_at_status(app: &mut App) {
    let input = &app.active_tab().input_buffer;
    if let Some(at_pos) = input.rfind('@') {
        let after = &input[at_pos..];
        if after.len() > 1 {
            app.status_message = format!("@文件: {} — 按 Tab 自动补全", after);
        }
    }
}

/// 命令面板键盘处理 —— 输入搜索关键词，Enter 提交，Esc 关闭
fn process_command_palette_key(app: &mut App, key: crossterm::event::KeyEvent) -> Option<Action> {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => {
            let input = app.command_palette_input.clone();
            if !input.trim().is_empty() {
                Some(Action::CommandPaletteSubmit(input))
            } else {
                Some(Action::OpenCommandPalette)
            }
        }
        (KeyCode::Esc, _) => Some(Action::OpenCommandPalette),
        (KeyCode::Char(c), m) if m == KeyModifiers::NONE || m == KeyModifiers::SHIFT => {
            app.command_palette_input.push(c);
            None
        }
        (KeyCode::Backspace, _) => {
            app.command_palette_input.pop();
            None
        }
        (KeyCode::Char(' '), _) => {
            app.command_palette_input.push(' ');
            None
        }
        _ => None,
    }
}

/// 清理终端状态 —— 退出 raw mode 并恢复屏幕
fn cleanup_terminal() -> anyhow::Result<()> {
    disable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, crossterm::event::DisableMouseCapture)?;
    execute!(stdout, LeaveAlternateScreen)?;
    Ok(())
}

/// 程序入口 —— 初始化终端、加载配置、查询余额、启动事件循环
fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let agents = load_agents();
    let app_config = load_app_config();

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    execute!(stdout, crossterm::event::EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let rt = tokio::runtime::Runtime::new()?;
    let (tx, mut rx) = mpsc::unbounded_channel::<Action>();

    let mut app = App::new(agents, app_config);

    let balance_result = rt.block_on(app.client.check_balance());
    match balance_result {
        Ok(info) => {
            if info.is_available {
                let total: f64 = info
                    .balance_infos
                    .iter()
                    .filter_map(|b| b.total_balance.parse::<f64>().ok())
                    .sum();
                app.remaining_balance = total;
                app.status_message = format!("余额: ¥{:.2}", total);
            } else {
                app.status_message = "余额不足，请充值".into();
            }
        }
        Err(e) => {
            app.status_message = format!("余额查询失败: {}", &e[..e.len().min(40)]);
        }
    }

    let tick_duration = Duration::from_millis(100);
    let mut last_tick = std::time::Instant::now();

    let result = rt.block_on(async {
        loop {
            while let Ok(action) = rx.try_recv() {
                app_update(&mut app, action);
            }

            terminal.draw(|frame| nezha_cyber::ui::render::render(frame, &app))?;

            if app.should_quit {
                let auto_save = app
                    .memory
                    .get_preference("auto_save")
                    .unwrap_or(None)
                    .unwrap_or_else(|| "on_exit".into());
                if auto_save == "on_exit" || auto_save == "always" {
                    let tab = app.active_tab();
                    if !tab.messages.is_empty() {
                        let _ = app.memory.save_conversation(
                            &tab.title,
                            &tab.agent_name,
                            &tab.model,
                            &tab.messages,
                        );
                    }
                    let _ = app.memory.trim_old_conversations();
                }
                break Ok::<_, anyhow::Error>(());
            }

            let timeout = tick_duration.saturating_sub(last_tick.elapsed());

            if event::poll(timeout)? {
                let ev = event::read()?;

                match ev {
                    Event::Key(key) => {
                        if let Some(action) = process_key(&mut app, key) {
                            let is_submit = matches!(&action, Action::SubmitMessage(_));
                            app_update(&mut app, action);
                            if is_submit {
                                app.send_stream_chat_request(tx.clone());
                            }
                        }
                    }
                    Event::Resize(w, h) => {
                        app_update(&mut app, Action::Resize(w, h));
                    }
                    Event::Mouse(mouse) => match mouse.kind {
                        MouseEventKind::ScrollUp => {
                            app_update(&mut app, Action::ScrollUp);
                        }
                        MouseEventKind::ScrollDown => {
                            app_update(&mut app, Action::ScrollDown);
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }

            if last_tick.elapsed() >= tick_duration {
                app_update(&mut app, Action::Tick);
                last_tick = std::time::Instant::now();
            }
        }
    });

    cleanup_terminal()?;

    result
}
