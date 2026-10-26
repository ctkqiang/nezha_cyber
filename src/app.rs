//! 应用全局状态与 Elm Architecture 的 Update 层。
//!
//! `App` 结构体持有所有 UI 状态、对话历史、Agent 配置与 API 客户端。
//! `update()` 函数是纯状态转换函数，接收 `Action` 并返回修改后的 `App`。

use crate::action::Action;
use crate::agent::config::{AgentConfig, AppConfig, DefaultPricing};
use crate::api::deepseek::{DeepSeekClient, DeepSeekConfig};
use crate::api::types::{ApiMessage, Message, Pricing, Role, ToolCall, Usage};
use crate::command::{self, CommandSpec};
use crate::input;
use crate::persistence::MemoryStore;
use crate::text;
use crate::tools::{execute_tool, scan_files_for_autocomplete};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;

/// 焦点区域枚举，控制 Tab 键焦点切换
#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    ChatInput,
    Sidebar,
    MessageList,
    CommandPalette,
}

/// 状态栏消息等级，决定图标、颜色与自动消失时间。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// 账户余额状态
#[derive(Debug, Clone, PartialEq)]
pub enum BalanceStatus {
    Loading,
    Ready(f64),
    Failed(String),
}

/// 工具调用状态，用于在 UI 中渲染卡片
#[derive(Debug, Clone)]
pub enum ToolCallStatus {
    Pending,
    Running,
    Success { result: String },
    Failed { error: String },
}

/// 待用户确认的工具调用对话框
#[derive(Debug, Clone)]
pub struct PendingConfirmation {
    pub tab_id: usize,
    pub call_id: String,
    pub name: String,
    pub args: String,
    pub description: String,
}

/// 补全弹窗的类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    File,
    Command,
}

/// 补全弹窗中的一个候选项
#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub label: String,
    pub detail: String,
}

/// 输入框上方的补全弹窗
#[derive(Debug, Clone)]
pub struct Completion {
    pub kind: CompletionKind,
    pub items: Vec<CompletionItem>,
    pub selected: usize,
}

/// 单个标签页的会话状态
#[derive(Debug, Clone)]
pub struct Tab {
    /// 稳定 ID（不会因关闭其他标签页而变化）
    pub id: usize,
    pub title: String,
    pub messages: Vec<Message>,
    pub model: String,
    pub agent_name: String,
    pub total_usage: Usage,
    /// 最近一次请求的 prompt tokens，近似等于当前上下文大小
    pub last_prompt_tokens: u64,
    pub input_buffer: String,
    /// 输入光标（字节索引）
    pub input_cursor: usize,
    /// 输入历史
    pub history: Vec<String>,
    pub history_index: usize,
    pub history_draft: String,
    /// 滚动偏移（从顶部算起的行数）
    pub scroll_offset: usize,
    /// 渲染时计算出的最大可滚动行数
    pub max_scroll: usize,
    pub auto_scroll: bool,
    pub thinking_ticks: u64,
    pub pending_tool_calls: HashMap<String, ToolCallStatus>,
    /// 当前标签页是否有正在进行的流式请求
    pub is_streaming: bool,
    /// 请求开始时间，用于显示耗时
    pub request_started: Option<Instant>,
    /// 取消标志
    pub stream_cancel: Option<Arc<AtomicBool>>,
    /// 本轮自动工具调用的轮次，防止无限循环
    pub tool_rounds: u8,
    /// 已保存对话的 ID（用于 /save /fav）
    pub conversation_id: Option<i64>,
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
            last_prompt_tokens: 0,
            input_buffer: String::new(),
            input_cursor: 0,
            history: Vec::new(),
            history_index: 0,
            history_draft: String::new(),
            scroll_offset: 0,
            max_scroll: 0,
            auto_scroll: true,
            thinking_ticks: 0,
            pending_tool_calls: HashMap::new(),
            is_streaming: false,
            request_started: None,
            stream_cancel: None,
            tool_rounds: 0,
            conversation_id: None,
        }
    }

    /// 记录一条输入历史。
    pub fn push_history(&mut self, entry: String) {
        let entry = entry.trim().to_string();
        if entry.is_empty() {
            return;
        }
        if self.history.last() != Some(&entry) {
            self.history.push(entry);
            if self.history.len() > 100 {
                self.history.remove(0);
            }
        }
        self.history_index = self.history.len();
        self.history_draft.clear();
    }

    /// 上一条历史输入。
    pub fn history_prev(&mut self) -> bool {
        if self.history.is_empty() {
            return false;
        }
        if self.history_index == self.history.len() {
            self.history_draft = self.input_buffer.clone();
        }
        if self.history_index > 0 {
            self.history_index -= 1;
        }
        self.input_buffer = self.history[self.history_index].clone();
        self.input_cursor = self.input_buffer.len();
        true
    }

    /// 下一条历史输入（末尾恢复草稿）。
    pub fn history_next(&mut self) -> bool {
        if self.history_index >= self.history.len() {
            return false;
        }
        self.history_index += 1;
        self.input_buffer = if self.history_index == self.history.len() {
            self.history_draft.clone()
        } else {
            self.history[self.history_index].clone()
        };
        self.input_cursor = self.input_buffer.len();
        true
    }

    /// 清空输入框。
    pub fn clear_input(&mut self) {
        self.input_buffer.clear();
        self.input_cursor = 0;
        self.history_index = self.history.len();
    }

    /// 清空当前对话内容（保留模型与 Agent）。
    pub fn reset_conversation(&mut self) {
        self.title = "新会话".into();
        self.messages.clear();
        self.total_usage = Usage::default();
        self.last_prompt_tokens = 0;
        self.pending_tool_calls.clear();
        self.scroll_offset = 0;
        self.auto_scroll = true;
        self.conversation_id = None;
        self.tool_rounds = 0;
        self.is_streaming = false;
        self.request_started = None;
        self.stream_cancel = None;
        self.thinking_ticks = 0;
    }
}

/// 应用全局状态
pub struct App {
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub next_tab_id: usize,
    pub sidebar_visible: bool,
    pub command_palette_open: bool,
    pub command_palette_input: String,
    pub command_palette_selected: usize,
    pub help_open: bool,
    pub completion: Option<Completion>,
    pub focus: Focus,
    pub agents: Vec<AgentConfig>,
    pub client: DeepSeekClient,
    pub memory: MemoryStore,
    pub pending_confirmation: Option<PendingConfirmation>,
    pub confirmation_queue: VecDeque<PendingConfirmation>,
    pub balance: BalanceStatus,
    pub pricing: Pricing,
    pub theme: String,
    pub should_quit: bool,
    pub mouse_capture: bool,
    pub status_message: String,
    pub status_level: StatusLevel,
    pub status_ttl: u8,
}

