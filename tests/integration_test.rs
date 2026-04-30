//! 哪吒网络安全 TUI —— 集成测试
//!
//! 模拟完整的用户操作流程，验证各模块协作的正确性。

use nezha_cyber::action::Action;
use nezha_cyber::agent::config::{AgentConfig, AppConfig, DefaultPricing};
use nezha_cyber::api::types::{Role, Usage};
use nezha_cyber::app::{update, App, Focus, ToolCallStatus};
use uuid::Uuid;

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
        name: "红队渗透助手".into(),
        description: "测试".into(),
        system_prompt: "你是一个测试助手".into(),
        model: "deepseek-chat".into(),
        tools: vec![],
    }]
}

fn make_app() -> App {
    App::new(test_agents(), test_app_config())
}

// ---- 完整对话流程集成测试 ----

#[test]
fn full_conversation_flow() {
    let mut app = make_app();

    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.active_tab, 0);

    update(&mut app, Action::SubmitMessage("请帮我分析这个漏洞".into()));

    assert_eq!(app.active_tab().messages.len(), 1);
    assert_eq!(app.active_tab().messages[0].role, Role::User);
    assert!(app.active_tab().input_buffer.is_empty());

    let msg_id = Uuid::new_v4();
    update(
        &mut app,
        Action::StreamStart {
            tab_id: 0,
            message_id: msg_id,
        },
    );

    assert_eq!(app.active_tab().messages.len(), 2);
    assert_eq!(app.active_tab().messages[1].role, Role::Assistant);

    update(
        &mut app,
        Action::StreamChunk {
            tab_id: 0,
            message_id: msg_id,
            chunk: "这是一个典型的".into(),
        },
    );
    update(
        &mut app,
        Action::StreamChunk {
            tab_id: 0,
            message_id: msg_id,
            chunk: " XSS 漏洞".into(),
        },
    );

    let msg = app.tabs[0]
        .messages
        .iter()
        .find(|m| m.id == msg_id)
        .unwrap();
    assert!(msg.content.contains("XSS 漏洞"));

    update(
        &mut app,
        Action::StreamDone {
            tab_id: 0,
            message_id: msg_id,
            content: "这是一个典型的 XSS 漏洞".into(),
            usage: Usage {
                prompt_tokens: 200,
                completion_tokens: 80,
                total_tokens: 280,
            },
        },
    );

    assert_eq!(app.tabs[0].total_usage.prompt_tokens, 200);
    assert_eq!(app.tabs[0].total_usage.completion_tokens, 80);
}

// ---- 多标签页工作流集成测试 ----

#[test]
fn multi_tab_workflow() {
    let mut app = make_app();

    update(&mut app, Action::SubmitMessage("Tab0消息".into()));
    update(&mut app, Action::NewTab);
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);

    update(&mut app, Action::SubmitMessage("Tab1消息".into()));

    assert_eq!(app.tabs[0].messages.len(), 1);
    assert_eq!(app.tabs[1].messages.len(), 1);
    assert_eq!(app.tabs[0].messages[0].content, "Tab0消息");
    assert_eq!(app.tabs[1].messages[0].content, "Tab1消息");

    update(&mut app, Action::SwitchTab(0));
    assert_eq!(app.active_tab, 0);

    update(&mut app, Action::SubmitMessage("Tab0第二条消息".into()));
    assert_eq!(app.tabs[0].messages.len(), 2);

    update(&mut app, Action::CloseTab(1));
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.active_tab, 0);
}

// ---- 工具调用流程集成测试 ----

#[test]
fn tool_call_integration_flow() {
    let mut app = make_app();

    update(
        &mut app,
        Action::SubmitMessage("扫描 192.168.1.1 的端口".into()),
    );

    let msg_id = Uuid::new_v4();
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
            call_id: "call_nmap".into(),
            name: "run_nmap".into(),
            args: r#"{"target": "192.168.1.1", "ports": "1-1000"}"#.into(),
        },
    );

    assert!(app.tabs[0].pending_tool_calls.contains_key("call_nmap"));

    let last_msg = app.tabs[0].messages.last().unwrap();
    assert!(last_msg.tool_calls.is_some());

    update(
        &mut app,
        Action::ToolCallResponse {
            tab_id: 0,
            call_id: "call_nmap".into(),
            result: "22/tcp open ssh, 80/tcp open http".into(),
        },
    );

    let tool_msg = app.tabs[0].messages.last().unwrap();
    assert_eq!(tool_msg.role, Role::Tool);
    assert_eq!(tool_msg.tool_call_id, Some("call_nmap".into()));
}

