//! 哪吒网络安全 TUI —— 程序入口与事件循环。
//!
//! 使用 Elm Architecture (Model-Update-View) 模式：
//! 1. 初始化终端 (raw mode + alternate screen)
//! 2. 加载 Agent 配置
//! 3. 创建 App 状态与 mpsc 通道
//! 4. 进入主循环：读取事件 → update 状态 → 重绘界面

use std::env;
use std::time::Duration;

use nezha_cyber::action::Action;
use nezha_cyber::agent::config::{AgentConfig, AgentsConfig, AppConfig};
use nezha_cyber::app::{App, update as app_update};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

/// 默认的 Agent 配置列表（内嵌，确保程序可独立运行）
fn default_agents() -> Vec<AgentConfig> {
    vec![
        AgentConfig {
            name: "红队渗透助手".into(),
            description: "辅助渗透测试、漏洞扫描与利用".into(),
            system_prompt: "你是一个专业的网络安全红队专家，精通渗透测试、漏洞挖掘、社会工程学攻击和防御。\
                你熟悉 Kali Linux、Metasploit、Nmap、Burp Suite 等工具的使用。\
                你的回答应当专业、准确、可操作。".into(),
            model: "deepseek-v4-pro".into(),
            tools: vec![],
        },
        AgentConfig {
            name: "代码审计专家".into(),
            description: "代码安全审计与漏洞分析".into(),
            system_prompt: "你是一个资深的代码安全审计专家，擅长发现代码中的安全漏洞、\
                逻辑缺陷和不合规的编码实践。你精通多种编程语言的安全最佳实践。".into(),
            model: "deepseek-v4-pro".into(),
            tools: vec![],
        },
        AgentConfig {
            name: "威胁情报分析师".into(),
            description: "威胁情报收集与分析".into(),
            system_prompt: "你是一个经验丰富的威胁情报分析师，擅长分析 APT 攻击、恶意软件行为、\
                攻击链追踪和 IOC 提取。你能够解读 MITRE ATT&CK 框架中的战术和技术。".into(),
            model: "deepseek-v4-pro".into(),
            tools: vec![],
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

/// 加载 AppConfig —— 优先从 config.yaml 读取，否则用环境变量
fn load_app_config() -> AppConfig {
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
        _ => None,
    }
}

/// 处理键盘输入 —— 区分命令快捷键与文本输入
fn process_key(app: &mut App, key: crossterm::event::KeyEvent) -> Option<Action> {
    if let Some(action) = key_event_to_action(&key) {
        return Some(action);
    }

    if key.kind == KeyEventKind::Release {
        return None;
    }

    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => {
            let input = app.active_tab().input_buffer.clone();
            if !input.trim().is_empty() {
                Some(Action::SubmitMessage(input))
            } else {
                None
            }
        }
        (KeyCode::Char(c), m) if m == KeyModifiers::NONE || m == KeyModifiers::SHIFT => {
            app.active_tab_mut().input_buffer.push(c);
            None
        }
        (KeyCode::Backspace, _) => {
            app.active_tab_mut().input_buffer.pop();
            None
        }
        (KeyCode::Char(' '), _) => {
            app.active_tab_mut().input_buffer.push(' ');
            None
        }
        (KeyCode::Esc, _) => {
            if app.command_palette_open {
                Some(Action::OpenCommandPalette)
            } else {
                None
            }
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
                app.status_message = format!("余额充足: {:.2}", total);
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
