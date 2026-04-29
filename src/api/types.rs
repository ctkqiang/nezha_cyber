//! DeepSeek API 数据类型 —— 与 OpenAI 兼容的请求/响应结构体。
//!
//! 本模块定义了与 DeepSeek Chat Completions API 交互所需的全部数据模型，
//! 包括消息、工具调用、流式 Delta、Usage 统计等。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 模型角色：system / user / assistant / tool
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 工具调用中的 Function 定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub arguments: String,
}

/// 消息中内嵌的工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolFunction,
}

/// 对话消息 —— API 请求/响应中的核心单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    /// 构造一条 system 角色消息
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: Role::System,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    /// 构造一条 user 角色消息
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: Role::User,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    /// 构造一条 assistant 角色消息（含可选 tool_calls）
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: Role::Assistant,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    /// 构造一条 tool 角色消息（工具返回结果）
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: Role::Tool,
            content: content.into(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            name: None,
        }
    }
}

/// OpenAI 兼容的 Function 定义（用于 tools 数组）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// OpenAI 兼容的 Tool 定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDefinition,
}

/// Chat Completions 请求体
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    pub stream: bool,
}

/// API 请求用的精简消息（不含 id 等本地字段）
#[derive(Debug, Clone, Serialize)]
pub struct ApiMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl From<&Message> for ApiMessage {
    fn from(msg: &Message) -> Self {
        Self {
            role: msg.role.as_str().to_string(),
            content: msg.content.clone(),
            tool_calls: msg.tool_calls.clone(),
            tool_call_id: msg.tool_call_id.clone(),
            name: msg.name.clone(),
        }
    }
}

/// Token 用量统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// DeepSeek API 定价（CNY per 1M tokens）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pricing {
    pub prompt_price_per_m: f64,
    pub completion_price_per_m: f64,
}

impl Usage {
    /// 按 DeepSeek 定价计算本次调用费用（人民币元）
    pub fn cost(&self, pricing: &Pricing) -> f64 {
        let prompt_cost = (self.prompt_tokens as f64 / 1_000_000.0) * pricing.prompt_price_per_m;
        let completion_cost =
            (self.completion_tokens as f64 / 1_000_000.0) * pricing.completion_price_per_m;
        prompt_cost + completion_cost
    }
}

/// SSE 流式响应中的 delta 块
#[derive(Debug, Clone, Deserialize)]
pub struct StreamDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<StreamToolCallDelta>>,
}

/// SSE 流中 tool_calls 数组的 delta 元素
#[derive(Debug, Clone, Deserialize)]
pub struct StreamToolCallDelta {
    pub index: usize,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub function: Option<StreamFunctionDelta>,
}

/// SSE 流中 function 字段的 delta
#[derive(Debug, Clone, Deserialize)]
pub struct StreamFunctionDelta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}