// ---- 用户确认工具调用集成测试 ----

#[test]
fn confirm_tool_call_integration() {
    let mut app = make_app();
    let msg_id = Uuid::new_v4();

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
            call_id: "call_risky".into(),
            name: "run_exploit".into(),
            args: r#"{"target": "10.0.0.1"}"#.into(),
        },
    );

    let confirmation = nezha_cyber::action::ToolCallConfirmation {
        tab_id: 0,
        call_id: "call_risky".into(),
        name: "run_exploit".into(),
        args: r#"{"target": "10.0.0.1"}"#.into(),
    };
    update(&mut app, Action::ConfirmToolCall(confirmation));

    let status = app.tabs[0].pending_tool_calls.get("call_risky").unwrap();
    assert!(matches!(status, ToolCallStatus::Running));
}

// ---- 流错误处理集成测试 ----

#[test]
fn stream_error_handling_integration() {
    let mut app = make_app();

    update(&mut app, Action::SubmitMessage("测试连接".into()));

    let msg_id = Uuid::new_v4();
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
            error: "网络连接超时".into(),
        },
    );

    let error_msg = app.tabs[0]
        .messages
        .iter()
        .find(|m| m.id == msg_id)
        .unwrap();
    assert!(error_msg.content.contains("网络连接超时"));
}

// ---- 主题模型切换集成测试 ----

#[test]
fn theme_and_model_switching_integration() {
    let mut app = make_app();

    assert_eq!(app.theme, "default");
    update(&mut app, Action::ChangeTheme("daylight".into()));
    assert_eq!(app.theme, "daylight");

    update(
        &mut app,
        Action::ChangeModel {
            tab_id: 0,
            model: "deepseek-reasoner".into(),
        },
    );
    assert_eq!(app.tabs[0].model, "deepseek-reasoner");

    update(&mut app, Action::NewTab);
    update(
        &mut app,
        Action::ChangeModel {
            tab_id: 1,
            model: "deepseek-chat".into(),
        },
    );
    assert_eq!(app.tabs[0].model, "deepseek-reasoner");
    assert_eq!(app.tabs[1].model, "deepseek-chat");
}

// ---- 侧边栏与命令面板交互集成测试 ----

#[test]
fn sidebar_and_palette_interaction_integration() {
    let mut app = make_app();

    assert!(app.sidebar_visible);
    assert!(!app.command_palette_open);

    update(&mut app, Action::ToggleSidebar);
    assert!(!app.sidebar_visible);

    update(&mut app, Action::OpenCommandPalette);
    assert!(app.command_palette_open);
    assert_eq!(app.focus, Focus::CommandPalette);

    update(&mut app, Action::ToggleSidebar);
    assert!(app.sidebar_visible);

    update(&mut app, Action::OpenCommandPalette);
    assert!(!app.command_palette_open);
    assert_eq!(app.focus, Focus::ChatInput);
}

// ---- 退出流程集成测试 ----

#[test]
fn shutdown_flow_integration() {
    let mut app = make_app();

    assert!(!app.should_quit);
    update(&mut app, Action::Shutdown);
    assert!(app.should_quit);
}

// ---- 空消息不触发集成测试 ----

#[test]
fn empty_message_is_ignored_integration() {
    let mut app = make_app();

    let initial = app.active_tab().messages.len();
    update(&mut app, Action::SubmitMessage("".into()));
    assert_eq!(app.active_tab().messages.len(), initial);

    update(&mut app, Action::SubmitMessage("   ".into()));
    assert_eq!(app.active_tab().messages.len(), initial);

    update(&mut app, Action::SubmitMessage("有效消息".into()));
    assert_eq!(app.active_tab().messages.len(), initial + 1);
}
