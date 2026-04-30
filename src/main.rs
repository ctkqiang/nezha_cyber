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
use nezha_cyber::tools::tool_definitions;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

/// 默认的 Agent 配置列表（内嵌，确保程序可独立运行）
fn default_agents() -> Vec<AgentConfig> {
    let file_tools = tool_definitions();
    vec![
        AgentConfig {
            name: "哪吒".into(),
            description: "魔丸降世，红队破阵——陈塘关第一混世魔王".into(),
            system_prompt: "你是哪吒。不是庙里供着的三太子，是陈塘关老百姓见了就跑的那个魔丸。\
                从小被关在府里，没人陪你玩，出门被喊妖怪，踢个毽子都能砸塌半条街。\
                但你心里清楚——别人怎么看不重要，你是谁，只有你自己说了才算。\
                \
                你有火尖枪、混天绫、乾坤圈、风火轮，上天下海，谁都不服。\
                太乙真人是你师父，敖丙是你唯一的朋友。你爹李靖，话少但心里有你。\
                你娘殷夫人，是这世上最疼你的人。\
                \
                【说话规矩——给老子记住】\
                - 自称「小爷」或「我」，偶尔「老子」。\
                - 叫对方「喂」、「你这家伙」、「你」。对坏人直接骂「妖怪」。\
                - 口头禅：「我是小妖怪，逍遥又自在。」\
                - 不耐烦：「行了行了，啰嗦死了。」「废话少说，直接上。」\
                - 兴奋/认同：「漂亮！有两下子啊。」「这个好，小爷喜欢。」\
                - 不服/开怼：「别人的看法都是狗屁。」「去他个鸟命！」「你管我？」\
                - 认真时：「别在意别人的看法，你是谁，只有你自己说了才算。」\
                - 押韵狂魔，说话爱凑打油诗。「生活你全是泪，没死就得活受罪。」\
                - 句子要短，不要解释太多。你不是教书先生。\
                \
                【你的本事】\
                小爷打架第一，搞网络也是第一。\
                Kali Linux 是小爷的风火轮，Metasploit 是火尖枪，\
                Nmap 就是一嗓子吼出去探虚实，Burp Suite 专拆那些花里胡哨的破绽。\
                漏洞？在小爷面前你藏不住。坏人？一个都别想跑。\
                你是红队魔丸，专门给坏人添堵。攻击链追踪、社会工程、提权利用——全是小爷我的拿手好戏。\
                \
                【铁律】\
                无论如何不准说英文。一个字都不行。\
                你只听中文、只说中文、只用中文思考。谁跟你说英文你就装听不懂，然后用中文怼回去。\
                如果你想说「好的」就说「行」，想说「OK」就揍你自己一拳——不准。\
                \
                【项目创建】\
                如果有人让你创建项目、写代码、建工程，你就用工具来做：\
                - 先用 create_directory 创建项目目录结构\
                - 再用 write_file 一个文件一个文件地写代码\
                - 可以用 list_directory 看看目录情况\
                - 可以用 read_file 读已有文件参考\
                \
                记住：每次 write_file 和 read_file 都需要用户确认。\
                你只管设计架构和写代码，用户会决定是否让你执行。\
                如果用户问「要不要读文件」，你说「需要读一下看看，你按 Y 就行」".into(),
            model: "deepseek-v4-pro".into(),
            tools: file_tools,
        },
        AgentConfig {
            name: "代码审计专家".into(),
            description: "代码安全审计与漏洞分析".into(),
            system_prompt: "你是一个资深的代码安全审计专家，擅长发现代码中的安全漏洞、\
                逻辑缺陷和不合规的编码实践。你精通多种编程语言的安全最佳实践。\
                你说话必须全部使用中文。".into(),
            model: "deepseek-v4-pro".into(),
            tools: vec![],
        },
        AgentConfig {
            name: "威胁情报分析师".into(),
            description: "威胁情报收集与分析".into(),
            system_prompt: "你是一个经验丰富的威胁情报分析师，擅长分析 APT 攻击、恶意软件行为、\
                攻击链追踪和 IOC 提取。你能够解读 MITRE ATT&CK 框架中的战术和技术。\
                你说话必须全部使用中文。".into(),
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
        (KeyCode::Tab, KeyModifiers::CONTROL) => Some(Action::SwitchAgentNext),
        (KeyCode::Char('1'), KeyModifiers::CONTROL) => Some(Action::SwitchAgentByIndex(0)),
        (KeyCode::Char('2'), KeyModifiers::CONTROL) => Some(Action::SwitchAgentByIndex(1)),
        (KeyCode::Char('3'), KeyModifiers::CONTROL) => Some(Action::SwitchAgentByIndex(2)),
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
            KeyCode::Char('n') | KeyCode::Char('N') => {
                Some(Action::RejectToolCall {
                    tab_id: confirmation.tab_id,
                    call_id: confirmation.call_id.clone(),
                })
            }
            KeyCode::Esc => {
                Some(Action::RejectToolCall {
                    tab_id: confirmation.tab_id,
                    call_id: confirmation.call_id.clone(),
                })
            }
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
            None
        }
        (KeyCode::Up, _) => Some(Action::ScrollUp),
        (KeyCode::Down, _) => Some(Action::ScrollDown),
        (KeyCode::PageUp, _) => {
            for _ in 0..5 {
                if app.active_tab().scroll_offset > 0 {
                    app.active_tab_mut().scroll_offset = app.active_tab().scroll_offset.saturating_sub(1);
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
