//! DeepSeek API 流式客户端。
//!
//! 封装与 DeepSeek Chat Completions API 的通信，支持：
//! - SSE 流式对话（逐 token 返回）
//! - Function Calling（工具调用）
//! - Token 用量统计与费用计算
//! - 账户余额查询
//! - 错误重试与流异常处理
//!
//! 所有 API 调用均在后台 tokio 任务中执行，通过 mpsc 通道将结果传回主循环。

use futures::StreamExt;
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use super::types::{
    ApiMessage, BalanceInfo, ChatCompletionRequest, PendingToolCall, StreamChoice,
    StreamResponse, StreamToolCallDelta, Tool, Usage,
};
use crate::action::Action;

/// DeepSeek 客户端配置
#[derive(Debug, Clone)]
pub struct DeepSeekConfig {
    pub api_base: String,
    pub api_key: String,
    pub model: String,
}

/// DeepSeek API 客户端
///
/// 持有 HTTP 客户端实例与 API 配置，提供流式对话与余额查询方法。
#[derive(Debug, Clone)]
pub struct DeepSeekClient {
    config: Arc<DeepSeekConfig>,
    client: Client,
}

impl DeepSeekClient {
    /// 创建新的 DeepSeek 客户端实例
    pub fn new(config: DeepSeekConfig) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("构建 HTTP 客户端失败");
        Self {
            config: Arc::new(config),
            client,
        }
    }

    /// 获取当前使用的模型名称
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// 更新模型名称
    pub fn set_model(&mut self, model: String) {
        Arc::make_mut(&mut self.config).model = model;
    }

    /// 获取 API 密钥引用
    pub fn api_key(&self) -> &str {
        &self.config.api_key
    }

    /// 获取 API 基础 URL 引用
    pub fn api_base(&self) -> &str {
        &self.config.api_base
    }

    /// 查询账户余额
    pub async fn check_balance(&self) -> Result<BalanceInfo, String> {
        let url = format!("{}/user/balance", self.config.api_base);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("余额查询请求失败: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("余额查询 API 返回错误 {}: {}", status.as_u16(), body));
        }

        let balance: BalanceInfo = response
            .json()
            .await
            .map_err(|e| format!("余额数据解析失败: {}", e))?;

        Ok(balance)
    }

    /// 发起流式对话请求（异步后台任务）
    ///
    /// # 参数
    /// - `tx`: Action 发送通道，用于将流式事件推回主循环
    /// - `tab_id`: 目标标签页 ID
    /// - `messages`: 对话历史（不含 id 的 API 格式）
    /// - `tools`: 可用的工具定义列表
    /// - `temperature`: 采样温度，None 使用默认值
    ///
    /// # 副作用
    /// 通过 `tx` 依次发送 `StreamStart`、`StreamChunk`、`StreamDone` 或 `StreamError`。
    pub fn stream_chat(
        &self,
        tx: UnboundedSender<Action>,
        tab_id: usize,
        messages: Vec<ApiMessage>,
        tools: Option<Vec<Tool>>,
        temperature: Option<f32>,
    ) {
        let config = self.config.clone();
        let client = self.client.clone();
        let message_id = Uuid::new_v4();

        tokio::spawn(async move {
            let request = ChatCompletionRequest {
                model: config.model.clone(),
                messages,
                tools,
                temperature,
                max_tokens: Some(4096),
                stream: true,
            };

            let url = format!("{}/chat/completions", config.api_base);

            let response = match client
                .post(&url)
                .header("Authorization", format!("Bearer {}", config.api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    let _ = tx.send(Action::StreamError {
                        tab_id,
                        message_id,
                        error: format!("HTTP 请求失败: {}", e),
                    });
                    return;
                }
            };

            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                let _ = tx.send(Action::StreamError {
                    tab_id,
                    message_id,
                    error: format!("API 返回错误 {}: {}", status.as_u16(), body),
                });
                return;
            }

            let _ = tx.send(Action::StreamStart { tab_id, message_id });

            let mut stream = response.bytes_stream();
            let mut content_buffer = String::new();
            let mut pending_tool_calls: Vec<PendingToolCall> = Vec::new();
            let mut final_usage: Option<Usage> = None;

            while let Some(chunk_result) = stream.next().await {
                let chunk_bytes = match chunk_result {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        let _ = tx.send(Action::StreamError {
                            tab_id,
                            message_id,
                            error: format!("流读取错误: {}", e),
                        });
                        return;
                    }
                };

                let chunk_text = String::from_utf8_lossy(&chunk_bytes);
                for line in chunk_text.lines() {
                    let line = line.trim();
                    if line.is_empty() || line == "data: [DONE]" {
                        if line == "data: [DONE]" {
                            continue;
                        }
                        continue;
                    }
                    let Some(data) = line.strip_prefix("data: ") else {
                        continue;
                    };

                    let parsed: StreamResponse = match serde_json::from_str(data) {
                        Ok(r) => r,
                        Err(_) => continue,
                    };

                    if let Some(usage) = parsed.usage {
                        final_usage = Some(usage);
                    }

                    for choice in parsed.choices {
                        process_stream_choice(
                            &tx,
                            tab_id,
                            message_id,
                            &choice,
                            &mut content_buffer,
                            &mut pending_tool_calls,
                        );
                    }
                }
            }

            let _ = tx.send(Action::StreamDone {
                tab_id,
                message_id,
                content: content_buffer,
                usage: final_usage.unwrap_or_default(),
            });
        });
    }
}

/// 处理流式响应中的单个 choice，分发内容增量或工具调用增量
fn process_stream_choice(
    tx: &UnboundedSender<Action>,
    tab_id: usize,
    message_id: Uuid,
    choice: &StreamChoice,
    content_buffer: &mut String,
    pending_tool_calls: &mut Vec<PendingToolCall>,
) {
    if let Some(content) = &choice.delta.content {
        content_buffer.push_str(content);
        let _ = tx.send(Action::StreamChunk {
            tab_id,
            message_id,
            chunk: content.clone(),
        });
    }

    if let Some(tool_call_deltas) = &choice.delta.tool_calls {
        for delta in tool_call_deltas {
            apply_tool_call_delta(tx, tab_id, pending_tool_calls, delta);
        }
    }
}

/// 将 tool_calls delta 片段累积到 pending_tool_calls 中
fn apply_tool_call_delta(
    tx: &UnboundedSender<Action>,
    tab_id: usize,
    pending: &mut Vec<PendingToolCall>,
    delta: &StreamToolCallDelta,
) {
    while pending.len() <= delta.index {
        pending.push(PendingToolCall::new(String::new()));
    }

    let entry = &mut pending[delta.index];

    if let Some(id) = &delta.id {
        entry.id = id.clone();
    }

    if let Some(func) = &delta.function {
        if let Some(name) = &func.name {
            entry.name.push_str(name);
        }
        if let Some(args) = &func.arguments {
            entry.arguments.push_str(args);
        }
    }

    if !entry.name.is_empty() && !entry.id.is_empty() && !entry.arguments.is_empty() {
        let args_value: serde_json::Value =
            serde_json::from_str(&entry.arguments).unwrap_or(serde_json::Value::Null);

        let _ = tx.send(Action::ToolCallRequest {
            tab_id,
            call_id: entry.id.clone(),
            name: entry.name.clone(),
            args: args_value,
        });
    }
}
