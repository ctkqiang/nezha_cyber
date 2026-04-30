//! 应用全局状态与 Elm Architecture 的 Update 层。
//!
//! `App` 结构体持有所有 UI 状态、对话历史、Agent 配置与 API 客户端。
//! `update()` 函数是纯状态转换函数，接收 `Action` 并返回修改后的 `App`。

use crate::action::Action;
use crate::agent::config::{AgentConfig, AppConfig, DefaultPricing};
use crate::api::deepseek::{DeepSeekClient, DeepSeekConfig};
use crate::api::types::{ApiMessage, Message, Pricing, Role, ToolCall, Usage};
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;

/// 焦点区域枚举，控制 Tab 键焦点切换
#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    ChatInput,
    Sidebar,
    MessageList,
    CommandPalette,
}

/// 工具调用状态，用于在 UI 中渲染卡片
#[derive(Debug, Clone)]
pub enum ToolCallStatus {
    Pending,
    Running,
    Success { result: String },
    Failed { error: String },
}

/// 单个标签页的会话状态
#[derive(Debug, Clone)]
pub struct Tab {
    pub id: usize,
    pub title: String,
    pub messages: Vec<Message>,
    pub model: String,
    pub agent_name: String,
    pub total_usage: Usage,
    pub input_buffer: String,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub thinking_ticks: u64,
    pub pending_tool_calls: HashMap<String, ToolCallStatus>,
}

impl Tab {
    pub fn new(
        id: usize,
        title: impl Into<String>,
        model: impl Into<String>,
        agent_name: impl Into<String>,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            messages: Vec::new(),
            model: model.into(),
            agent_name: agent_name.into(),
            total_usage: Usage::default(),
            input_buffer: String::new(),
            scroll_offset: 0,
            auto_scroll: true,
            thinking_ticks: 0,
            pending_tool_calls: HashMap::new(),
        }
    }
}

/// 应用全局状态
pub struct App {
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub sidebar_visible: bool,
    pub command_palette_open: bool,
    pub command_palette_input: String,
    pub focus: Focus,
    pub agents: Vec<AgentConfig>,
    pub client: DeepSeekClient,
    pub pricing: Pricing,
    pub theme: String,
    pub should_quit: bool,
    pub status_message: String,
}

impl App {
    /// 从配置创建应用实例
    pub fn new(agents: Vec<AgentConfig>, app_config: AppConfig) -> Self {
        let default_pricing = app_config.default_pricing.unwrap_or(DefaultPricing {
            prompt_price_per_m: 2.0,
            completion_price_per_m: 8.0,
        });

        let client = DeepSeekClient::new(DeepSeekConfig {
            api_base: app_config.api_base,
            api_key: app_config.api_key,
            model: app_config.default_model.clone(),
        });

        let default_model = app_config.default_model;
        let default_agent = agents.first().map(|a| a.name.clone()).unwrap_or_default();

        let first_tab = Tab::new(0, "会话 1", &default_model, &default_agent);

        Self {
            tabs: vec![first_tab],
            active_tab: 0,
            sidebar_visible: true,
            command_palette_open: false,
            command_palette_input: String::new(),
            focus: Focus::ChatInput,
            agents,
            client,
            pricing: Pricing {
                prompt_price_per_m: default_pricing.prompt_price_per_m,
                completion_price_per_m: default_pricing.completion_price_per_m,
            },
            theme: "default".into(),
            should_quit: false,
            status_message: String::new(),
        }
    }

    /// 获取当前活跃的标签页引用
    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    /// 获取当前活跃的标签页可变引用
    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab]
    }

    /// 发起流式对话请求 —— 构建消息列表并调用 DeepSeek API
    ///
    /// 将当前活跃标签页的对话历史（含系统提示）打包为 API 请求，
    /// 通过 `stream_chat` 后台任务发送，结果通过 `tx` 通道异步回传。
    pub fn send_stream_chat_request(&self, tx: UnboundedSender<Action>) {
        let tab = self.active_tab();
        let tab_id = self.active_tab;

        let agent = self.agents.iter().find(|a| a.name == tab.agent_name);

        let system_content = agent.map(|a| a.system_prompt.clone()).unwrap_or_default();

        let system_msg = ApiMessage {
            role: "system".to_string(),
            content: system_content,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        };

        let mut api_messages = vec![system_msg];
        for msg in &tab.messages {
            api_messages.push(ApiMessage::from(msg));
        }

        let tools = agent.map(|a| a.to_api_tools());

        self.client
            .stream_chat(tx, tab_id, api_messages, tools, Some(0.7));
    }
}

