//! 文本排版工具 —— 终端显示宽度、宽字符安全截断与智能换行。
//!
//! 终端里一个汉字通常占两列、一个 ASCII 字符占一列，直接用 `len()` 计算
//! 会导致中文界面错位。本模块统一用 `unicode-width` 计算显示宽度，并提供：
//!
//! - [`display_width`]：字符串在终端中的显示宽度
//! - [`truncate_width`]：按显示宽度截断并追加省略号
//! - [`wrap_text`]：按显示宽度换行（优先在空格处断行，兼容中日韩宽字符）

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// 返回字符串在终端中占用的列数。
pub fn display_width(text: &str) -> usize {
    text.width()
}

/// 按显示宽度截断字符串，超出部分用 `…` 表示。
///
/// `max_width` 是包含省略号的最终宽度。
pub fn truncate_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width == 1 {
        return "…".to_string();
    }

    let mut out = String::new();
    let mut width = 0;
    for ch in text.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if width + ch_width > max_width - 1 {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out.push('…');
    out
}

/// 按显示宽度换行，每行前可添加固定前缀（用于缩进）。
///
/// 规则：
/// - 保留原有换行符；
/// - 英文优先在最近的空格处断行，避免把单词拆开；
/// - 中文/日文/韩文等宽字符可以在任意字符处断行；
/// - 超长单词会被硬切分，保证不超过 `max_width`。
pub fn wrap_text(text: &str, max_width: usize, prefix: &str) -> Vec<String> {
    if max_width == 0 {
        return vec![format!("{}{}", prefix, text)];
    }

    let prefix_width = prefix.width();
    let content_width = max_width.saturating_sub(prefix_width).max(1);

    if text.is_empty() {
        return vec![prefix.to_string()];
    }

    let mut result = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            result.push(prefix.to_string());
            continue;
        }
        for line in wrap_line(paragraph, content_width) {
            result.push(format!("{}{}", prefix, line));
        }
    }
    result
}

fn wrap_line(line: &str, width: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    // 当前行最后一个空格所在的字节位置（断行点）
    let mut last_space: Option<usize> = None;

    for ch in line.chars() {
        let ch_width = ch.width().unwrap_or(0);

        if current_width + ch_width > width && current_width > 0 {
            if let Some(break_at) = last_space.take() {
                let head = current[..break_at].trim_end().to_string();
                let tail = current[break_at + 1..].trim_start().to_string();
                if !head.is_empty() {
                    result.push(head);
                }
                current = tail;
                current_width = current.width();
            }

            if current_width + ch_width > width && current_width > 0 {
                result.push(std::mem::take(&mut current));
                current_width = 0;
            }
        }

        current.push(ch);
        current_width += ch_width;
        if ch == ' ' {
            last_space = Some(current.len() - 1);
        }
    }

    if !current.is_empty() || result.is_empty() {
        result.push(current);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_width_counts_wide_chars() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("中文"), 4);
        assert_eq!(display_width("a中"), 3);
    }

    #[test]
    fn truncate_short_text_unchanged() {
        assert_eq!(truncate_width("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_text_adds_ellipsis() {
        let truncated = truncate_width("hello world", 8);
        assert_eq!(truncated.width(), 8);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn truncate_zero_width_is_empty() {
        assert_eq!(truncate_width("abc", 0), "");
    }

    #[test]
    fn truncate_cjk_respects_width() {
        let truncated = truncate_width("这是一个很长的中文标题", 8);
        assert!(truncated.width() <= 8);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn wrap_empty_string_returns_one_line() {
        assert_eq!(wrap_text("", 80, ""), vec![""]);
    }

    #[test]
    fn wrap_short_line_no_wrap() {
        assert_eq!(wrap_text("hello", 80, ""), vec!["hello"]);
    }

    #[test]
    fn wrap_long_line_wraps() {
        let text = "a".repeat(100);
        let result = wrap_text(&text, 40, "");
        assert!(result.len() > 1);
        for line in &result {
            assert!(line.len() <= 40);
        }
    }

    #[test]
    fn wrap_prefers_word_boundaries() {
        let result = wrap_text("hello world again", 12, "");
        assert_eq!(result, vec!["hello world", "again"]);
    }

    #[test]
    fn wrap_with_prefix_indents() {
        assert_eq!(wrap_text("test", 40, "  "), vec!["  test"]);
    }

    #[test]
    fn wrap_multiline_input() {
        let result = wrap_text("line1\nline2\nline3", 40, "");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn wrap_keeps_blank_lines() {
        let result = wrap_text("a\n\nb", 40, "");
        assert_eq!(result, vec!["a", "", "b"]);
    }

    #[test]
    fn wrap_cjk_by_width() {
        let result = wrap_text("这是一个用于测试换行的中文句子", 10, "");
        assert!(result.len() > 1);
        for line in &result {
            assert!(line.width() <= 10, "line too wide: {}", line);
        }
    }
}
