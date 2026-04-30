
//! 对话持久化模块 —— 基于 SQLite 的会话记忆存储与检索。
//!
//! 提供完整的 CRUD 操作：保存对话到 SQLite、按条件加载历史、删除过期会话。
//! 通过用户偏好控制自动保存策略（on_exit / always / never）。

pub mod db;
pub use db::MemoryStore;