/// 核心状态转换函数 —— 接收 Action 并修改 App 状态
///
/// 返回 `true` 表示需要重新绘制界面。
/// 此函数为纯函数，不执行副作用（副作用在 main.rs 事件循环中触发）。
pub fn update(app: &mut App, action: Action) -> bool {
    match action {
        Action::Key(_event) => true,

        Action::Tick => {
            let tab = app.active_tab_mut();
            tab.thinking_ticks = tab.thinking_ticks.wrapping_add(1);
            false
        }

        Action::Resize(_w, _h) => true,

        Action::Shutdown => {
            app.should_quit = true;
            true
        }

        Action::SubmitMessage(content) => {
            let content = content.trim().to_string();
            if content.is_empty() {
                return false;
            }

            let user_msg = Message::user(&content);
            app.active_tab_mut().messages.push(user_msg);
            app.active_tab_mut().input_buffer.clear();
            app.status_message = format!("已发送: {}", &content[..content.len().min(40)]);

            true
        }

        Action::SwitchTab(index) => {
            if index < app.tabs.len() {
                app.active_tab = index;
                app.focus = Focus::ChatInput;
            }
            true
        }

        Action::NewTab => {
            let id = app.tabs.len();
            let default_model = app.client.model().to_string();
            let default_agent = app
                .agents
                .first()
                .map(|a| a.name.clone())
                .unwrap_or_default();
            let tab = Tab::new(
                id,
                format!("会话 {}", id + 1),
                &default_model,
                &default_agent,
            );
            app.tabs.push(tab);
            app.active_tab = app.tabs.len() - 1;
            app.focus = Focus::ChatInput;
            true
        }

        Action::CloseTab(index) => {
            if app.tabs.len() > 1 && index < app.tabs.len() {
                app.tabs.remove(index);
                if app.active_tab >= app.tabs.len() {
                    app.active_tab = app.tabs.len() - 1;
                }
            }
            true
        }

        Action::ToggleSidebar => {
            app.sidebar_visible = !app.sidebar_visible;
            true
        }

        Action::OpenCommandPalette => {
            app.command_palette_open = !app.command_palette_open;
            app.command_palette_input.clear();
            app.focus = if app.command_palette_open {
                Focus::CommandPalette
            } else {
                Focus::ChatInput
            };
            true
        }

        Action::StreamStart { tab_id, message_id } => {
            if let Some(tab) = app.tabs.get_mut(tab_id) {
                let assistant_msg = Message {
                    id: message_id,
                    role: Role::Assistant,
                    content: String::new(),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                };
                tab.messages.push(assistant_msg);
            }
            true
        }

        Action::StreamChunk {
            tab_id,
            message_id,
            chunk,
        } => {
            if let Some(tab) = app.tabs.get_mut(tab_id) {
                if let Some(msg) = tab.messages.iter_mut().find(|m| m.id == message_id) {
                    msg.content.push_str(&chunk);
                }
            }
            true
        }

        Action::StreamDone {
            tab_id,
            message_id,
            usage,
            ..
        } => {
            if let Some(tab) = app.tabs.get_mut(tab_id) {
                tab.total_usage.prompt_tokens += usage.prompt_tokens;
                tab.total_usage.completion_tokens += usage.completion_tokens;
                tab.total_usage.total_tokens += usage.total_tokens;

                if let Some(msg) = tab.messages.iter_mut().find(|m| m.id == message_id) {
                    if msg.content.is_empty() {
                        msg.content = "(空响应)".into();
                    }
                }
            }
            app.status_message = format!(
                "完成 - Token: {}P + {}C | 费用: ¥{:.4}",
                usage.prompt_tokens,
                usage.completion_tokens,
                usage.cost(&app.pricing)
            );
            true
        }

        Action::StreamError {
            tab_id,
            message_id,
            error,
        } => {
            if let Some(tab) = app.tabs.get_mut(tab_id) {
                if let Some(msg) = tab.messages.iter_mut().find(|m| m.id == message_id) {
                    msg.content = format!("**错误**: {}", error);
                }
            }
            app.status_message = format!("错误: {}", &error[..error.len().min(60)]);
            true
        }

        Action::ToolCallRequest {
            tab_id,
            call_id,
            name,
            args,
        } => {
            if let Some(tab) = app.tabs.get_mut(tab_id) {
                tab.pending_tool_calls
                    .insert(call_id.clone(), ToolCallStatus::Pending);
                let tool_call = ToolCall {
                    id: call_id.clone(),
                    call_type: "function".into(),
                    function: crate::api::types::ToolFunction {
                        name: name.clone(),
                        arguments: serde_json::to_string(&args).unwrap_or_default(),
                    },
                };
                if let Some(last_msg) = tab.messages.last_mut() {
                    if last_msg.role == Role::Assistant {
                        let mut calls = last_msg.tool_calls.take().unwrap_or_default();
                        calls.push(tool_call);
                        last_msg.tool_calls = Some(calls);
                    }
                }
            }
            true
        }

        Action::ToolCallResponse {
            tab_id,
            call_id,
            result,
        } => {
            if let Some(tab) = app.tabs.get_mut(tab_id) {
                tab.pending_tool_calls.insert(
                    call_id.clone(),
                    ToolCallStatus::Success {
                        result: result.clone(),
                    },
                );
                let tool_msg = Message::tool(&call_id, &result);
                tab.messages.push(tool_msg);
            }
            true
        }

        Action::ConfirmToolCall(confirmation) => {
            if let Some(tab) = app.tabs.get_mut(confirmation.tab_id) {
                tab.pending_tool_calls
                    .insert(confirmation.call_id.clone(), ToolCallStatus::Running);
            }
            true
        }

        Action::CommandPaletteSubmit(input) => {
            let input = input.trim().to_string();
            app.command_palette_open = false;
            app.command_palette_input.clear();
            app.focus = Focus::ChatInput;

            if input.starts_with("/model ") {
                let model = input[7..].trim().to_string();
                if let Some(tab) = app.tabs.get_mut(app.active_tab) {
                    tab.model = model.clone();
                }
                app.client.set_model(model);
            } else if input.starts_with("/theme ") {
                let theme = input[7..].trim().to_string();
                app.theme = theme;
            } else if input.starts_with("/agent ") {
                let agent = input[7..].trim().to_string();
                if let Some(tab) = app.tabs.get_mut(app.active_tab) {
                    if app.agents.iter().any(|a| a.name == agent) {
                        tab.agent_name = agent.clone();
                        app.status_message = format!("Agent: {}", agent);
                    }
                }
            } else if input == "/new" {
                let id = app.tabs.len();
                let default_model = app.client.model().to_string();
                let default_agent = app
                    .agents
                    .first()
                    .map(|a| a.name.clone())
                    .unwrap_or_default();
                let tab = Tab::new(
                    id,
                    format!("会话 {}", id + 1),
                    &default_model,
                    &default_agent,
                );
                app.tabs.push(tab);
                app.active_tab = app.tabs.len() - 1;
            } else if input == "/close" {
                let idx = app.active_tab;
                if app.tabs.len() > 1 && idx < app.tabs.len() {
                    app.tabs.remove(idx);
                    if app.active_tab >= app.tabs.len() {
                        app.active_tab = app.tabs.len() - 1;
                    }
                }
            } else if input == "/compact" || input == "/fork" || input == "/export" {
                app.status_message = format!("命令 '{}' 尚未实现", input);
            } else {
                app.status_message = format!("未知命令: {}", input);
            }
            true
        }

        Action::ScrollUp => {
            let tab = app.active_tab_mut();
            if tab.scroll_offset > 0 {
                tab.scroll_offset = tab.scroll_offset.saturating_sub(1);
                tab.auto_scroll = false;
            }
            true
        }

        Action::ScrollDown => {
            let tab = app.active_tab_mut();
            tab.scroll_offset += 1;
            true
        }

        Action::AutoScroll => {
            let tab = app.active_tab_mut();
            tab.auto_scroll = true;
            tab.scroll_offset = 0;
            true
        }

        Action::ChangeTheme(theme) => {
            app.theme = theme;
            true
        }

        Action::ChangeModel { tab_id, model } => {
            if let Some(tab) = app.tabs.get_mut(tab_id) {
                tab.model = model.clone();
            }
            app.client.set_model(model);
            true
        }

        Action::SwitchAgent { tab_id, agent_name } => {
            if let Some(tab) = app.tabs.get_mut(tab_id) {
                if app.agents.iter().any(|a| a.name == agent_name) {
                    tab.agent_name = agent_name.clone();
                    app.status_message = format!("已切换到: {}", agent_name);
                }
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ToolCallConfirmation;
    use crate::agent::config::{AppConfig, DefaultPricing};

    fn test_app_config() -> AppConfig {
        AppConfig {
            api_base: "https://api.deepseek.com/v1".into(),
            api_key: "sk-test".into(),
            default_model: "deepseek-chat".into(),
            default_pricing: Some(DefaultPricing {
                prompt_price_per_m: 2.0,
                completion_price_per_m: 8.0,
            }),
        }
    }

    fn test_agents() -> Vec<AgentConfig> {
        vec![AgentConfig {
            name: "测试Agent".into(),
            description: "测试用".into(),
            system_prompt: "你是一个测试助手".into(),
            model: "deepseek-chat".into(),
            tools: vec![],
        }]
    }

    fn make_app() -> App {
        App::new(test_agents(), test_app_config())
    }

    #[test]
    fn app_new_creates_one_tab() {
        let app = make_app();
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.active_tab, 0);
    }

    #[test]
    fn app_new_sidebar_visible() {
        let app = make_app();
        assert!(app.sidebar_visible);
    }

    #[test]
    fn app_new_command_palette_closed() {
        let app = make_app();
        assert!(!app.command_palette_open);
    }

    #[test]
    fn app_new_focus_is_chat_input() {
        let app = make_app();
        assert_eq!(app.focus, Focus::ChatInput);
    }

    #[test]
    fn app_new_should_not_quit() {
        let app = make_app();
        assert!(!app.should_quit);
    }

    #[test]
    fn update_shutdown_sets_should_quit() {
        let mut app = make_app();
        let result = update(&mut app, Action::Shutdown);
        assert!(result);
        assert!(app.should_quit);
    }

    #[test]
    fn update_submit_message_appends_user_msg() {
        let mut app = make_app();
        let initial_count = app.active_tab().messages.len();
        let result = update(&mut app, Action::SubmitMessage("你好世界".into()));
        assert!(result);
        assert_eq!(app.active_tab().messages.len(), initial_count + 1);
        let last_msg = app.active_tab().messages.last().unwrap();
        assert_eq!(last_msg.role, Role::User);
        assert_eq!(last_msg.content, "你好世界");
    }

    #[test]
    fn update_submit_message_clears_input_buffer() {
        let mut app = make_app();
        app.active_tab_mut().input_buffer = "old text".into();
        update(&mut app, Action::SubmitMessage("新消息".into()));
        assert!(app.active_tab().input_buffer.is_empty());
    }

    #[test]
    fn update_submit_message_trims_whitespace() {
        let mut app = make_app();
        update(&mut app, Action::SubmitMessage("  你好  ".into()));
        let last_msg = app.active_tab().messages.last().unwrap();
        assert_eq!(last_msg.content, "你好");
    }

    #[test]
    fn update_submit_empty_message_does_nothing() {
        let mut app = make_app();
        let initial_count = app.active_tab().messages.len();
        let result = update(&mut app, Action::SubmitMessage("   ".into()));
        assert!(!result);
        assert_eq!(app.active_tab().messages.len(), initial_count);
    }

    #[test]
    fn update_new_tab_adds_tab_and_switches() {
        let mut app = make_app();
        assert_eq!(app.tabs.len(), 1);
        update(&mut app, Action::NewTab);
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab, 1);
        assert_eq!(app.focus, Focus::ChatInput);
    }

    #[test]
    fn update_new_tab_increments_title() {
        let mut app = make_app();
        update(&mut app, Action::NewTab);
        assert_eq!(app.tabs[1].title, "会话 2");
        update(&mut app, Action::NewTab);
        assert_eq!(app.tabs[2].title, "会话 3");
    }

    #[test]
    fn update_close_tab_removes_tab() {
        let mut app = make_app();
        update(&mut app, Action::NewTab);
        assert_eq!(app.tabs.len(), 2);
        update(&mut app, Action::CloseTab(1));
        assert_eq!(app.tabs.len(), 1);
    }

    #[test]
    fn update_close_tab_adjusts_active_index() {
        let mut app = make_app();
        update(&mut app, Action::NewTab);
        update(&mut app, Action::NewTab);
        assert_eq!(app.active_tab, 2);
        update(&mut app, Action::CloseTab(2));
        assert_eq!(app.active_tab, 1);
    }

    #[test]
    fn update_close_last_tab_is_ignored() {
        let mut app = make_app();
        update(&mut app, Action::CloseTab(0));
        assert_eq!(app.tabs.len(), 1);
    }

    #[test]
    fn update_switch_tab_to_valid_index() {
        let mut app = make_app();
        update(&mut app, Action::NewTab);
        update(&mut app, Action::NewTab);
        update(&mut app, Action::SwitchTab(1));
        assert_eq!(app.active_tab, 1);
    }

    #[test]
    fn update_switch_tab_to_invalid_index_is_ignored() {
        let mut app = make_app();
        update(&mut app, Action::SwitchTab(99));
        assert_eq!(app.active_tab, 0);
    }

    #[test]
    fn update_toggle_sidebar_flips_visibility() {
        let mut app = make_app();
        assert!(app.sidebar_visible);
        update(&mut app, Action::ToggleSidebar);
        assert!(!app.sidebar_visible);
        update(&mut app, Action::ToggleSidebar);
        assert!(app.sidebar_visible);
    }

    #[test]
    fn update_open_command_palette_toggles() {
        let mut app = make_app();
        assert!(!app.command_palette_open);
        update(&mut app, Action::OpenCommandPalette);
        assert!(app.command_palette_open);
        assert_eq!(app.focus, Focus::CommandPalette);
        update(&mut app, Action::OpenCommandPalette);
        assert!(!app.command_palette_open);
        assert_eq!(app.focus, Focus::ChatInput);
    }

    #[test]
    fn update_change_theme_updates_theme_field() {
        let mut app = make_app();
        assert_eq!(app.theme, "default");
        update(&mut app, Action::ChangeTheme("daylight".into()));
        assert_eq!(app.theme, "daylight");
    }

    #[test]
    fn update_change_model_updates_tab_and_client() {
        let mut app = make_app();
        let _old_model = app.active_tab().model.clone();
        update(
            &mut app,
            Action::ChangeModel {
                tab_id: 0,
                model: "deepseek-reasoner".into(),
            },
        );
        assert_eq!(app.tabs[0].model, "deepseek-reasoner");
        assert_eq!(app.client.model(), "deepseek-reasoner");
    }

    #[test]
    fn update_stream_start_appends_empty_assistant_msg() {
        let mut app = make_app();
        let msg_id = uuid::Uuid::new_v4();
        let initial_count = app.active_tab().messages.len();
        update(
            &mut app,
            Action::StreamStart {
                tab_id: 0,
                message_id: msg_id,
            },
        );
        assert_eq!(app.active_tab().messages.len(), initial_count + 1);
        let last = app.active_tab().messages.last().unwrap();
        assert_eq!(last.id, msg_id);
        assert_eq!(last.role, Role::Assistant);
        assert_eq!(last.content, "");
    }

    #[test]
    fn update_stream_chunk_appends_content() {
        let mut app = make_app();
        let msg_id = uuid::Uuid::new_v4();
        update(
            &mut app,
            Action::StreamStart {
                tab_id: 0,
                message_id: msg_id,
            },
        );
        update(
            &mut app,
            Action::StreamChunk {
                tab_id: 0,
                message_id: msg_id,
                chunk: "你好".into(),
            },
        );
        update(
            &mut app,
            Action::StreamChunk {
                tab_id: 0,
                message_id: msg_id,
                chunk: "世界".into(),
            },
        );
        let msg = app.tabs[0]
            .messages
            .iter()
            .find(|m| m.id == msg_id)
            .unwrap();
        assert_eq!(msg.content, "你好世界");
    }

    #[test]
    fn update_stream_done_accumulates_usage() {
        let mut app = make_app();
        let msg_id = uuid::Uuid::new_v4();
        update(
            &mut app,
            Action::StreamStart {
                tab_id: 0,
                message_id: msg_id,
            },
        );
        let usage = Usage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };
        update(
            &mut app,
            Action::StreamDone {
                tab_id: 0,
                message_id: msg_id,
                content: "完成".into(),
                usage: usage.clone(),
            },
        );
        assert_eq!(app.tabs[0].total_usage.prompt_tokens, 100);
        assert_eq!(app.tabs[0].total_usage.completion_tokens, 50);
    }

    #[test]
    fn update_stream_done_empty_content_gets_placeholder() {
        let mut app = make_app();
        let msg_id = uuid::Uuid::new_v4();
        update(
            &mut app,
            Action::StreamStart {
                tab_id: 0,
                message_id: msg_id,
            },
        );
        update(
            &mut app,
            Action::StreamDone {
                tab_id: 0,
                message_id: msg_id,
                content: "".into(),
                usage: Usage::default(),
            },
        );
        let msg = app.tabs[0]
            .messages
            .iter()
            .find(|m| m.id == msg_id)
            .unwrap();
        assert_eq!(msg.content, "(空响应)");
    }

    #[test]
    fn update_stream_error_sets_error_content() {
        let mut app = make_app();
        let msg_id = uuid::Uuid::new_v4();
        update(
            &mut app,
            Action::StreamStart {
                tab_id: 0,
                message_id: msg_id,
            },
        );
        update(
            &mut app,
            Action::StreamError {
                tab_id: 0,
                message_id: msg_id,
                error: "连接超时".into(),
            },
        );
        let msg = app.tabs[0]
            .messages
            .iter()
            .find(|m| m.id == msg_id)
            .unwrap();
        assert!(msg.content.contains("连接超时"));
    }

    #[test]
    fn update_tool_call_request_adds_pending_status() {
        let mut app = make_app();
        let msg_id = uuid::Uuid::new_v4();
        update(
            &mut app,
            Action::StreamStart {
                tab_id: 0,
                message_id: msg_id,
            },
        );
        update(
            &mut app,
            Action::ToolCallRequest {
                tab_id: 0,
                call_id: "call_001".into(),
                name: "run_nmap".into(),
                args: serde_json::json!({"target": "192.168.1.1"}),
            },
        );
        assert!(app.tabs[0].pending_tool_calls.contains_key("call_001"));
    }

    #[test]
    fn update_tool_call_response_adds_tool_message() {
        let mut app = make_app();
        let msg_id = uuid::Uuid::new_v4();
        update(
            &mut app,
            Action::StreamStart {
                tab_id: 0,
                message_id: msg_id,
            },
        );
        let initial_count = app.tabs[0].messages.len();
        update(
            &mut app,
            Action::ToolCallResponse {
                tab_id: 0,
                call_id: "call_002".into(),
                result: "扫描完成: 22端口开放".into(),
            },
        );
        assert_eq!(app.tabs[0].messages.len(), initial_count + 1);
        let last = app.tabs[0].messages.last().unwrap();
        assert_eq!(last.role, Role::Tool);
        assert_eq!(last.tool_call_id, Some("call_002".into()));
    }

    #[test]
    fn update_confirm_tool_call_sets_running() {
        let mut app = make_app();
        let confirmation = ToolCallConfirmation {
            tab_id: 0,
            call_id: "call_003".into(),
            name: "test_tool".into(),
            args: serde_json::json!({}),
        };
        app.tabs[0]
            .pending_tool_calls
            .insert("call_003".into(), ToolCallStatus::Pending);
        update(&mut app, Action::ConfirmToolCall(confirmation));
        let status = app.tabs[0].pending_tool_calls.get("call_003").unwrap();
        assert!(matches!(status, ToolCallStatus::Running));
    }

    #[test]
    fn update_tick_returns_false() {
        let mut app = make_app();
        assert!(!update(&mut app, Action::Tick));
    }

    #[test]
    fn update_resize_returns_true() {
        let mut app = make_app();
        assert!(update(&mut app, Action::Resize(80, 24)));
    }

    #[test]
    fn active_tab_returns_correct_tab() {
        let app = make_app();
        assert_eq!(app.active_tab().id, 0);
    }

    #[test]
    fn active_tab_mut_allows_modification() {
        let mut app = make_app();
        app.active_tab_mut().input_buffer.push('a');
        assert_eq!(app.active_tab().input_buffer, "a");
    }

    #[test]
    fn update_scroll_up_decrements_offset() {
        let mut app = make_app();
        app.active_tab_mut().scroll_offset = 5;
        app.active_tab_mut().auto_scroll = true;
        update(&mut app, Action::ScrollUp);
        assert_eq!(app.active_tab().scroll_offset, 4);
        assert!(!app.active_tab().auto_scroll);
    }

    #[test]
    fn update_scroll_up_at_zero_stays_at_zero() {
        let mut app = make_app();
        app.active_tab_mut().scroll_offset = 0;
        update(&mut app, Action::ScrollUp);
        assert_eq!(app.active_tab().scroll_offset, 0);
    }

    #[test]
    fn update_scroll_down_increments_offset() {
        let mut app = make_app();
        update(&mut app, Action::ScrollDown);
        assert_eq!(app.active_tab().scroll_offset, 1);
    }

    #[test]
    fn update_auto_scroll_resets_state() {
        let mut app = make_app();
        app.active_tab_mut().scroll_offset = 10;
        app.active_tab_mut().auto_scroll = false;
        update(&mut app, Action::AutoScroll);
        assert_eq!(app.active_tab().scroll_offset, 0);
        assert!(app.active_tab().auto_scroll);
    }

    #[test]
    fn update_command_palette_submit_model_switch() {
        let mut app = make_app();
        update(
            &mut app,
            Action::CommandPaletteSubmit("/model deepseek-reasoner".into()),
        );
        assert!(!app.command_palette_open);
        assert_eq!(app.focus, Focus::ChatInput);
        assert_eq!(app.tabs[0].model, "deepseek-reasoner");
        assert_eq!(app.client.model(), "deepseek-reasoner");
    }

    #[test]
    fn update_command_palette_submit_theme_switch() {
        let mut app = make_app();
        update(
            &mut app,
            Action::CommandPaletteSubmit("/theme daylight".into()),
        );
        assert_eq!(app.theme, "daylight");
    }

    #[test]
    fn update_command_palette_submit_new_tab() {
        let mut app = make_app();
        assert_eq!(app.tabs.len(), 1);
        update(&mut app, Action::CommandPaletteSubmit("/new".into()));
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab, 1);
    }

    #[test]
    fn update_command_palette_submit_close_tab() {
        let mut app = make_app();
        update(&mut app, Action::NewTab);
        assert_eq!(app.tabs.len(), 2);
        update(&mut app, Action::CommandPaletteSubmit("/close".into()));
        assert_eq!(app.tabs.len(), 1);
    }

    #[test]
    fn update_command_palette_submit_unknown_command() {
        let mut app = make_app();
        update(
            &mut app,
            Action::CommandPaletteSubmit("/invalid_cmd".into()),
        );
        assert!(app.status_message.contains("未知命令"));
    }
}