impl App {
    /// 从配置创建应用实例
    pub fn new(agents: Vec<AgentConfig>, app_config: AppConfig) -> Self {
        let default_pricing = app_config.default_pricing.unwrap_or(DefaultPricing {
            prompt_price_per_m: 2.0,
            completion_price_per_m: 8.0,
        });

        let api_key_missing = app_config.api_key.trim().is_empty();
        let client = DeepSeekClient::new(DeepSeekConfig {
            api_base: app_config.api_base,
            api_key: app_config.api_key,
            model: app_config.default_model.clone(),
        });

        let memory = MemoryStore::open().expect("无法初始化 SQLite 数据库");

        let default_model = app_config.default_model;
        let default_agent = agents.first().map(|a| a.name.clone()).unwrap_or_default();

        let first_tab = Tab::new(0, "新会话", &default_model, &default_agent);

        let mut app = Self {
            tabs: vec![first_tab],
            active_tab: 0,
            next_tab_id: 1,
            sidebar_visible: true,
            command_palette_open: false,
            command_palette_input: String::new(),
            command_palette_selected: 0,
            help_open: false,
            completion: None,
            focus: Focus::ChatInput,
            agents,
            client,
            memory,
            pending_confirmation: None,
            confirmation_queue: VecDeque::new(),
            balance: BalanceStatus::Loading,
            pricing: Pricing {
                prompt_price_per_m: default_pricing.prompt_price_per_m,
                completion_price_per_m: default_pricing.completion_price_per_m,
            },
            theme: "default".into(),
            should_quit: false,
            mouse_capture: true,
            status_message: String::new(),
            status_level: StatusLevel::Info,
            status_ttl: 0,
        };

        if api_key_missing {
            app.set_error("未配置 DEEPSEEK_TOKEN · 在 .env 中设置后重启");
        }

        app
    }

    /// 获取当前活跃的标签页引用
    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    /// 获取当前活跃的标签页可变引用
    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab]
    }

    /// 通过稳定 ID 查找标签页。
    pub fn tab_mut(&mut self, id: usize) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    /// 设置普通信息（约 4.5 秒后自动消失）。
    pub fn set_status(&mut self, message: impl Into<String>) {
        self.set_status_with(StatusLevel::Info, message);
    }

    /// 设置成功信息。
    pub fn set_success(&mut self, message: impl Into<String>) {
        self.set_status_with(StatusLevel::Success, message);
    }

    /// 设置警告信息（约 9 秒后自动消失）。
    pub fn set_warning(&mut self, message: impl Into<String>) {
        self.set_status_with(StatusLevel::Warning, message);
    }

    /// 设置错误信息（约 15 秒后自动消失）。
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.set_status_with(StatusLevel::Error, message);
    }

    fn set_status_with(&mut self, level: StatusLevel, message: impl Into<String>) {
        self.status_message = message.into();
        self.status_level = level;
        self.status_ttl = match level {
            StatusLevel::Info => 45,
            StatusLevel::Success => 55,
            StatusLevel::Warning => 90,
            StatusLevel::Error => 150,
        };
    }

    /// 是否有需要持续重绘的动画（流式输出或状态消息倒计时）。
    pub fn needs_animation(&self) -> bool {
        self.status_ttl > 0 || self.tabs.iter().any(|tab| tab.is_streaming)
    }

    /// 当前命令面板选中的命令。
    pub fn selected_command(&self) -> Option<&'static CommandSpec> {
        let list = command::filter(&self.command_palette_input);
        if list.is_empty() {
            None
        } else {
            Some(list[self.command_palette_selected.min(list.len() - 1)])
        }
    }

    /// 根据输入框内容刷新 @ 文件 / 斜杠命令补全弹窗。
    pub fn refresh_completion(&mut self) {
        let tab = self.active_tab();
        let content = tab.input_buffer.clone();
        let cursor = tab.input_cursor;

        let mut next = None;

        let before_cursor = &content[..input::clamp_cursor(&content, cursor)];
        let typing_command = content.starts_with('/')
            && !content.contains('\n')
            && !before_cursor.chars().any(|ch| ch.is_whitespace());

        if typing_command {
            let query = content.trim_start_matches('/');
            let exact_match = !query.is_empty()
                && command::COMMANDS
                    .iter()
                    .any(|spec| spec.name[1..].eq_ignore_ascii_case(query));
            let items: Vec<CompletionItem> = command::filter(query)
                .into_iter()
                .map(|spec| CompletionItem {
                    label: spec.name.to_string(),
                    detail: spec.desc.to_string(),
                })
                .collect();
            if !exact_match && !items.is_empty() {
                next = Some(Completion {
                    kind: CompletionKind::Command,
                    items,
                    selected: 0,
                });
            }
        } else if let Some(query) = input::active_at_query(&content, cursor) {
            let items: Vec<CompletionItem> = scan_files_for_autocomplete(&query.prefix)
                .into_iter()
                .map(|path| CompletionItem {
                    label: path,
                    detail: String::new(),
                })
                .collect();
            let already_complete =
                items.len() == 1 && items[0].label == query.prefix && !query.quoted;
            if !items.is_empty() && !already_complete {
                next = Some(Completion {
                    kind: CompletionKind::File,
                    items,
                    selected: 0,
                });
            }
        }

        self.completion = next;
    }

    /// 把当前选中的补全项写入输入框。
    pub fn apply_completion(&mut self) {
        let Some(completion) = self.completion.clone() else {
            return;
        };
        let Some(item) = completion.items.get(completion.selected).cloned() else {
            return;
        };

        match completion.kind {
            CompletionKind::Command => {
                let text = format!("{} ", item.label);
                let tab = self.active_tab_mut();
                tab.input_buffer = text.clone();
                tab.input_cursor = text.len();
                self.completion = None;
            }
            CompletionKind::File => {
                {
                    let tab = self.active_tab_mut();
                    let text = &mut tab.input_buffer;
                    let cursor = &mut tab.input_cursor;
                    if let Some(query) = input::active_at_query(text, *cursor) {
                        input::apply_at_completion(text, cursor, &query, &item.label);
                    }
                }
                self.refresh_completion();
            }
        }
    }

    /// 发起流式对话请求 —— 构建消息列表并调用 DeepSeek API
    ///
    /// 将当前活跃标签页的对话历史（含系统提示）打包为 API 请求，
    /// 通过 `stream_chat` 后台任务发送，结果通过 `tx` 通道异步回传。
    pub fn send_stream_chat_request(&mut self, tx: UnboundedSender<Action>) {
        let tab_id = self.active_tab().id;
        self.send_stream_chat_request_for(tab_id, tx);
    }

    /// 为指定标签页发起流式请求（后台标签页的工具调用也走这里）。
    pub fn send_stream_chat_request_for(&mut self, tab_id: usize, tx: UnboundedSender<Action>) {
        if self.client.api_key().trim().is_empty() {
            self.set_error("未配置 API Key · 在 .env 中设置 DEEPSEEK_TOKEN 后重启");
            return;
        }

        let Some(tab_index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        if self.tabs[tab_index].is_streaming {
            self.set_warning("正在生成回复 · 按 Esc 取消后再发送");
            return;
        }

        let tab = &self.tabs[tab_index];
        let model = tab.model.clone();

        let agent = self.agents.iter().find(|a| a.name == tab.agent_name);
        let system_content = agent.map(|a| a.system_prompt.clone()).unwrap_or_default();
        let tools = agent.map(|a| a.to_api_tools());

        let mut api_messages = vec![ApiMessage {
            role: "system".to_string(),
            content: system_content,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }];
        let mut unresolved_tool_calls: Vec<String> = Vec::new();
        for msg in &tab.messages {
            // /history 等本地提示以 System 消息展示，不应进入模型上下文。
            if msg.role == Role::System {
                continue;
            }
            match msg.role {
                Role::Assistant => {
                    unresolved_tool_calls = msg
                        .tool_calls
                        .as_ref()
                        .map(|calls| calls.iter().map(|call| call.id.clone()).collect())
                        .unwrap_or_default();
                }
                Role::Tool => {
                    if let Some(id) = &msg.tool_call_id {
                        unresolved_tool_calls.retain(|pending| pending != id);
                    }
                }
                _ => {}
            }
            api_messages.push(ApiMessage::from(msg));
        }
        // 会话可能在等待工具确认时被关闭/恢复；补一条占位结果，
        // 避免 API 因「tool_calls 后缺少 tool 消息」而拒绝请求。
        for call_id in unresolved_tool_calls {
            api_messages.push(ApiMessage {
                role: "tool".to_string(),
                content: "(工具未执行：会话已中断)".to_string(),
                tool_calls: None,
                tool_call_id: Some(call_id),
                name: None,
            });
        }

        let cancel = Arc::new(AtomicBool::new(false));
        {
            let tab = &mut self.tabs[tab_index];
            if let Some(previous) = tab.stream_cancel.replace(cancel.clone()) {
                previous.store(true, Ordering::Relaxed);
            }
            tab.is_streaming = true;
            tab.thinking_ticks = 0;
            tab.request_started = Some(Instant::now());
            tab.auto_scroll = true;
            tab.scroll_offset = 0;
        }

        self.completion = None;
        self.client
            .clone()
            .stream_chat(tx, tab_id, model, api_messages, tools, Some(0.7), cancel);
    }
}

