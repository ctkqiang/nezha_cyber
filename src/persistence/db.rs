
//! SQLite 数据库连接管理与 CRUD 操作。
//!
//! 使用 rusqlite (bundled) 内嵌 SQLite，无需系统安装。
//! 数据库文件存储在用户当前目录下的 `.nezha_memory.db`。

use rusqlite::{params, Connection, Result as SqliteResult};
use std::path::PathBuf;

use crate::api::types::{Message, Role, ToolCall};

fn db_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join(".nezha_memory.db")
}

/// 数据库迁移 DDL
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS conversations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    agent_name TEXT NOT NULL,
    model TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    is_favorited INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    tool_calls TEXT,
    tool_call_id TEXT,
    seq INTEGER NOT NULL,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS preferences (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR IGNORE INTO preferences (key, value) VALUES ('auto_save', 'on_exit');
INSERT OR IGNORE INTO preferences (key, value) VALUES ('max_conversations', '50');
";

/// 对话摘要，用于列表展示
#[derive(Debug, Clone)]
pub struct ConversationSummary {
    pub id: i64,
    pub title: String,
    pub agent_name: String,
    pub model: String,
    pub created_at: String,
    pub message_count: usize,
    pub is_favorited: bool,
}

/// SQLite 会话记忆存储
///
/// 管理数据库连接生命周期，提供对话的保存、加载、删除与偏好查询。
pub struct MemoryStore {
    conn: Connection,
}

impl MemoryStore {
    /// 打开或创建数据库连接并执行迁移
    pub fn open() -> SqliteResult<Self> {
        let conn = Connection::open(db_path())?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// 保存一个完整对话（事务包裹：插入 conversation + 批量插入 messages）
    pub fn save_conversation(
        &self,
        title: &str,
        agent_name: &str,
        model: &str,
        messages: &[Message],
    ) -> SqliteResult<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.conn.unchecked_transaction()?;

        tx.execute(
            "INSERT INTO conversations (title, agent_name, model, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![title, agent_name, model, now, now],
        )?;
        let conv_id = tx.last_insert_rowid();

        for (i, msg) in messages.iter().enumerate() {
            let tool_calls_json = msg
                .tool_calls
                .as_ref()
                .map(|tc| serde_json::to_string(tc).unwrap_or_default());

            tx.execute(
                "INSERT INTO messages (conversation_id, role, content, tool_calls, tool_call_id, seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    conv_id,
                    msg.role.as_str(),
                    msg.content,
                    tool_calls_json,
                    msg.tool_call_id,
                    i as i64,
                ],
            )?;
        }

        tx.commit()?;
        Ok(conv_id)
    }

    /// 加载指定对话的完整消息列表
    pub fn load_conversation(&self, conversation_id: i64) -> SqliteResult<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT role, content, tool_calls, tool_call_id FROM messages WHERE conversation_id = ?1 ORDER BY seq ASC",
        )?;

        let rows = stmt.query_map(params![conversation_id], |row| {
            let role_str: String = row.get(0)?;
            let content: String = row.get(1)?;
            let tool_calls_json: Option<String> = row.get(2)?;
            let tool_call_id: Option<String> = row.get(3)?;
            Ok((role_str, content, tool_calls_json, tool_call_id))
        })?;

        let mut messages = Vec::new();
        for row in rows {
            let (role_str, content, tool_calls_json, tool_call_id) = row?;
            let role = Role::from_str(&role_str);
            let tool_calls = tool_calls_json
                .and_then(|s| serde_json::from_str::<Vec<ToolCall>>(&s).ok());

            messages.push(Message {
                id: uuid::Uuid::new_v4(),
                role,
                content,
                tool_calls,
                tool_call_id,
                name: None,
            });
        }
        Ok(messages)
    }

    /// 列出所有已保存的对话摘要（按更新时间倒序）
    pub fn list_conversations(&self) -> SqliteResult<Vec<ConversationSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.title, c.agent_name, c.model, c.created_at, c.is_favorited, COUNT(m.id) as msg_count
             FROM conversations c
             LEFT JOIN messages m ON m.conversation_id = c.id
             GROUP BY c.id
             ORDER BY c.updated_at DESC
             LIMIT 100",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(ConversationSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                agent_name: row.get(2)?,
                model: row.get(3)?,
                created_at: row.get(4)?,
                is_favorited: row.get::<_, i64>(5)? != 0,
                message_count: row.get::<_, i64>(6)? as usize,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// 删除指定对话及其所有消息
    pub fn delete_conversation(&self, conversation_id: i64) -> SqliteResult<()> {
        self.conn
            .execute("DELETE FROM conversations WHERE id = ?1", params![conversation_id])?;
        Ok(())
    }

    /// 收藏/取消收藏对话
    pub fn toggle_favorite(&self, conversation_id: i64) -> SqliteResult<bool> {
        let current: i64 = self
            .conn
            .query_row(
                "SELECT is_favorited FROM conversations WHERE id = ?1",
                params![conversation_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let new_val = if current == 0 { 1i64 } else { 0i64 };
        self.conn.execute(
            "UPDATE conversations SET is_favorited = ?1 WHERE id = ?2",
            params![new_val, conversation_id],
        )?;
        Ok(new_val != 0)
    }

    /// 读取用户偏好值
    pub fn get_preference(&self, key: &str) -> SqliteResult<Option<String>> {
        let result = self.conn.query_row(
            "SELECT value FROM preferences WHERE key = ?1",
            params![key],
            |row| row.get(0),
        );
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 设置用户偏好值
    pub fn set_preference(&self, key: &str, value: &str) -> SqliteResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO preferences (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// 清理超出数量上限的旧对话（保留最近 N 个）
    pub fn trim_old_conversations(&self) -> SqliteResult<usize> {
        let max_str = self
            .get_preference("max_conversations")?
            .unwrap_or_else(|| "50".into());
        let max: usize = max_str.parse().unwrap_or(50);

        let deleted = self.conn.execute(
            "DELETE FROM conversations WHERE id NOT IN (
                SELECT id FROM conversations ORDER BY updated_at DESC LIMIT ?1
            )",
            params![max as i64],
        )?;
        Ok(deleted)
    }
}

/// Role 辅助方法
impl Role {
    pub fn from_str(s: &str) -> Self {
        match s {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "system" => Role::System,
            "tool" => Role::Tool,
            _ => Role::User,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::Message;

    fn make_store() -> MemoryStore {
        let tmp = std::env::temp_dir().join(format!("nezha_test_{}.db", uuid::Uuid::new_v4()));
        let conn = Connection::open(&tmp).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        MemoryStore { conn }
    }

    fn sample_messages() -> Vec<Message> {
        vec![
            Message::user("你好"),
            Message::assistant("你好！有什么可以帮你的？"),
            Message::user("帮我分析一下这个端口扫描结果"),
            Message::assistant("22端口开放SSH，80端口开放HTTP，建议关闭22或限制IP访问"),
        ]
    }

    #[test]
    fn save_and_load_conversation_roundtrip() {
        let store = make_store();
        let msgs = sample_messages();
        let id = store
            .save_conversation("测试会话", "哪吒", "deepseek-v4-pro", &msgs)
            .unwrap();
        assert!(id > 0);

        let loaded = store.load_conversation(id).unwrap();
        assert_eq!(loaded.len(), 4);
        assert_eq!(loaded[0].role, Role::User);
        assert_eq!(loaded[0].content, "你好");
        assert_eq!(loaded[1].role, Role::Assistant);
        assert_eq!(loaded[3].content, "22端口开放SSH，80端口开放HTTP，建议关闭22或限制IP访问");
    }

    #[test]
    fn list_conversations_returns_correct_summary() {
        let store = make_store();
        store
            .save_conversation("会话A", "哪吒", "deepseek-v4-pro", &sample_messages())
            .unwrap();
        store
            .save_conversation("会话B", "代码审计专家", "deepseek-v4-pro", &sample_messages())
            .unwrap();

        let list = store.list_conversations().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].message_count, 4);
    }

    #[test]
    fn delete_conversation_removes_from_list() {
        let store = make_store();
        let id = store
            .save_conversation("待删除", "哪吒", "deepseek-v4-pro", &sample_messages())
            .unwrap();
        store.delete_conversation(id).unwrap();
        let list = store.list_conversations().unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn toggle_favorite_flips_status() {
        let store = make_store();
        let id = store
            .save_conversation("收藏测试", "哪吒", "deepseek-v4-pro", &sample_messages())
            .unwrap();

        let fav = store.toggle_favorite(id).unwrap();
        assert!(fav);

        let fav = store.toggle_favorite(id).unwrap();
        assert!(!fav);
    }

    #[test]
    fn get_set_preference_roundtrip() {
        let store = make_store();
        store.set_preference("auto_save", "always").unwrap();
        let val = store.get_preference("auto_save").unwrap();
        assert_eq!(val, Some("always".into()));
    }

    #[test]
    fn get_nonexistent_preference_returns_none() {
        let store = make_store();
        let val = store.get_preference("nonexistent_key").unwrap();
        assert_eq!(val, None);
    }

    #[test]
    fn trim_old_conversations_respects_limit() {
        let store = make_store();
        store.set_preference("max_conversations", "2").unwrap();
        for i in 0..5 {
            store
                .save_conversation(
                    &format!("会话{}", i),
                    "哪吒",
                    "deepseek-v4-pro",
                    &sample_messages(),
                )
                .unwrap();
        }
        let list = store.list_conversations().unwrap();
        assert_eq!(list.len(), 5);

        let deleted = store.trim_old_conversations().unwrap();
        assert_eq!(deleted, 3);

        let list = store.list_conversations().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn role_from_str_all_variants() {
        assert_eq!(Role::from_str("user"), Role::User);
        assert_eq!(Role::from_str("assistant"), Role::Assistant);
        assert_eq!(Role::from_str("system"), Role::System);
        assert_eq!(Role::from_str("tool"), Role::Tool);
    }

    #[test]
    fn role_from_str_unknown_defaults_to_user() {
        assert_eq!(Role::from_str("invalid_role"), Role::User);
    }

    #[test]
    fn save_empty_messages_still_creates_conversation() {
        let store = make_store();
        let id = store
            .save_conversation("空对话", "哪吒", "deepseek-v4-pro", &[])
            .unwrap();
        let loaded = store.load_conversation(id).unwrap();
        assert!(loaded.is_empty());
        let list = store.list_conversations().unwrap();
        assert_eq!(list[0].message_count, 0);
    }
}
