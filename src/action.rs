//! 全局事件定义 —— 所有用户交互、API 响应、Tick 事件均通过 Action 枚举传递。
//!
//! 本模块是 Elm Architecture 中消息层的核心，所有状态变更都由 Action 触发。

use crossterm::event::KeyEvent;
use uuid::Uuid;

use crate::api::types::Usage;

/// 工具调用确认信息，在用户确认/拒绝工具调用时传递
#[derive(Debug, Clone)]
pub struct ToolCallConfirmation {
    pub tab_id: usize,
    pub call_id: String,
    pub name: String,
    pub args: serde_json::Value,
}

/// 全局事件枚举 —— 所有状态变更的源头
///
/// 通过 `tokio::sync::mpsc::unbounded_channel<Action>` 在主循环与异步任务间传递。
/// 包含三类事件：用户输入（按键/命令）、流式 API 响应、系统事件（Tick/Resize）。
#[derive(Debug, Clone)]
pub enum Action {
    // ---- 用户输入 ----
    Key(KeyEvent),
    SubmitMessage(String),
    SwitchTab(usize),
    NewTab,
    CloseTab(usize),
    ToggleSidebar,
    OpenCommandPalette,

    // ---- 系统事件 ----
    Tick,
    Resize(u16, u16),
    Shutdown,

    // ---- 流式 API 响应 ----
    StreamStart {
        tab_id: usize,
        message_id: Uuid,
    },
    StreamChunk {
        tab_id: usize,
        message_id: Uuid,
        chunk: String,
    },
    StreamDone {
        tab_id: usize,
        message_id: Uuid,
        content: String,
        usage: Usage,
    },
    StreamError {
        tab_id: usize,
        message_id: Uuid,
        error: String,
    },

    // ---- 工具调用 ----
    ToolCallRequest {
        tab_id: usize,
        call_id: String,
        name: String,
        args: serde_json::Value,
    },
    ToolCallResponse {
        tab_id: usize,
        call_id: String,
        result: String,
    },
    ConfirmToolCall(ToolCallConfirmation),

    // ---- 命令面板交互 ----
    CommandPaletteSubmit(String),

    // ---- 消息列表滚动 ----
    ScrollUp,
    ScrollDown,
    AutoScroll,

    // ---- 模型与主题切换 ----
    ChangeTheme(String),
    ChangeModel {
        tab_id: usize,
        model: String,
    },
    SwitchAgent {
        tab_id: usize,
        agent_name: String,
    },
    SwitchAgentByIndex(usize),
    SwitchAgentNext,
}
