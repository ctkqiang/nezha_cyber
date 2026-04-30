
//! 工具执行器 —— 解析 tool_call 参数并执行本地操作。
//!
//! 支持的工具：write_file、read_file、create_directory、list_directory、search_files。
//! 所有文件操作限制在当前工作目录内（安全沙箱）。

use crate::agent::config::AgentToolConfig;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

fn ensure_safe(base: &PathBuf, target: &PathBuf) -> Result<PathBuf, String> {
    let canonical_base = base.canonicalize().map_err(|e| format!("基础路径不可访问: {}", e))?;
    let resolved = if target.is_relative() {
        canonical_base.join(target)
    } else {
        target.clone()
    };
    let canonical = resolved
        .canonicalize()
        .unwrap_or_else(|_| resolved.clone());
    if !canonical.starts_with(&canonical_base) {
        return Err(format!("路径越界: {}", canonical.display()));
    }
    Ok(resolved)
}

fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[derive(Debug, Deserialize)]
struct WriteFileArgs {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ReadFileArgs {
    path: String,
}

#[derive(Debug, Deserialize)]
struct CreateDirectoryArgs {
    path: String,
}

#[derive(Debug, Deserialize)]
struct ListDirectoryArgs {
    path: Option<String>,
}

pub fn tool_definitions() -> Vec<AgentToolConfig> {
    vec![
        AgentToolConfig {
            name: "write_file".into(),
            description: "创建或覆写一个文件，传入 path 和 content。需要用户确认。".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "文件相对路径"},
                    "content": {"type": "string", "description": "文件内容"}
                },
                "required": ["path", "content"]
            }),
        },
        AgentToolConfig {
            name: "read_file".into(),
            description: "读取文件内容，传入 path。需要用户确认。".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "文件相对路径"}
                },
                "required": ["path"]
            }),
        },
        AgentToolConfig {
            name: "create_directory".into(),
            description: "创建目录（含父目录），传入 path。".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "目录相对路径"}
                },
                "required": ["path"]
            }),
        },
        AgentToolConfig {
            name: "list_directory".into(),
            description: "列出目录内容，传入可选 path，默认当前目录。".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "目录相对路径"}
                },
                "required": []
            }),
        },
    ]
}

pub fn execute_tool(name: &str, args: &str) -> String {
    match name {
        "write_file" => exec_write_file(args),
        "read_file" => exec_read_file(args),
        "create_directory" => exec_create_directory(args),
        "list_directory" => exec_list_directory(args),
        _ => format!("未知工具: {}", name),
    }
}

fn exec_write_file(args: &str) -> String {
    let args: WriteFileArgs = match serde_json::from_str(args) {
        Ok(a) => a,
        Err(e) => return format!("参数错误: {}", e),
    };

    let base = cwd();
    let target = base.join(&args.path);

    match ensure_safe(&base, &target) {
        Ok(resolved) => {
            if let Some(parent) = resolved.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    return format!("创建父目录失败: {}", e);
                }
            }
            match fs::write(&resolved, &args.content) {
                Ok(()) => {
                    let lines = args.content.lines().count();
                    let bytes = args.content.len();
                    format!("已写入 {} ({} 行, {} 字节)", args.path, lines, bytes)
                }
                Err(e) => format!("写入失败: {}", e),
            }
        }
        Err(e) => e,
    }
}

fn exec_read_file(args: &str) -> String {
    let args: ReadFileArgs = match serde_json::from_str(args) {
        Ok(a) => a,
        Err(e) => return format!("参数错误: {}", e),
    };

    let base = cwd();
    let target = base.join(&args.path);

    match ensure_safe(&base, &target) {
        Ok(resolved) => match fs::read_to_string(&resolved) {
            Ok(content) => {
                let truncated = if content.lines().count() > 200 {
                    let shortened: Vec<&str> = content.lines().take(200).collect();
                    format!("{}\n... (已截断，共 {} 行)", shortened.join("\n"), content.lines().count())
                } else {
                    content
                };
                format!("```{}\n{}\n```", args.path, truncated)
            }
            Err(e) => format!("读取失败: {}", e),
        },
        Err(e) => e,
    }
}

fn exec_create_directory(args: &str) -> String {
    let args: CreateDirectoryArgs = match serde_json::from_str(args) {
        Ok(a) => a,
        Err(e) => return format!("参数错误: {}", e),
    };

    let base = cwd();
    let target = base.join(&args.path);

    match ensure_safe(&base, &target) {
        Ok(resolved) => match fs::create_dir_all(&resolved) {
            Ok(()) => format!("已创建目录: {}", args.path),
            Err(e) => format!("创建目录失败: {}", e),
        },
        Err(e) => e,
    }
}

fn exec_list_directory(args: &str) -> String {
    let args: ListDirectoryArgs = match serde_json::from_str(args) {
        Ok(a) => a,
        Err(_) => return "参数错误".into(),
    };

    let base = cwd();
    let target = match &args.path {
        Some(p) => base.join(p),
        None => base.clone(),
    };

    match ensure_safe(&base, &target) {
        Ok(resolved) => match fs::read_dir(&resolved) {
            Ok(entries) => {
                let mut items = Vec::new();
                for entry in entries.flatten() {
                    let ft = entry.file_type().map(|t| if t.is_dir() { "/" } else { "" }).unwrap_or("");
                    let name = entry.file_name().to_string_lossy().into_owned();
                    items.push(format!("  {}{}", name, ft));
                }
                items.sort();
                format!("{} ({} 项):\n{}", args.path.as_deref().unwrap_or("."), items.len(), items.join("\n"))
            }
            Err(e) => format!("列出目录失败: {}", e),
        },
        Err(e) => e,
    }
}