/// 核心状态转换函数 —— 接收 Action 并修改 App 状态
///
/// 返回 `true` 表示需要重新绘制界面。
pub fn update(app: &mut App, action: Action) -> bool {
    match action {
        Action::Key(_event) => true,

        Action::Tick => {
            let streaming = app.active_tab().is_streaming;
            if streaming {
                let tab = app.active_tab_mut();
                tab.thinking_ticks = tab.thinking_ticks.wrapping_add(1);
            }
            if app.status_ttl > 0 {
                app.status_ttl -= 1;
                if app.status_ttl == 0 {
                    app.status_message.clear();
                }
            }
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

            if app.active_tab().is_streaming {
                app.set_warning("正在生成回复 · 按 Esc 取消后再发送");
                return true;
            }

            let visible = history_entry(&content);
            {
                let tab = app.active_tab_mut();
                let default_title = tab.title == "新会话" || tab.title.starts_with("会话 ");
                if tab.messages.is_empty() || default_title {
                    tab.title = derive_title(&visible);
                }
                tab.messages.push(Message::user(&content));
                tab.push_history(visible.clone());
                tab.clear_input();
                tab.auto_scroll = true;
                tab.scroll_offset = 0;
                tab.tool_rounds = 0;
                tab.thinking_ticks = 0;
            }
            app.completion = None;
            true
        }

        Action::SwitchTab(index) => {
            if index < app.tabs.len() {
                app.active_tab = index;
                app.focus = Focus::ChatInput;
                app.refresh_completion();
            }
            true
        }

        Action::NewTab => {
            create_tab(app, None, Vec::new(), None);
            app.set_success("已新建会话");
            true
        }

        Action::CloseTab(index) => {
            close_tab(app, index);
            true
        }

        Action::ToggleSidebar => {
            app.sidebar_visible = !app.sidebar_visible;
            let state = if app.sidebar_visible {
                "显示"
            } else {
                "隐藏"
            };
            app.set_status(format!("侧边栏已{}", state));
            true
        }

        Action::OpenCommandPalette => {
            app.command_palette_open = !app.command_palette_open;
            app.command_palette_input.clear();
            app.command_palette_selected = 0;
            app.completion = None;
            app.focus = if app.command_palette_open {
                Focus::CommandPalette
            } else {
                Focus::ChatInput
            };
            true
        }

        Action::ToggleHelp => {
            app.help_open = !app.help_open;
            if app.help_open {
                app.completion = None;
            }
            true
        }

        Action::CancelStream => {
            let was_streaming = {
                let tab = app.active_tab_mut();
                let was_streaming = tab.is_streaming;
                if let Some(flag) = tab.stream_cancel.take() {
                    flag.store(true, Ordering::Relaxed);
                }
                tab.is_streaming = false;
                tab.request_started = None;
                tab.thinking_ticks = 0;

                // 尚未输出任何内容时，直接移除空的助手消息，保持对话整洁。
                let remove_empty = tab
                    .messages
                    .last()
                    .map(|message| {
                        message.role == Role::Assistant
                            && message.content.trim().is_empty()
                            && message
                                .tool_calls
                                .as_ref()
                                .map(|calls| calls.is_empty())
                                .unwrap_or(true)
                    })
                    .unwrap_or(false);
                if remove_empty {
                    tab.messages.pop();
                }
                was_streaming
            };

            if was_streaming {
                app.set_warning("已取消生成 · Esc 或 Ctrl+C 可随时停止");
            }
            true
        }

        Action::StreamStart { tab_id, message_id } => {
            if let Some(tab) = app.tab_mut(tab_id) {
                tab.is_streaming = true;
                if tab.request_started.is_none() {
                    tab.request_started = Some(Instant::now());
                }
                if !tab.messages.iter().any(|m| m.id == message_id) {
                    tab.messages.push(Message {
                        id: message_id,
                        role: Role::Assistant,
                        content: String::new(),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                }
            }
            true
        }

        Action::StreamChunk {
            tab_id,
            message_id,
            chunk,
        } => {
            if let Some(tab) = app.tab_mut(tab_id) {
                if let Some(msg) = tab.messages.iter_mut().find(|m| m.id == message_id) {
                    msg.content.push_str(&chunk);
                }
            }
            true
        }

        Action::StreamDone {
            tab_id,
            message_id,
            content,
            usage,
            has_tool_calls,
        } => {
            if let Some(tab) = app.tab_mut(tab_id) {
                tab.total_usage.prompt_tokens += usage.prompt_tokens;
                tab.total_usage.completion_tokens += usage.completion_tokens;
                tab.total_usage.total_tokens += usage.total_tokens;
                tab.last_prompt_tokens = usage.prompt_tokens;
                tab.is_streaming = false;
                tab.request_started = None;
                tab.stream_cancel = None;
                tab.thinking_ticks = 0;

                if let Some(msg) = tab.messages.iter_mut().find(|m| m.id == message_id) {
                    let message_has_tools = msg
                        .tool_calls
                        .as_ref()
                        .map(|calls| !calls.is_empty())
                        .unwrap_or(false);
                    if msg.content.is_empty() {
                        msg.content = if !content.is_empty() {
                            content
                        } else if has_tool_calls || message_has_tools {
                            String::new()
                        } else {
                            "(空响应)".into()
                        };
                    }
                }
            }
            app.set_success(format!(
                "完成 · {} tokens · ¥{:.4}",
                usage.total_tokens,
                usage.cost(&app.pricing)
            ));
            true
        }

        Action::StreamError {
            tab_id,
            message_id,
            error,
        } => {
            if let Some(tab) = app.tab_mut(tab_id) {
                tab.is_streaming = false;
                tab.request_started = None;
                tab.stream_cancel = None;
                tab.thinking_ticks = 0;

                let mut has_placeholder = false;
                let mut has_partial_content = false;
                if let Some(msg) = tab.messages.iter_mut().find(|m| m.id == message_id) {
                    has_placeholder = true;
                    if msg.content.trim().is_empty() {
                        // 没有生成任何内容：把占位消息改成系统提示，避免进入模型上下文。
                        msg.role = Role::System;
                        msg.content = format!("请求失败：{}", error);
                    } else {
                        has_partial_content = true;
                    }
                }
                if has_partial_content || !has_placeholder {
                    tab.messages
                        .push(Message::system(format!("请求失败：{}", error)));
                }
            }
            app.set_error(format!("请求失败 · {}", text::truncate_width(&error, 90)));
            true
        }

        Action::ToolCallRequest {
            tab_id,
            call_id,
            name,
            args,
        } => {
            let is_new = app
                .tab_mut(tab_id)
                .map(|tab| !tab.pending_tool_calls.contains_key(&call_id))
                .unwrap_or(false);
            if !is_new {
                return true;
            }

            if let Some(tab) = app.tab_mut(tab_id) {
                tab.pending_tool_calls
                    .insert(call_id.clone(), ToolCallStatus::Pending);

                let already_attached = tab.messages.iter().any(|m| {
                    m.tool_calls
                        .as_ref()
                        .map(|calls| calls.iter().any(|c| c.id == call_id))
                        .unwrap_or(false)
                });
                if !already_attached {
                    if let Some(last_msg) = tab
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|m| m.role == Role::Assistant)
                    {
                        let calls = last_msg.tool_calls.get_or_insert_with(Vec::new);
                        calls.push(ToolCall {
                            id: call_id.clone(),
                            call_type: "function".into(),
                            function: crate::api::types::ToolFunction {
                                name: name.clone(),
                                arguments: args.clone(),
                            },
                        });
                    }
                }
            }

            let summary = describe_tool(&name, &args);
            let tool_description = app
                .agents
                .iter()
                .flat_map(|agent| agent.tools.iter())
                .find(|tool| tool.name == name)
                .map(|tool| tool.description.clone())
                .unwrap_or_default();
            let description = if tool_description.is_empty() {
                summary
            } else {
                format!("{} · {}", summary, tool_description)
            };

            let confirmation = PendingConfirmation {
                tab_id,
                call_id: call_id.clone(),
                name: name.clone(),
                args: args.clone(),
                description,
            };

            if app.pending_confirmation.is_none() {
                app.pending_confirmation = Some(confirmation);
            } else {
                app.confirmation_queue.push_back(confirmation);
            }
            let remaining = app.confirmation_queue.len();
            let message = if remaining > 0 {
                format!("待确认：{}（另有 {} 个）· Y 执行 / N 跳过", name, remaining)
            } else {
                format!("待确认：{} · Y 执行 / N 跳过", name)
            };
            app.set_warning(message);
            true
        }

        Action::ToolCallResponse {
            tab_id,
            call_id,
            result,
        } => {
            if let Some(tab) = app.tab_mut(tab_id) {
                tab.pending_tool_calls.insert(
                    call_id.clone(),
                    ToolCallStatus::Success {
                        result: result.clone(),
                    },
                );
                tab.messages.push(Message::tool(&call_id, &result));
            }
            true
        }

        Action::ConfirmToolCall(confirmation) => {
            if let Some(tab) = app.tab_mut(confirmation.tab_id) {
                tab.pending_tool_calls
                    .insert(confirmation.call_id.clone(), ToolCallStatus::Running);
            }

            let result = execute_tool(&confirmation.name, &confirmation.args);

            if let Some(tab) = app.tab_mut(confirmation.tab_id) {
                tab.pending_tool_calls.insert(
                    confirmation.call_id.clone(),
                    ToolCallStatus::Success {
                        result: result.clone(),
                    },
                );
                tab.messages
                    .push(Message::tool(&confirmation.call_id, &result));
            }

            advance_confirmation(app);
            if app.pending_confirmation.is_none() {
                app.set_success(format!("{} 执行完成", confirmation.name));
            }
            true
        }

        Action::RejectToolCall { tab_id, call_id } => {
            let result = "(用户拒绝)".to_string();
            if let Some(tab) = app.tab_mut(tab_id) {
                tab.pending_tool_calls.insert(
                    call_id.clone(),
                    ToolCallStatus::Failed {
                        error: result.clone(),
                    },
                );
                tab.messages.push(Message::tool(&call_id, &result));
            }
            advance_confirmation(app);
            if app.pending_confirmation.is_none() {
                app.set_warning("已跳过工具调用");
            }
            true
        }

        Action::CommandPaletteSubmit(input) => {
            let raw = input.trim().to_string();
            let selected = app.selected_command();

            app.command_palette_open = false;
            app.command_palette_input.clear();
            app.command_palette_selected = 0;
            app.completion = None;
            app.focus = Focus::ChatInput;

            if raw.starts_with('/') {
                handle_command(app, &raw);
            } else if !raw.is_empty() {
                if let Some(spec) = selected {
                    run_palette_command(app, spec);
                } else {
                    app.set_error(format!("未知命令：{} · 输入 /help 查看帮助", raw));
                }
            } else if let Some(spec) = selected {
                run_palette_command(app, spec);
            }
            true
        }

        Action::ScrollUp => {
            let tab = app.active_tab_mut();
            if tab.auto_scroll {
                tab.scroll_offset = tab.max_scroll.saturating_sub(1);
                tab.auto_scroll = false;
            } else if tab.scroll_offset > 0 {
                tab.scroll_offset -= 1;
            }
            true
        }

        Action::ScrollDown => {
            let tab = app.active_tab_mut();
            if !tab.auto_scroll {
                if tab.scroll_offset + 1 >= tab.max_scroll {
                    tab.scroll_offset = 0;
                    tab.auto_scroll = true;
                } else {
                    tab.scroll_offset += 1;
                }
            }
            true
        }

        Action::ScrollPageUp => {
            let tab = app.active_tab_mut();
            let step = 10;
            let was_following = tab.auto_scroll;
            tab.auto_scroll = false;
            tab.scroll_offset = if was_following {
                tab.max_scroll.saturating_sub(step)
            } else {
                tab.scroll_offset.saturating_sub(step)
            };
            true
        }

        Action::ScrollPageDown => {
            let tab = app.active_tab_mut();
            let step = 10;
            if tab.auto_scroll {
                return true;
            }
            if tab.scroll_offset + step >= tab.max_scroll {
                tab.scroll_offset = 0;
                tab.auto_scroll = true;
            } else {
                tab.scroll_offset += step;
            }
            true
        }

        Action::ScrollToTop => {
            let tab = app.active_tab_mut();
            tab.auto_scroll = false;
            tab.scroll_offset = 0;
            true
        }

        Action::AutoScroll => {
            let tab = app.active_tab_mut();
            tab.auto_scroll = true;
            tab.scroll_offset = 0;
            true
        }

        Action::BalanceUpdated(result) => {
            match result {
                Ok(balance) => {
                    app.balance = BalanceStatus::Ready(balance);
                    if balance <= 0.01 {
                        app.set_warning("余额不足 · 请前往 DeepSeek 平台充值");
                    }
                }
                Err(error) => {
                    app.balance = BalanceStatus::Failed(error.clone());
                    app.set_error(format!(
                        "余额查询失败 · {}",
                        text::truncate_width(&error, 80)
                    ));
                }
            }
            true
        }

        Action::ChangeTheme(theme) => {
            app.theme = theme.clone();
            app.set_success(format!("主题已切换为 {}", theme));
            true
        }

        Action::ChangeModel { tab_id, model } => {
            if let Some(tab) = app.tab_mut(tab_id) {
                tab.model = model.clone();
            }
            app.client.set_model(model.clone());
            app.set_success(format!("模型已切换为 {}", model));
            true
        }

        Action::SwitchAgent { tab_id, agent_name } => {
            let known = app.agents.iter().any(|agent| agent.name == agent_name);
            if known {
                if let Some(tab) = app.tab_mut(tab_id) {
                    tab.agent_name = agent_name.clone();
                }
                app.set_success(format!("已切换到 {}", agent_name));
            }
            true
        }

        Action::SwitchAgentByIndex(idx) => {
            let name = app.agents.get(idx).map(|agent| agent.name.clone());
            if let Some(name) = name {
                app.active_tab_mut().agent_name = name.clone();
                app.set_success(format!("已切换到 {}", name));
            }
            true
        }

        Action::SwitchAgentNext => {
            if !app.agents.is_empty() {
                let current = app
                    .agents
                    .iter()
                    .position(|a| a.name == app.active_tab().agent_name)
                    .unwrap_or(0);
                let next = (current + 1) % app.agents.len();
                let name = app.agents[next].name.clone();
                app.active_tab_mut().agent_name = name.clone();
                app.set_success(format!("已切换到 {}", name));
            }
            true
        }

        Action::SaveConversation => {
            match save_current_tab(app) {
                Ok(id) => app.set_success(format!("对话已保存 · ID {}", id)),
                Err(error) => app.set_error(format!("保存失败 · {}", error)),
            }
            true
        }

        Action::LoadConversation(id) => {
            match app.memory.load_conversation(id) {
                Ok(messages) => {
                    create_tab(app, Some(format!("对话 #{}", id)), messages, Some(id));
                    app.set_success(format!("已加载对话 #{}", id));
                }
                Err(error) => app.set_error(format!("加载失败 · {}", error)),
            }
            true
        }

        Action::ListConversations => {
            list_conversations(app);
            true
        }

        Action::DeleteConversation(id) => {
            match app.memory.delete_conversation(id) {
                Ok(()) => app.set_success(format!("已删除对话 #{}", id)),
                Err(error) => app.set_error(format!("删除失败 · {}", error)),
            }
            true
        }

        Action::ToggleFavorite(id) => {
            match app.memory.toggle_favorite(id) {
                Ok(favorited) => {
                    let label = if favorited {
                        "已收藏"
                    } else {
                        "已取消收藏"
                    };
                    app.set_success(format!("{}对话 #{}", label, id));
                }
                Err(error) => app.set_error(format!("操作失败 · {}", error)),
            }
            true
        }

        Action::LoadConversationToTab {
            tab_id,
            conversation_id,
        } => {
            match app.memory.load_conversation(conversation_id) {
                Ok(messages) => {
                    let mut loaded = false;
                    if let Some(tab) = app.tab_mut(tab_id) {
                        tab.messages = messages;
                        tab.auto_scroll = true;
                        tab.scroll_offset = 0;
                        tab.conversation_id = Some(conversation_id);
                        loaded = true;
                    }
                    if loaded {
                        app.set_success(format!("对话 #{} 已载入当前会话", conversation_id));
                    }
                }
                Err(error) => app.set_error(format!("加载失败 · {}", error)),
            }
            true
        }
    }
}

/// 新建标签页，返回其在 `tabs` 中的索引。
fn create_tab(
    app: &mut App,
    title: Option<String>,
    messages: Vec<Message>,
    conversation_id: Option<i64>,
) -> usize {
    let id = app.next_tab_id;
    app.next_tab_id += 1;
    let model = app.client.model().to_string();
    let agent = app
        .agents
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_default();
    let mut tab = Tab::new(
        id,
        title.unwrap_or_else(|| format!("会话 {}", id + 1)),
        &model,
        agent,
    );
    tab.messages = messages;
    tab.conversation_id = conversation_id;
    app.tabs.push(tab);
    app.active_tab = app.tabs.len() - 1;
    app.focus = Focus::ChatInput;
    app.active_tab
}

/// 关闭指定索引的标签页，并修正活动索引。
fn close_tab(app: &mut App, index: usize) {
    if app.tabs.len() <= 1 || index >= app.tabs.len() {
        app.set_warning("至少保留一个会话");
        return;
    }
    if let Some(flag) = app.tabs[index].stream_cancel.take() {
        flag.store(true, Ordering::Relaxed);
    }

    // 关闭前自动保存，避免误按 Ctrl+W 丢失对话。
    let auto_save = app
        .memory
        .get_preference("auto_save")
        .ok()
        .flatten()
        .unwrap_or_else(|| "on_exit".into());
    let mut saved = false;
    {
        let tab = &app.tabs[index];
        let messages: Vec<Message> = tab
            .messages
            .iter()
            .filter(|message| message.role != Role::System)
            .cloned()
            .collect();
        if auto_save != "off" && !messages.is_empty() && tab.conversation_id.is_none() {
            saved = app
                .memory
                .save_conversation(&tab.title, &tab.agent_name, &tab.model, &messages)
                .is_ok();
        }
    }

    let closed_id = app.tabs[index].id;
    let was_active = index == app.active_tab;
    app.tabs.remove(index);

    // 关闭标签页后，清掉属于它的待确认工具调用，避免弹框指向不存在的会话。
    while app
        .pending_confirmation
        .as_ref()
        .map(|confirmation| confirmation.tab_id == closed_id)
        .unwrap_or(false)
    {
        app.pending_confirmation = app.confirmation_queue.pop_front();
    }
    app.confirmation_queue
        .retain(|confirmation| confirmation.tab_id != closed_id);
    if app.active_tab > index {
        app.active_tab -= 1;
    }
    if app.active_tab >= app.tabs.len() {
        app.active_tab = app.tabs.len() - 1;
    }
    if was_active {
        app.focus = Focus::ChatInput;
    }
    if saved {
        app.set_success("已自动保存并关闭会话");
    } else {
        app.set_success("已关闭会话");
    }
}

/// 确认/拒绝工具调用后，把队列中的下一个对话框推到前台。
fn advance_confirmation(app: &mut App) {
    app.pending_confirmation = app.confirmation_queue.pop_front();
    if let Some(next) = app.pending_confirmation.clone() {
        let remaining = app.confirmation_queue.len();
        let message = if remaining > 0 {
            format!(
                "待确认：{}（另有 {} 个）· Y 执行 / N 跳过",
                next.name, remaining
            )
        } else {
            format!("待确认：{} · Y 执行 / N 跳过", next.name)
        };
        app.set_warning(message);
    }
}

/// 为工具调用生成一句话摘要。
fn describe_tool(name: &str, args: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
    match name {
        "write_file" => {
            let path = parsed["path"].as_str().unwrap_or("?");
            let lines = parsed["content"].as_str().unwrap_or("").lines().count();
            format!("写入文件 {} · {} 行", path, lines)
        }
        "read_file" => format!("读取文件 {}", parsed["path"].as_str().unwrap_or("?")),
        "create_directory" => format!("创建目录 {}", parsed["path"].as_str().unwrap_or("?")),
        "list_directory" => format!("列出目录 {}", parsed["path"].as_str().unwrap_or("当前目录")),
        _ => format!("执行工具 {}", name),
    }
}

/// 从带附件的内容中提取用户实际输入的文本（用于标题与历史）。
fn history_entry(content: &str) -> String {
    const MARKER: &str = "── 附件结束 ──";
    if let Some(index) = content.find(MARKER) {
        content[index + MARKER.len()..].trim().to_string()
    } else {
        content.trim().to_string()
    }
}

/// 由用户输入生成一个简洁的标签页标题。
fn derive_title(content: &str) -> String {
    let first = content
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("新会话")
        .trim();
    let cleaned = first.trim_start_matches(['/', '@', ' ']).trim();
    if cleaned.is_empty() {
        "新会话".into()
    } else {
        text::truncate_width(cleaned, 18)
    }
}

/// 处理斜杠命令。
fn handle_command(app: &mut App, raw: &str) {
    let raw = raw.trim();
    if raw.is_empty() {
        return;
    }

    app.active_tab_mut().push_history(raw.to_string());

    let mut parts = raw.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or("");
    let argument = parts.next().unwrap_or("").trim().to_string();

    match command {
        "/new" => {
            create_tab(app, None, Vec::new(), None);
            app.set_success("已新建会话");
        }
        "/close" => {
            let index = app.active_tab;
            close_tab(app, index);
        }
        "/clear" => {
            {
                let tab = app.active_tab_mut();
                if let Some(flag) = tab.stream_cancel.take() {
                    flag.store(true, Ordering::Relaxed);
                }
                tab.reset_conversation();
            }
            app.pending_confirmation = None;
            app.confirmation_queue.clear();
            app.set_success("已清空当前对话");
        }
        "/theme" => {
            if argument.is_empty() {
                let current = app.theme.clone();
                let index = crate::ui::theme::CYCLE_THEMES
                    .iter()
                    .position(|name| *name == current)
                    .unwrap_or(0);
                let next = crate::ui::theme::CYCLE_THEMES
                    [(index + 1) % crate::ui::theme::CYCLE_THEMES.len()];
                app.theme = next.to_string();
                app.set_success(format!("主题已切换为 {}", next));
            } else if crate::ui::theme::ALL_THEME_NAMES.contains(&argument.as_str()) {
                app.theme = argument.clone();
                app.set_success(format!("主题已切换为 {}", argument));
            } else {
                app.set_error(format!(
                    "未知主题 {} · 可用: {}",
                    argument,
                    crate::ui::theme::CYCLE_THEMES.join(", ")
                ));
            }
        }
        "/model" => {
            if argument.is_empty() {
                app.set_status(format!(
                    "当前模型 {} · 可用: deepseek-v4-pro, deepseek-v4-flash, deepseek-chat, deepseek-reasoner",
                    app.active_tab().model
                ));
            } else {
                if let Some(tab) = app.tabs.get_mut(app.active_tab) {
                    tab.model = argument.clone();
                }
                app.client.set_model(argument.clone());
                app.set_success(format!("模型已切换为 {}", argument));
            }
        }
        "/agent" => {
            if argument.is_empty() {
                let names: Vec<String> = app.agents.iter().map(|a| a.name.clone()).collect();
                app.set_status(format!(
                    "当前 Agent {} · 可用: {}",
                    app.active_tab().agent_name,
                    names.join(", ")
                ));
            } else if app.agents.iter().any(|a| a.name == argument) {
                app.active_tab_mut().agent_name = argument.clone();
                app.set_success(format!("已切换到 {}", argument));
            } else {
                app.set_error(format!("未知 Agent {}", argument));
            }
        }
        "/save" => match save_current_tab(app) {
            Ok(id) => app.set_success(format!("对话已保存 · ID {}", id)),
            Err(error) => app.set_error(format!("保存失败 · {}", error)),
        },
        "/history" => list_conversations(app),
        "/load" => match argument.parse::<i64>() {
            Ok(id) => match app.memory.load_conversation(id) {
                Ok(messages) => {
                    create_tab(app, Some(format!("对话 #{}", id)), messages, Some(id));
                    app.set_success(format!("已加载对话 #{}", id));
                }
                Err(error) => app.set_error(format!("加载失败 · {}", error)),
            },
            Err(_) => app.set_error("用法: /load <ID> · 用 /history 查看可用 ID"),
        },
        "/fav" => {
            let id = match app.active_tab().conversation_id {
                Some(id) => Some(id),
                None => match save_current_tab(app) {
                    Ok(id) => Some(id),
                    Err(error) => {
                        app.set_error(format!("无法收藏 · {}", error));
                        None
                    }
                },
            };
            if let Some(id) = id {
                match app.memory.toggle_favorite(id) {
                    Ok(favorited) => {
                        let label = if favorited {
                            "已收藏"
                        } else {
                            "已取消收藏"
                        };
                        app.set_success(format!("{}对话 #{}", label, id));
                    }
                    Err(error) => app.set_error(format!("操作失败 · {}", error)),
                }
            }
        }
        "/export" => export_current(app, &argument),
        "/help" => {
            app.help_open = true;
            app.set_status("F1 / Esc 关闭帮助");
        }
        "/quit" | "/exit" => {
            app.should_quit = true;
        }
        _ => {
            app.set_error(format!("未知命令：{} · 输入 /help 查看可用命令", command));
        }
    }
}

/// 从命令面板选中命令：带参数的命令先补全，其余直接执行。
fn run_palette_command(app: &mut App, spec: &'static CommandSpec) {
    if spec.fill_args {
        let text = format!("{} ", spec.name);
        let tab = app.active_tab_mut();
        tab.input_buffer = text.clone();
        tab.input_cursor = text.len();
        app.focus = Focus::ChatInput;
        app.set_status(format!("{} · {}", spec.usage(), spec.desc));
    } else {
        handle_command(app, spec.name);
    }
}

/// 保存当前对话，返回对话 ID。
fn save_current_tab(app: &mut App) -> Result<i64, String> {
    let tab = app.active_tab();
    if tab.messages.is_empty() {
        return Err("当前对话为空".into());
    }
    let title = tab.title.clone();
    let agent = tab.agent_name.clone();
    let model = tab.model.clone();
    let messages: Vec<Message> = tab
        .messages
        .iter()
        .filter(|message| message.role != Role::System)
        .cloned()
        .collect();
    if messages.is_empty() {
        return Err("当前对话为空".into());
    }

    let id = app
        .memory
        .save_conversation(&title, &agent, &model, &messages)
        .map_err(|e| e.to_string())?;

    app.active_tab_mut().conversation_id = Some(id);
    let _ = app.memory.trim_old_conversations();
    Ok(id)
}

/// 列出记忆库中的对话。
fn list_conversations(app: &mut App) {
    match app.memory.list_conversations() {
        Ok(list) if list.is_empty() => {
            app.set_status("记忆库为空 · 使用 /save 保存当前对话");
        }
        Ok(list) => {
            let mut content = format!("已保存的对话（{}）\n\n", list.len());
            for item in list.iter().take(20) {
                let star = if item.is_favorited { "★" } else { "☆" };
                content.push_str(&format!(
                    "{} #{:<3}  {}  ·  {} 条消息  ·  {}\n",
                    star,
                    item.id,
                    text::truncate_width(&item.title, 28),
                    item.message_count,
                    item.created_at.get(..10).unwrap_or(&item.created_at)
                ));
            }
            if list.len() > 20 {
                content.push_str(&format!(
                    "\n… 还有 {} 条，仅显示前 20 条\n",
                    list.len() - 20
                ));
            }
            content.push_str("\n使用 /load <ID> 恢复对话");
            {
                let tab = app.active_tab_mut();
                // 避免重复执行 /history 时堆叠多份列表
                tab.messages.retain(|message| {
                    !(message.role == Role::System && message.content.starts_with("已保存的对话（"))
                });
                tab.messages.push(Message::system(content));
                tab.auto_scroll = true;
            }
            app.set_success(format!("找到 {} 条对话", list.len()));
        }
        Err(error) => app.set_error(format!("查询失败 · {}", error)),
    }
}

/// 导出当前对话为 Markdown。
fn export_current(app: &mut App, argument: &str) {
    let tab = app.active_tab();
    if tab.messages.is_empty() {
        app.set_error("当前对话为空，无法导出");
        return;
    }

    let markdown = conversation_markdown(tab);
    let path = if argument.is_empty() {
        PathBuf::from("exports").join(format!(
            "{}-{}.md",
            sanitize_filename(&tab.title),
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        ))
    } else {
        PathBuf::from(argument)
    };

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                app.set_error(format!("创建目录失败 · {}", error));
                return;
            }
        }
    }

    match std::fs::write(&path, markdown) {
        Ok(()) => app.set_success(format!("已导出到 {}", path.display())),
        Err(error) => app.set_error(format!("导出失败 · {}", error)),
    }
}

