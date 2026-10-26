//! OCR 与文件内容提取 —— PDF / 图片 / 文本文件的文字提取。
//!
//! 支持三种文件格式：
//! - PDF: 使用 pdf-extract 库提取文本
//! - 图片 (png/jpg/jpeg/bmp/webp/tiff): 调用系统 tesseract CLI 进行 OCR
//! - 文本文件 (txt/md/rs/py 等): 直接读取文件内容
//!
//! 所有操作限制在当前工作目录内，禁止读取越界路径。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn is_safe_path(path: &Path) -> Result<PathBuf, String> {
    let base = cwd()
        .canonicalize()
        .map_err(|e| format!("当前目录不可访问: {}", e))?;
    let resolved = if path.is_relative() {
        base.join(path)
    } else {
        path.to_path_buf()
    };
    let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());
    if !canonical.starts_with(&base) {
        return Err(format!("路径越界: {}", resolved.display()));
    }
    Ok(resolved)
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext,
        "png" | "jpg" | "jpeg" | "bmp" | "webp" | "tiff" | "gif" | "tif"
    )
}

fn is_text_ext(ext: &str) -> bool {
    matches!(
        ext,
        "txt"
            | "md"
            | "rs"
            | "py"
            | "js"
            | "ts"
            | "go"
            | "java"
            | "c"
            | "cpp"
            | "h"
            | "hpp"
            | "toml"
            | "yaml"
            | "yml"
            | "json"
            | "xml"
            | "html"
            | "css"
            | "sh"
            | "bash"
            | "zsh"
            | "sql"
            | "r"
            | "swift"
            | "kt"
            | "env"
            | "cfg"
            | "ini"
            | "log"
            | "csv"
            | "vue"
            | "svelte"
            | "tf"
            | "proto"
            | "lua"
            | "rb"
            | "php"
            | "scala"
            | "clj"
            | "ex"
            | "exs"
            | "dart"
            | "nim"
            | "zig"
    )
}

/// 从文件路径提取文本内容
///
/// 根据文件扩展名自动选择提取策略：
/// - PDF → pdf-extract 库
/// - 图片 → tesseract CLI（需要系统安装 tesseract）
/// - 文本文件 → 直接读取
pub fn extract_file_text(path: &Path) -> Result<String, String> {
    let resolved = is_safe_path(path)?;

    if !resolved.exists() {
        return Err(format!("文件不存在: {}", path.display()));
    }

    let ext = extension_of(&resolved);

    if ext == "pdf" {
        return extract_pdf(&resolved);
    }

    if is_image_ext(&ext) {
        return extract_image_ocr(&resolved);
    }

    if is_text_ext(&ext) || ext.is_empty() {
        return extract_text_file(&resolved);
    }

    if let Ok(content) = fs::read_to_string(&resolved) {
        if !content.contains('\0') {
            return Ok(truncate_content(&content));
        }
    }

    Err(format!("不支持的文件格式: {}", ext))
}

fn extract_pdf(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("读取 PDF 失败: {}", e))?;
    let text =
        pdf_extract::extract_text_from_mem(&bytes).map_err(|e| format!("PDF 解析失败: {}", e))?;
    Ok(truncate_content(&text))
}

fn extract_image_ocr(path: &Path) -> Result<String, String> {
    let output = Command::new("tesseract")
        .arg(path.as_os_str())
        .arg("stdout")
        .arg("-l")
        .arg("chi_sim+eng")
        .output()
        .map_err(|e| {
            format!(
                "tesseract 未安装或无法执行: {}. 安装: brew install tesseract tesseract-lang",
                e
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "OCR 失败: {}. 确保已安装 tesseract: brew install tesseract tesseract-lang",
            stderr
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if text.trim().is_empty() {
        return Err("OCR 未识别到文字".into());
    }
    Ok(truncate_content(&text))
}

fn extract_text_file(path: &Path) -> Result<String, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("读取文件失败: {}", e))?;
    Ok(truncate_content(&content))
}

fn truncate_content(text: &str) -> String {
    let max_lines = 500;
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() > max_lines {
        format!(
            "{}\n... (已截断，原始 {} 行)",
            lines[..max_lines].join("\n"),
            lines.len()
        )
    } else {
        text.to_string()
    }
}

/// 从输入字符串中解析 @文件路径 引用
///
/// 返回 (清理后的输入, 文件路径列表)。
/// 支持 @相对路径 和 @"路径含空格" 格式。
pub fn parse_at_references(input: &str) -> (String, Vec<String>) {
    let mut cleaned = String::new();
    let mut files = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '@' && i + 1 < len && !chars[i + 1].is_whitespace() {
            let start = i + 1;

            if i + 1 < len && chars[i + 1] == '"' {
                let content_start = i + 2;
                let mut j = content_start;
                while j < len && chars[j] != '"' {
                    j += 1;
                }
                if j < len {
                    let filepath: String = chars[content_start..j].iter().collect();
                    if !filepath.is_empty() {
                        files.push(filepath);
                    }
                    i = j + 1;
                    continue;
                }
            }

            let mut j = start;
            while j < len && !chars[j].is_whitespace() {
                j += 1;
            }
            let filepath: String = chars[start..j].iter().collect();
            if !filepath.is_empty() {
                files.push(filepath);
            }
            i = j;
            continue;
        }

        cleaned.push(chars[i]);
        i += 1;
    }

    (cleaned.trim().to_string(), files)
}

