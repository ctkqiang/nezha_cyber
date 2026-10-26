//! DeepSeek API 流式客户端。
//!
//! 封装与 DeepSeek Chat Completions API 的通信，支持：
//! - SSE 流式对话（逐 token 返回）
//! - Function Calling（工具调用）
//! - Token 用量统计与费用计算
//! - 账户余额查询
//! - 可取消的流式请求
//!
//! 所有 API 调用均在后台 tokio 任务中执行，通过 mpsc 通道将结果传回主循环。

use futures::StreamExt;
use reqwest::Client;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use super::types::{
    ApiMessage, BalanceInfo, ChatCompletionRequest, PendingToolCall, StreamResponse,
    StreamToolCallDelta, Tool, Usage,
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

    /// 获取当前使用的默认模型名称
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// 更新默认模型名称
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
        if self.config.api_key.is_empty() {
            return Err("未设置 API Key，请设置 DEEPSEEK_TOKEN 环境变量或 .env 文件".into());
        }

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
            return Err(format!(
                "余额查询 API 返回错误 {}: {}",
                status.as_u16(),
                body
            ));
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
    /// - `model`: 本次请求使用的模型（每个标签页可以不同）
    /// - `messages`: 对话历史（不含 id 的 API 格式）
    /// - `tools`: 可用的工具定义列表
    /// - `temperature`: 采样温度，None 使用默认值
    /// - `cancel`: 取消标志，置位后任务尽快结束且不再发送事件
    ///
    /// # 副作用
    /// 通过 `tx` 依次发送 `StreamStart`、`StreamChunk`、`StreamDone` 或 `StreamError`，
    /// 若模型请求了工具调用，则随后发送 `ToolCallRequest`。
    #[allow(clippy::too_many_arguments)]
    pub fn stream_chat(
        &self,
        tx: UnboundedSender<Action>,
        tab_id: usize,
        model: String,
        messages: Vec<ApiMessage>,
        tools: Option<Vec<Tool>>,
        temperature: Option<f32>,
        cancel: Arc<AtomicBool>,
    ) {
        let config = self.config.clone();
        let client = self.client.clone();
        let message_id = Uuid::new_v4();

        tokio::spawn(async move {
            let request = ChatCompletionRequest {
                model,
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
                    if !is_cancelled(&cancel) {
                        let _ = tx.send(Action::StreamError {
                            tab_id,
                            message_id,
                            error: format!("HTTP 请求失败: {}", e),
                        });
                    }
                    return;
                }
            };

            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                if !is_cancelled(&cancel) {
                    let _ = tx.send(Action::StreamError {
                        tab_id,
                        message_id,
                        error: format!(
                            "API 返回错误 {}: {}",
                            status.as_u16(),
                            truncate(&body, 400)
                        ),
                    });
                }
                return;
            }

            if is_cancelled(&cancel) {
                return;
            }
            let _ = tx.send(Action::StreamStart { tab_id, message_id });

            let mut stream = response.bytes_stream();
            let mut sse_buffer: Vec<u8> = Vec::new();
            let mut content_buffer = String::new();
            let mut pending_tool_calls: Vec<PendingToolCall> = Vec::new();
            let mut final_usage: Option<Usage> = None;

            'stream: while let Some(chunk_result) = stream.next().await {
                if is_cancelled(&cancel) {
                    return;
                }

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

                // SSE 事件可能被任意拆分到多个 TCP chunk 中，
                // 先写入字节缓冲区，凑齐一整行后再做 UTF-8 解码，
                // 避免中文 token 恰好被拆在 UTF-8 字节边界上而乱码。
                sse_buffer.extend_from_slice(&chunk_bytes);

                while let Some(pos) = sse_buffer.iter().position(|byte| *byte == b'\n') {
                    let raw_line: Vec<u8> = sse_buffer.drain(..=pos).collect();
                    let decoded = String::from_utf8_lossy(&raw_line);
                    let line = decoded.trim();
                    if line.is_empty() {
                        continue;
                    }
                    let Some(data) = line.strip_prefix("data:") else {
                        continue;
                    };
                    let data = data.trim();
                    if data == "[DONE]" {
                        break 'stream;
                    }
                    let Ok(parsed) = serde_json::from_str::<StreamResponse>(data) else {
                        continue;
                    };

                    if let Some(usage) = parsed.usage {
                        final_usage = Some(usage);
                    }

                    for choice in &parsed.choices {
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
                                apply_tool_call_delta(&mut pending_tool_calls, delta);
                            }
                        }
                    }
                }
            }

            if is_cancelled(&cancel) {
                return;
            }

            // 工具调用参数在流中会分片到达，等流结束后再一次性交给 UI，
            // 避免用户拿到半截参数、也避免重复弹出确认框。
            // 注意：必须先发工具调用、再发 StreamDone，这样 StreamDone 才能
            // 判断「本轮只有工具调用、没有正文」，而不是误显示「(空响应)」。
            let has_tool_calls = pending_tool_calls
                .iter()
                .any(|call| !call.id.is_empty() && !call.name.is_empty());
            for call in pending_tool_calls {
                if call.id.is_empty() || call.name.is_empty() {
                    continue;
                }
                let _ = tx.send(Action::ToolCallRequest {
                    tab_id,
                    call_id: call.id,
                    name: call.name,
                    args: call.arguments,
                });
            }

            let _ = tx.send(Action::StreamDone {
                tab_id,
                message_id,
                content: content_buffer,
                usage: final_usage.unwrap_or_default(),
                has_tool_calls,
            });
        });
    }
}

fn is_cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// 将 tool_calls delta 片段累积到 pending_tool_calls 中
fn apply_tool_call_delta(pending: &mut Vec<PendingToolCall>, delta: &StreamToolCallDelta) {
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
}