fn conversation_markdown(tab: &Tab) -> String {
    let mut output = format!(
        "# {}\n\n- Agent: {}\n- 模型: {}\n- 导出时间: {}\n- 消息数: {}\n- Token 用量: {}\n\n---\n\n",
        tab.title,
        tab.agent_name,
        tab.model,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        tab.messages.len(),
        tab.total_usage.total_tokens
    );

    for message in &tab.messages {
        // 本地提示（/history 列表、错误提示）不写入导出文件
        if message.role == Role::System {
            continue;
        }
        match message.role {
            Role::User => output.push_str(&format!("## 用户\n\n{}\n\n", message.content)),
            Role::Assistant => {
                output.push_str(&format!("## 助手\n\n{}\n\n", message.content));
                if let Some(calls) = &message.tool_calls {
                    for call in calls {
                        output.push_str(&format!(
                            "- 工具调用 `{}`：`{}`\n",
                            call.function.name,
                            call.function.arguments.replace('\n', " ")
                        ));
                    }
                    output.push('\n');
                }
            }
            Role::Tool => {
                output.push_str(&format!(
                    "### 工具结果 `{}`\n\n```\n{}\n```\n\n",
                    message.tool_call_id.clone().unwrap_or_default(),
                    message.content
                ));
            }
            Role::System => {
                output.push_str(&format!("> {}\n\n", message.content.replace('\n', "\n> ")));
            }
        }
    }
    output
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | ' ' => '-',
            _ => ch,
        })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "conversation".into()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::config::{AppConfig, DefaultPricing};

    fn test_app_config() -> AppConfig {
        AppConfig {
            api_base: "https://api.deepseek.com".into(),
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
        assert_eq!(app.active_tab().input_cursor, 0);
    }

    #[test]
    fn update_submit_message_trims_whitespace() {
        let mut app = make_app();
        update(&mut app, Action::SubmitMessage("  你好  ".into()));
        let last_msg = app.active_tab().messages.last().unwrap();
        assert_eq!(last_msg.content, "你好");
    }

    #[test]
    fn update_submit_message_auto_titles_tab() {
        let mut app = make_app();
        update(
            &mut app,
            Action::SubmitMessage("帮我审计这个仓库的 SQL 注入风险".into()),
        );
        assert!(app.active_tab().title.contains("审计"));
    }

    #[test]
    fn update_submit_message_records_history() {
        let mut app = make_app();
        update(&mut app, Action::SubmitMessage("第一条".into()));
        assert_eq!(app.active_tab().history.len(), 1);
        assert_eq!(app.active_tab().history[0], "第一条");
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
    fn update_submit_is_blocked_while_streaming() {
        let mut app = make_app();
        app.active_tab_mut().is_streaming = true;
        update(&mut app, Action::SubmitMessage("并发消息".into()));
        assert!(app.active_tab().messages.is_empty());
        assert!(app.status_message.contains("正在生成"));
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
    fn update_new_tab_uses_stable_unique_ids() {
        let mut app = make_app();
        update(&mut app, Action::NewTab);
        assert_eq!(app.tabs[0].id, 0);
        assert_eq!(app.tabs[1].id, 1);
        assert_eq!(app.next_tab_id, 2);
    }

    #[test]
    fn closing_first_tab_keeps_remaining_ids_stable() {
        let mut app = make_app();
        update(&mut app, Action::NewTab);
        update(&mut app, Action::CloseTab(0));
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.tabs[0].id, 1);

        let message_id = uuid::Uuid::new_v4();
        update(
            &mut app,
            Action::StreamStart {
                tab_id: 1,
                message_id,
            },
        );
        assert_eq!(app.tabs[0].messages.len(), 1);
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
    fn update_close_tab_before_active_shifts_index() {
        let mut app = make_app();
        update(&mut app, Action::NewTab);
        update(&mut app, Action::NewTab);
        app.active_tab = 2;
        update(&mut app, Action::CloseTab(0));
        assert_eq!(app.active_tab, 1);
        assert_eq!(app.tabs[1].id, 2);
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
    fn update_toggle_help_flips_state() {
        let mut app = make_app();
        update(&mut app, Action::ToggleHelp);
        assert!(app.help_open);
        update(&mut app, Action::ToggleHelp);
        assert!(!app.help_open);
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
        assert!(app.active_tab().is_streaming);
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
                has_tool_calls: false,
            },
        );
        assert_eq!(app.tabs[0].total_usage.prompt_tokens, 100);
        assert_eq!(app.tabs[0].total_usage.completion_tokens, 50);
        assert_eq!(app.tabs[0].last_prompt_tokens, 100);
        assert!(!app.tabs[0].is_streaming);
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
                has_tool_calls: false,
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
    fn update_stream_done_with_tool_calls_keeps_content_empty() {
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
                call_id: "call_1".into(),
                name: "read_file".into(),
                args: r#"{"path":"README.md"}"#.into(),
            },
        );
        update(
            &mut app,
            Action::StreamDone {
                tab_id: 0,
                message_id: msg_id,
                content: "".into(),
                usage: Usage::default(),
                has_tool_calls: true,
            },
        );
        let msg = app.tabs[0]
            .messages
            .iter()
            .find(|m| m.id == msg_id)
            .unwrap();
        assert!(msg.content.is_empty());
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
        assert!(!app.tabs[0].is_streaming);
    }

    #[test]
    fn stream_error_without_placeholder_adds_system_message() {
        let mut app = make_app();
        update(
            &mut app,
            Action::StreamError {
                tab_id: 0,
                message_id: uuid::Uuid::new_v4(),
                error: "无法连接 API".into(),
            },
        );
        let last = app.active_tab().messages.last().unwrap();
        assert_eq!(last.role, Role::System);
        assert!(last.content.contains("无法连接 API"));
        assert!(!app.active_tab().is_streaming);
    }

    #[test]
    fn closing_tab_clears_its_pending_confirmation() {
        let mut app = make_app();
        update(&mut app, Action::NewTab);
        let tab_id = app.active_tab().id;
        update(
            &mut app,
            Action::StreamStart {
                tab_id,
                message_id: uuid::Uuid::new_v4(),
            },
        );
        update(
            &mut app,
            Action::ToolCallRequest {
                tab_id,
                call_id: "call_close".into(),
                name: "read_file".into(),
                args: r#"{"path":"README.md"}"#.into(),
            },
        );
        assert!(app.pending_confirmation.is_some());

        update(&mut app, Action::CloseTab(1));
        assert!(app.pending_confirmation.is_none());
        assert!(app.confirmation_queue.is_empty());
    }

    #[test]
    fn clear_resets_tab_title() {
        let mut app = make_app();
        update(&mut app, Action::SubmitMessage("临时标题".into()));
        assert_ne!(app.active_tab().title, "新会话");
        update(&mut app, Action::CommandPaletteSubmit("/clear".into()));
        assert_eq!(app.active_tab().title, "新会话");
    }

    #[test]
    fn update_cancel_stream_removes_empty_reply() {
        let mut app = make_app();
        let msg_id = uuid::Uuid::new_v4();
        update(
            &mut app,
            Action::StreamStart {
                tab_id: 0,
                message_id: msg_id,
            },
        );
        update(&mut app, Action::CancelStream);
        assert!(app.tabs[0].messages.is_empty());
        assert!(!app.tabs[0].is_streaming);
        assert!(app.status_message.contains("已取消"));
    }

    #[test]
    fn update_cancel_stream_keeps_partial_reply() {
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
                chunk: "部分输出".into(),
            },
        );
        update(&mut app, Action::CancelStream);
        let msg = app.tabs[0]
            .messages
            .iter()
            .find(|m| m.id == msg_id)
            .unwrap();
        assert_eq!(msg.content, "部分输出");
        assert!(!app.tabs[0].is_streaming);
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
                args: r#"{"target": "192.168.1.1"}"#.into(),
            },
        );
        assert!(app.tabs[0].pending_tool_calls.contains_key("call_001"));
        assert!(app.pending_confirmation.is_some());
    }

    #[test]
    fn duplicate_tool_call_request_does_not_enqueue_twice() {
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
                name: "read_file".into(),
                args: r#"{"path":"a"}"#.into(),
            },
        );
        update(
            &mut app,
            Action::ToolCallRequest {
                tab_id: 0,
                call_id: "call_001".into(),
                name: "read_file".into(),
                args: r#"{"path":"a"}"#.into(),
            },
        );
        assert_eq!(app.confirmation_queue.len(), 0);
    }

    #[test]
    fn multiple_tool_calls_are_queued() {
        let mut app = make_app();
        let msg_id = uuid::Uuid::new_v4();
        update(
            &mut app,
            Action::StreamStart {
                tab_id: 0,
                message_id: msg_id,
            },
        );
        for id in ["call_a", "call_b"] {
            update(
                &mut app,
                Action::ToolCallRequest {
                    tab_id: 0,
                    call_id: id.into(),
                    name: "read_file".into(),
                    args: r#"{"path":"README.md"}"#.into(),
                },
            );
        }
        assert!(app.pending_confirmation.is_some());
        assert_eq!(app.confirmation_queue.len(), 1);
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
    fn update_confirm_tool_call_executes_and_sets_success() {
        let mut app = make_app();
        let confirmation = crate::action::ToolCallConfirmation {
            tab_id: 0,
            call_id: "call_003".into(),
            name: "test_tool".into(),
            args: r#"{}"#.into(),
        };
        app.tabs[0]
            .pending_tool_calls
            .insert("call_003".into(), ToolCallStatus::Pending);
        update(&mut app, Action::ConfirmToolCall(confirmation));
        let status = app.tabs[0].pending_tool_calls.get("call_003").unwrap();
        assert!(matches!(
            status,
            ToolCallStatus::Success { .. } | ToolCallStatus::Running
        ));
    }

    #[test]
    fn update_tick_returns_false() {
        let mut app = make_app();
        assert!(!update(&mut app, Action::Tick));
    }

    #[test]
    fn update_tick_expires_status_message() {
        let mut app = make_app();
        update(&mut app, Action::ToggleSidebar);
        assert!(!app.status_message.is_empty());
        for _ in 0..200 {
            update(&mut app, Action::Tick);
        }
        assert!(app.status_message.is_empty());
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
    fn scroll_up_pauses_follow_and_steps_from_bottom() {
        let mut app = make_app();
        app.active_tab_mut().max_scroll = 10;
        app.active_tab_mut().auto_scroll = true;

        update(&mut app, Action::ScrollUp);
        assert_eq!(app.active_tab().scroll_offset, 9);
        assert!(!app.active_tab().auto_scroll);

        update(&mut app, Action::ScrollUp);
        assert_eq!(app.active_tab().scroll_offset, 8);
    }

    #[test]
    fn scroll_up_at_top_stays_at_top() {
        let mut app = make_app();
        app.active_tab_mut().max_scroll = 10;
        app.active_tab_mut().auto_scroll = false;
        app.active_tab_mut().scroll_offset = 0;
        update(&mut app, Action::ScrollUp);
        assert_eq!(app.active_tab().scroll_offset, 0);
    }

    #[test]
    fn scroll_down_increments_when_not_following() {
        let mut app = make_app();
        app.active_tab_mut().max_scroll = 10;
        app.active_tab_mut().auto_scroll = false;
        app.active_tab_mut().scroll_offset = 3;
        update(&mut app, Action::ScrollDown);
        assert_eq!(app.active_tab().scroll_offset, 4);
    }

    #[test]
    fn scroll_down_at_bottom_resumes_follow() {
        let mut app = make_app();
        app.active_tab_mut().max_scroll = 5;
        app.active_tab_mut().auto_scroll = false;
        app.active_tab_mut().scroll_offset = 4;
        update(&mut app, Action::ScrollDown);
        assert!(app.active_tab().auto_scroll);
        assert_eq!(app.active_tab().scroll_offset, 0);
    }

    #[test]
    fn scroll_page_up_clamps_to_top() {
        let mut app = make_app();
        app.active_tab_mut().max_scroll = 30;
        app.active_tab_mut().auto_scroll = false;
        app.active_tab_mut().scroll_offset = 4;
        update(&mut app, Action::ScrollPageUp);
        assert_eq!(app.active_tab().scroll_offset, 0);
    }

    #[test]
    fn scroll_to_top_and_auto_scroll() {
        let mut app = make_app();
        app.active_tab_mut().max_scroll = 20;
        app.active_tab_mut().auto_scroll = false;
        app.active_tab_mut().scroll_offset = 8;
        update(&mut app, Action::ScrollToTop);
        assert_eq!(app.active_tab().scroll_offset, 0);
        assert!(!app.active_tab().auto_scroll);
        update(&mut app, Action::AutoScroll);
        assert!(app.active_tab().auto_scroll);
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
    fn input_history_navigation_restores_draft() {
        let mut app = make_app();
        update(&mut app, Action::SubmitMessage("第一条".into()));
        update(&mut app, Action::SubmitMessage("第二条".into()));
        let tab = app.active_tab_mut();
        tab.input_buffer = "草稿".into();
        tab.input_cursor = tab.input_buffer.len();

        assert!(tab.history_prev());
        assert_eq!(tab.input_buffer, "第二条");
        assert!(tab.history_prev());
        assert_eq!(tab.input_buffer, "第一条");
        assert!(tab.history_next());
        assert_eq!(tab.input_buffer, "第二条");
        assert!(tab.history_next());
        assert_eq!(tab.input_buffer, "草稿");
        assert!(!tab.history_next());
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
    fn theme_command_without_argument_cycles() {
        let mut app = make_app();
        app.theme = "cyber-dark".into();
        update(&mut app, Action::CommandPaletteSubmit("/theme".into()));
        assert_eq!(app.theme, "tokyo-night");
    }

    #[test]
    fn clear_command_empties_messages() {
        let mut app = make_app();
        update(&mut app, Action::SubmitMessage("稍后清空".into()));
        assert!(!app.active_tab().messages.is_empty());
        update(&mut app, Action::CommandPaletteSubmit("/clear".into()));
        assert!(app.active_tab().messages.is_empty());
        assert_eq!(app.active_tab().total_usage.total_tokens, 0);
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

    #[test]
    fn palette_selection_executes_filtered_command() {
        let mut app = make_app();
        update(&mut app, Action::SubmitMessage("待清空".into()));
        update(&mut app, Action::OpenCommandPalette);
        app.command_palette_input = "cle".into();
        app.command_palette_selected = 0;
        update(&mut app, Action::CommandPaletteSubmit(String::new()));
        assert!(app.active_tab().messages.is_empty());
    }

    #[test]
    fn help_command_opens_help() {
        let mut app = make_app();
        update(&mut app, Action::CommandPaletteSubmit("/help".into()));
        assert!(app.help_open);
    }
}