/// 从当前目录扫描匹配前缀的文件列表（@ 自动补全用）
///
/// 支持子目录：`@src/ma` 会在 `src/` 下补全 `src/main.rs`。
/// 返回最多 20 个匹配项，前缀匹配优先，模糊包含其次。
/// 忽略隐藏文件、target 与 node_modules，并拒绝越界路径。
pub fn scan_files_for_autocomplete(prefix: &str) -> Vec<String> {
    scan_files_in(&cwd(), prefix)
}

fn scan_files_in(base: &Path, prefix: &str) -> Vec<String> {
    if prefix.starts_with('/') || prefix.split('/').any(|part| part == "..") {
        return Vec::new();
    }

    let (parent, name_prefix) = match prefix.rsplit_once('/') {
        Some((parent, name)) => (parent, name),
        None => ("", prefix),
    };
    let directory = if parent.is_empty() {
        base.to_path_buf()
    } else {
        base.join(parent)
    };

    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };

    let lower = name_prefix.to_lowercase();
    let prefix_lower = prefix.to_lowercase();
    let mut matches = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if !name.to_lowercase().contains(&lower) {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let mut display = if parent.is_empty() {
            name
        } else {
            format!("{}/{}", parent, name)
        };
        if is_dir {
            display.push('/');
        }
        matches.push(display);
    }

    matches.sort_by_key(|candidate| {
        let lower_candidate = candidate.to_lowercase();
        if lower_candidate.starts_with(&prefix_lower) {
            (0u8, lower_candidate)
        } else {
            (1u8, lower_candidate)
        }
    });
    matches.truncate(20);
    matches
}

/// 检测输入中光标位置前的 @ 前缀（Tab 自动补全用）
///
/// 返回 (前缀文本, 是否在引号内)。
pub fn detect_at_cursor(input: &str) -> Option<(String, bool)> {
    if let Some(pos) = input.rfind('@') {
        let after = &input[pos + 1..];
        if after.starts_with('"') {
            let inner = after.trim_start_matches('"');
            let end = inner.find('"').unwrap_or(inner.len());
            return Some((inner[..end].to_string(), true));
        }
        let end = after
            .find(|c: char| c.is_whitespace())
            .unwrap_or(after.len());
        let prefix = &after[..end];
        return Some((prefix.to_string(), false));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_at_basic() {
        let (cleaned, files) = parse_at_references("hello @file.txt world");
        assert_eq!(cleaned, "hello  world");
        assert_eq!(files, vec!["file.txt"]);
    }

    #[test]
    fn parse_at_quoted() {
        let (cleaned, files) = parse_at_references("check @\"my file.pdf\" please");
        assert_eq!(cleaned, "check  please");
        assert_eq!(files, vec!["my file.pdf"]);
    }

    #[test]
    fn parse_multiple_at() {
        let (cleaned, files) =
            parse_at_references("@src/main.rs and @docs/readme.md and @\"my doc.pdf\"");
        assert_eq!(cleaned, "and  and");
        assert_eq!(files, vec!["src/main.rs", "docs/readme.md", "my doc.pdf"]);
    }

    #[test]
    fn parse_no_at() {
        let (cleaned, files) = parse_at_references("hello world");
        assert_eq!(cleaned, "hello world");
        assert!(files.is_empty());
    }

    #[test]
    fn parse_at_only() {
        let (cleaned, files) = parse_at_references("@file.txt");
        assert_eq!(cleaned, "");
        assert_eq!(files, vec!["file.txt"]);
    }

    #[test]
    fn extension_detection() {
        assert!(is_image_ext("png"));
        assert!(is_image_ext("jpg"));
        assert!(!is_image_ext("pdf"));
        assert!(is_text_ext("rs"));
        assert!(is_text_ext("py"));
        assert!(is_text_ext("txt"));
    }

    #[test]
    fn truncate_short_content() {
        let text = "hello\nworld";
        assert_eq!(truncate_content(text), "hello\nworld");
    }
}

#[cfg(test)]
mod autocomplete_tests {
    use super::scan_files_in;
    use std::path::PathBuf;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nezha-autocomplete-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("src/nested")).unwrap();
        std::fs::write(dir.join("README.md"), "readme").unwrap();
        std::fs::write(dir.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.join("src/lib.rs"), "").unwrap();
        dir
    }

    #[test]
    fn completes_top_level_files() {
        let dir = temp_dir();
        let matches = scan_files_in(&dir, "RE");
        assert!(matches.contains(&"README.md".to_string()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn completes_nested_directory() {
        let dir = temp_dir();
        let matches = scan_files_in(&dir, "src/");
        assert!(matches.contains(&"src/main.rs".to_string()));
        assert!(matches.contains(&"src/lib.rs".to_string()));
        assert!(matches.contains(&"src/nested/".to_string()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn filters_nested_prefix() {
        let dir = temp_dir();
        let matches = scan_files_in(&dir, "src/ma");
        assert_eq!(matches, vec!["src/main.rs".to_string()]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_parent_traversal() {
        let dir = temp_dir();
        assert!(scan_files_in(&dir, "../etc").is_empty());
        assert!(scan_files_in(&dir, "/etc").is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