/// SSE 流式响应中的 choice
#[derive(Debug, Clone, Deserialize)]
pub struct StreamChoice {
    pub index: usize,
    pub delta: StreamDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// SSE 流式响应的顶层结构
#[derive(Debug, Clone, Deserialize)]
pub struct StreamResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<StreamChoice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// 非流式 Chat Completions 响应
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

/// 非流式响应中的 choice
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub index: usize,
    pub message: ChoiceMessage,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// choice 中的 message
#[derive(Debug, Clone, Deserialize)]
pub struct ChoiceMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// 账户余额信息
#[derive(Debug, Clone, Deserialize)]
pub struct BalanceInfo {
    pub is_available: bool,
    #[serde(default)]
    pub balance_infos: Vec<BalanceDetail>,
}

/// 余额明细
#[derive(Debug, Clone, Deserialize)]
pub struct BalanceDetail {
    pub currency: String,
    pub total_balance: String,
    pub granted_balance: String,
    pub topped_up_balance: String,
}

/// 累积的工具调用状态，用于在流式接收过程中拼接 tool_calls
#[derive(Debug, Clone)]
pub struct PendingToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl PendingToolCall {
    pub fn new(id: String) -> Self {
        Self {
            id,
            name: String::new(),
            arguments: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_system_creates_with_correct_role() {
        let msg = Message::system("系统初始化完成");
        assert_eq!(msg.role, Role::System);
        assert_eq!(msg.content, "系统初始化完成");
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
    }

    #[test]
    fn message_user_creates_with_correct_role() {
        let msg = Message::user("请分析这个漏洞");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content, "请分析这个漏洞");
    }

    #[test]
    fn message_assistant_creates_with_correct_role() {
        let msg = Message::assistant("这是一个 XSS 漏洞");
        assert_eq!(msg.role, Role::Assistant);
        assert_eq!(msg.content, "这是一个 XSS 漏洞");
    }

    #[test]
    fn message_tool_creates_with_tool_call_id() {
        let msg = Message::tool("call_abc123", "扫描完成");
        assert_eq!(msg.role, Role::Tool);
        assert_eq!(msg.content, "扫描完成");
        assert_eq!(msg.tool_call_id, Some("call_abc123".into()));
    }

    #[test]
    fn message_system_handles_empty_content() {
        let msg = Message::system("");
        assert_eq!(msg.content, "");
    }

    #[test]
    fn message_user_handles_unicode() {
        let msg = Message::user("你好世界");
        assert_eq!(msg.content, "你好世界");
    }

    #[test]
    fn role_as_str_returns_lowercase() {
        assert_eq!(Role::System.as_str(), "system");
        assert_eq!(Role::User.as_str(), "user");
        assert_eq!(Role::Assistant.as_str(), "assistant");
        assert_eq!(Role::Tool.as_str(), "tool");
    }

    #[test]
    fn role_display_matches_as_str() {
        assert_eq!(Role::System.to_string(), "system");
        assert_eq!(Role::User.to_string(), "user");
        assert_eq!(Role::Assistant.to_string(), "assistant");
        assert_eq!(Role::Tool.to_string(), "tool");
    }

    #[test]
    fn role_deserialize_from_lowercase_json() {
        let json = r#""user""#;
        let role: Role = serde_json::from_str(json).unwrap();
        assert_eq!(role, Role::User);
    }

    #[test]
    fn role_serialize_to_lowercase_json() {
        let role = Role::Assistant;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, r#""assistant""#);
    }

    #[test]
    fn usage_default_is_zero() {
        let usage = Usage::default();
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    #[test]
    fn usage_cost_calculates_correctly() {
        let usage = Usage {
            prompt_tokens: 5000,
            completion_tokens: 3000,
            total_tokens: 8000,
        };
        let pricing = Pricing {
            prompt_price_per_m: 2.0,
            completion_price_per_m: 8.0,
        };
        let expected = (5000.0 / 1_000_000.0) * 2.0 + (3000.0 / 1_000_000.0) * 8.0;
        let cost = usage.cost(&pricing);
        assert!((cost - expected).abs() < 0.0001);
    }

    #[test]
    fn usage_cost_zero_when_no_tokens() {
        let usage = Usage::default();
        let pricing = Pricing {
            prompt_price_per_m: 2.0,
            completion_price_per_m: 8.0,
        };
        assert_eq!(usage.cost(&pricing), 0.0);
    }

    #[test]
    fn usage_cost_scales_linearly() {
        let pricing = Pricing {
            prompt_price_per_m: 2.0,
            completion_price_per_m: 8.0,
        };
        let small = Usage {
            prompt_tokens: 1000,
            completion_tokens: 0,
            total_tokens: 1000,
        };
        let large = Usage {
            prompt_tokens: 10000,
            completion_tokens: 0,
            total_tokens: 10000,
        };
        assert!((large.cost(&pricing) - small.cost(&pricing) * 10.0).abs() < 0.0001);
    }

    #[test]
    fn api_message_from_user_message() {
        let msg = Message::user("测试消息");
        let api: ApiMessage = (&msg).into();
        assert_eq!(api.role, "user");
        assert_eq!(api.content, "测试消息");
        assert!(api.tool_calls.is_none());
        assert!(api.tool_call_id.is_none());
    }

    #[test]
    fn api_message_from_tool_message_preserves_tool_call_id() {
        let msg = Message::tool("call_456", "执行结果");
        let api: ApiMessage = (&msg).into();
        assert_eq!(api.role, "tool");
        assert_eq!(api.content, "执行结果");
        assert_eq!(api.tool_call_id, Some("call_456".into()));
    }

    #[test]
    fn pending_tool_call_new_initializes_empty() {
        let ptc = PendingToolCall::new("call_001".into());
        assert_eq!(ptc.id, "call_001");
        assert_eq!(ptc.name, "");
        assert_eq!(ptc.arguments, "");
    }

    #[test]
    fn pending_tool_call_accumulates_data() {
        let mut ptc = PendingToolCall::new("call_002".into());
        ptc.name.push_str("run_nmap");
        ptc.arguments.push_str(r#"{"target":"192.168.1.1"}"#);
        assert_eq!(ptc.name, "run_nmap");
        assert_eq!(ptc.arguments, r#"{"target":"192.168.1.1"}"#);
    }

    #[test]
    fn tool_call_has_function_type() {
        let tc = ToolCall {
            id: "t1".into(),
            call_type: "function".into(),
            function: ToolFunction {
                name: "test_tool".into(),
                arguments: r#"{}"#.into(),
            },
        };
        assert_eq!(tc.call_type, "function");
    }
}
