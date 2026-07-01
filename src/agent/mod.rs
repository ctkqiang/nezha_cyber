//! Agent 模块 —— 智能体配置管理与多 Agent 调度。
//!
//! 负责加载 YAML 配置文件、管理 Agent 实例、切换 Agent 上下文。
//! 包含红队各角色的系统提示词库。

pub mod config;
pub mod prompts;
