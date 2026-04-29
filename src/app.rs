//! 应用全局状态与 Elm Architecture 的 Update 层。
//!
//! `App` 结构体持有所有 UI 状态、对话历史、Agent 配置与 API 客户端。
//! `update()` 函数是纯状态转换函数，接收 `Action` 并返回修改后的 `App`。

use crate::action::Action;
use crate::agent::config::{AgentConfig, AppConfig, DefaultPricing};
use crate::api::deepseek::{DeepSeekClient, DeepSeekConfig};
use crate::api::types::{Message, Pricing, Role, ToolCall, Usage};
use std::collections::HashMap;

/// 焦点区域枚举，控制 Tab 键焦点切换
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Focus {
    ChatInput,
    Sidebar,
    MessageList,
    CommandPalette,
}

/// 工具调用状态，用于在 UI 中渲染卡片
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ToolCallStatus {
    Pending,
    Running,
    Success { result: String },
    Failed { error: String },
}

/// 单个标签页的会话状态
#[derive(Debug, Clone)]
pub struct Tab {
    #[allow(dead_code)]
    pub id: usize,
    pub title: String,
    pub messages: Vec<Message>,
    pub model: String,
    pub agent_name: String,
    pub total_usage: Usage,
    pub input_buffer: String,
    #[allow(dead_code)]
    pub scroll_offset: usize,
    #[allow(dead_code)]
    pub auto_scroll: bool,
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
}

/// 核心状态转换函数 —— 接收 Action 并修改 App 状态
///
/// 返回 `true` 表示需要重新绘制界面。
/// 此函数为纯函数，不执行副作用（副作用在 main.rs 事件循环中触发）。
pub fn update(app: &mut App, action: Action) -> bool {
    match action {
        Action::Key(_event) => true,

        Action::Tick => false,

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
    }
}
