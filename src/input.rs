//! 输入框编辑逻辑 —— 光标移动、插入、删除、历史记录与视觉行布局。
//!
//! 所有函数都以 **字节索引** 表示光标位置，并保证落在 UTF-8 字符边界上，
//! 因此可以安全处理中文、Emoji 等宽字符。渲染层再通过 [`visual_layout`]
//! 把逻辑文本换算成终端行与列。

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// 把任意字节位置收敛到合法的字符边界。
pub fn clamp_cursor(text: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(text.len());
    while cursor > 0 && !text.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}

/// 在光标处插入文本，并把光标移动到插入内容之后。
pub fn insert_str(text: &mut String, cursor: &mut usize, value: &str) {
    let at = clamp_cursor(text, *cursor);
    text.insert_str(at, value);
    *cursor = at + value.len();
}

/// 在光标处插入单个字符。
pub fn insert_char(text: &mut String, cursor: &mut usize, ch: char) {
    let at = clamp_cursor(text, *cursor);
    text.insert(at, ch);
    *cursor = at + ch.len_utf8();
}

/// 插入换行符。
pub fn insert_newline(text: &mut String, cursor: &mut usize) {
    insert_char(text, cursor, '\n');
}

/// 删除光标前的一个字符，返回是否发生删除。
pub fn backspace(text: &mut String, cursor: &mut usize) -> bool {
    let at = clamp_cursor(text, *cursor);
    if at == 0 {
        return false;
    }
    let prev = prev_boundary(text, at);
    text.replace_range(prev..at, "");
    *cursor = prev;
    true
}

/// 删除光标后的一个字符，返回是否发生删除。
pub fn delete_forward(text: &mut String, cursor: &mut usize) -> bool {
    let at = clamp_cursor(text, *cursor);
    if at >= text.len() {
        return false;
    }
    let next = next_boundary(text, at);
    text.replace_range(at..next, "");
    true
}

/// 删除光标前的一个单词（或一段中文）。
pub fn delete_word_backward(text: &mut String, cursor: &mut usize) {
    let at = clamp_cursor(text, *cursor);
    if at == 0 {
        return;
    }
    let start = word_start(text, at);
    text.replace_range(start..at, "");
    *cursor = start;
}

/// 光标左移一个字符。
pub fn move_left(text: &str, cursor: &mut usize) {
    let at = clamp_cursor(text, *cursor);
    if at > 0 {
        *cursor = prev_boundary(text, at);
    }
}

/// 光标右移一个字符。
pub fn move_right(text: &str, cursor: &mut usize) {
    let at = clamp_cursor(text, *cursor);
    if at < text.len() {
        *cursor = next_boundary(text, at);
    }
}

/// 光标左移一个单词。
pub fn move_word_left(text: &str, cursor: &mut usize) {
    let at = clamp_cursor(text, *cursor);
    *cursor = word_start(text, at);
}

/// 光标右移一个单词。
pub fn move_word_right(text: &str, cursor: &mut usize) {
    let at = clamp_cursor(text, *cursor);
    let mut iter = text[at..].char_indices().peekable();

    // 先跳过当前单词剩余字符
    while let Some((_, ch)) = iter.peek() {
        if ch.is_whitespace() {
            break;
        }
        iter.next();
    }
    // 再跳过紧随其后的空白，落在下一个单词开头
    while let Some((_, ch)) = iter.peek() {
        if !ch.is_whitespace() {
            break;
        }
        iter.next();
    }

    *cursor = iter.peek().map(|(idx, _)| at + idx).unwrap_or(text.len());
}

/// 移动到当前逻辑行行首。
pub fn line_start(text: &str, cursor: &mut usize) {
    let at = clamp_cursor(text, *cursor);
    *cursor = text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
}

/// 移动到当前逻辑行行尾。
pub fn line_end(text: &str, cursor: &mut usize) {
    let at = clamp_cursor(text, *cursor);
    *cursor = text[at..].find('\n').map(|i| at + i).unwrap_or(text.len());
}

/// 光标上下移动一个逻辑行，尽量保持列位置。
pub fn move_up_line(text: &str, cursor: &mut usize) {
    let at = clamp_cursor(text, *cursor);
    let start = text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
    if start == 0 {
        return;
    }
    let prev_end = start - 1;
    let prev_start = text[..prev_end].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let column = text[start..at].width();
    *cursor = prev_start + byte_at_column(&text[prev_start..prev_end], column);
}

/// 光标下移一个逻辑行，尽量保持列位置。
pub fn move_down_line(text: &str, cursor: &mut usize) {
    let at = clamp_cursor(text, *cursor);
    let end = text[at..].find('\n').map(|i| at + i).unwrap_or(text.len());
    if end >= text.len() {
        return;
    }
    let start = text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let column = text[start..at].width();
    let next_start = end + 1;
    let next_end = text[next_start..]
        .find('\n')
        .map(|i| next_start + i)
        .unwrap_or(text.len());
    *cursor = next_start + byte_at_column(&text[next_start..next_end], column);
}

/// 当前输入中处于光标下的 `@` 引用查询。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtQuery {
    /// `@` 的字节位置
    pub token_start: usize,
    /// `@` 之后已经输入的路径前缀
    pub prefix: String,
    /// 是否使用了 `@"..."` 引号形式
    pub quoted: bool,
}

/// 检测光标前是否正在编辑一个 `@文件` 引用。
///
/// 只识别光标所在的 token，不会把已经输入完的引用误判成新的引用。
pub fn active_at_query(text: &str, cursor: usize) -> Option<AtQuery> {
    let at = clamp_cursor(text, cursor);
    let before = &text[..at];

    // 先处理带引号的路径（路径中可能包含空格）。
    if let Some(start) = before.rfind("@\"") {
        let is_boundary = start == 0
            || before[..start]
                .chars()
                .next_back()
                .map(|ch| ch.is_whitespace())
                .unwrap_or(true);
        if is_boundary {
            let rest = &before[start + 2..];
            if !rest.contains('"') {
                return Some(AtQuery {
                    token_start: start,
                    prefix: rest.to_string(),
                    quoted: true,
                });
            }
        }
    }

    let token_start = before
        .rfind(|c: char| c.is_whitespace())
        .map(|i| i + 1)
        .unwrap_or(0);
    let token = &before[token_start..];
    let rest = token.strip_prefix('@')?;

    Some(AtQuery {
        token_start,
        prefix: rest.to_string(),
        quoted: false,
    })
}

/// 用补全项替换光标下的 `@` token。
pub fn apply_at_completion(text: &mut String, cursor: &mut usize, query: &AtQuery, item: &str) {
    let needs_quotes = query.quoted || item.contains(' ');
    let replacement = if needs_quotes {
        format!("@\"{}\"", item)
    } else {
        format!("@{}", item)
    };
    text.replace_range(query.token_start..*cursor, &replacement);
    *cursor = query.token_start + replacement.len();
}

/// 一组候选项的最长公共前缀。
pub fn common_prefix(items: &[String]) -> String {
    let Some(first) = items.first() else {
        return String::new();
    };
    let mut prefix = first.clone();
    for item in &items[1..] {
        while !item.starts_with(&prefix) {
            if prefix.pop().is_none() {
                return String::new();
            }
        }
    }
    prefix
}

/// 输入框的视觉布局结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualLayout {
    /// 每个视觉行的字节范围（不含换行符）
    pub rows: Vec<(usize, usize)>,
    /// 光标所在的视觉行
    pub cursor_row: usize,
    /// 光标在视觉行中的显示列
    pub cursor_column: usize,
}

/// 按显示宽度计算输入文本的视觉行，并定位光标。
pub fn visual_layout(text: &str, cursor: usize, width: usize) -> VisualLayout {
    let width = width.max(1);
    let mut rows: Vec<(usize, usize)> = Vec::new();
    let mut row_start = 0usize;
    let mut row_width = 0usize;

    for (idx, ch) in text.char_indices() {
        if ch == '\n' {
            rows.push((row_start, idx));
            row_start = idx + 1;
            row_width = 0;
            continue;
        }
        let ch_width = ch.width().unwrap_or(0);
        if row_width + ch_width > width && row_width > 0 {
            rows.push((row_start, idx));
            row_start = idx;
            row_width = 0;
        }
        row_width += ch_width;
    }
    rows.push((row_start, text.len()));

    let cursor = clamp_cursor(text, cursor);
    let mut cursor_row = rows.len().saturating_sub(1);
    let mut cursor_column = 0;

    // 当光标恰好落在自动折行点时，优先显示在上一行末尾，
    // 这样光标不会在用户未换行时跳到下一行。
    for (row, (start, end)) in rows.iter().enumerate() {
        if cursor >= *start && cursor <= *end {
            cursor_row = row;
            cursor_column = text[*start..cursor].width().min(width.saturating_sub(1));
            break;
        }
    }

    VisualLayout {
        rows,
        cursor_row,
        cursor_column,
    }
}

fn prev_boundary(text: &str, at: usize) -> usize {
    text[..at]
        .char_indices()
        .next_back()
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn next_boundary(text: &str, at: usize) -> usize {
    text[at..]
        .char_indices()
        .nth(1)
        .map(|(idx, _)| at + idx)
        .unwrap_or(text.len())
}

fn word_start(text: &str, at: usize) -> usize {
    let mut idx = at;

    // 跳过光标前的空白
    while idx > 0 {
        let ch = text[..idx].chars().next_back().unwrap();
        if ch.is_whitespace() {
            idx -= ch.len_utf8();
        } else {
            break;
        }
    }

    // 找到该单词（或中文片段）之前的边界
    text[..idx]
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(i, ch)| i + ch.len_utf8())
        .unwrap_or(0)
}

fn byte_at_column(text: &str, target: usize) -> usize {
    let mut width = 0usize;
    for (idx, ch) in text.char_indices() {
        if width >= target {
            return idx;
        }
        width += ch.width().unwrap_or(0);
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_char_moves_cursor() {
        let mut text = String::new();
        let mut cursor = 0;
        insert_char(&mut text, &mut cursor, 'a');
        insert_char(&mut text, &mut cursor, '中');
        assert_eq!(text, "a中");
        assert_eq!(cursor, text.len());
    }

    #[test]
    fn insert_in_middle_handles_unicode() {
        let mut text = "ab".to_string();
        let mut cursor = 1;
        insert_char(&mut text, &mut cursor, '中');
        assert_eq!(text, "a中b");
        assert_eq!(cursor, 4);
    }

    #[test]
    fn backspace_removes_previous_char() {
        let mut text = "a中".to_string();
        let mut cursor = text.len();
        assert!(backspace(&mut text, &mut cursor));
        assert_eq!(text, "a");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn backspace_at_start_does_nothing() {
        let mut text = "abc".to_string();
        let mut cursor = 0;
        assert!(!backspace(&mut text, &mut cursor));
        assert_eq!(text, "abc");
    }

    #[test]
    fn delete_forward_removes_next_char() {
        let mut text = "a中b".to_string();
        let mut cursor = 1;
        assert!(delete_forward(&mut text, &mut cursor));
        assert_eq!(text, "ab");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn delete_word_backward_removes_word() {
        let mut text = "hello world".to_string();
        let mut cursor = text.len();
        delete_word_backward(&mut text, &mut cursor);
        assert_eq!(text, "hello ");
        assert_eq!(cursor, 6);
    }

    #[test]
    fn move_left_and_right_respects_unicode() {
        let text = "a中b";
        let mut cursor = text.len();
        move_left(text, &mut cursor);
        assert_eq!(cursor, 4);
        move_left(text, &mut cursor);
        assert_eq!(cursor, 1);
        move_right(text, &mut cursor);
        assert_eq!(cursor, 4);
    }

    #[test]
    fn word_movement_skips_words() {
        let text = "hello brave world";
        let mut cursor = text.len();
        move_word_left(text, &mut cursor);
        assert_eq!(&text[cursor..], "world");
        move_word_left(text, &mut cursor);
        assert_eq!(&text[cursor..], "brave world");
    }

    #[test]
    fn home_end_follow_logical_lines() {
        let text = "line1\nline2";
        let mut cursor = 8;
        line_start(text, &mut cursor);
        assert_eq!(cursor, 6);
        line_end(text, &mut cursor);
        assert_eq!(cursor, text.len());
    }

    #[test]
    fn vertical_movement_keeps_column() {
        let text = "abc\nxy\n12345";
        let mut cursor = 2;
        move_down_line(text, &mut cursor);
        assert_eq!(cursor, 6);
        move_down_line(text, &mut cursor);
        assert_eq!(cursor, 9);
        move_up_line(text, &mut cursor);
        assert_eq!(cursor, 6);
    }

    #[test]
    fn at_query_detects_token_under_cursor() {
        let text = "check @src/main.rs now";
        let cursor = text.find(" now").unwrap();
        let query = active_at_query(text, cursor).unwrap();
        assert_eq!(query.prefix, "src/main.rs");
        assert!(!query.quoted);
    }

    #[test]
    fn at_query_ignores_completed_reference() {
        let text = "@README.md explain";
        let cursor = text.len();
        assert!(active_at_query(text, cursor).is_none());
    }

    #[test]
    fn at_query_supports_quotes() {
        let text = "@\"my file";
        let cursor = text.len();
        let query = active_at_query(text, cursor).unwrap();
        assert_eq!(query.prefix, "my file");
        assert!(query.quoted);
    }

    #[test]
    fn at_completion_quotes_paths_with_spaces() {
        let mut text = "see @my fi".to_string();
        let mut cursor = text.len();
        let query = AtQuery {
            token_start: 4,
            prefix: "my fi".into(),
            quoted: false,
        };
        apply_at_completion(&mut text, &mut cursor, &query, "my file.pdf");
        assert_eq!(text, "see @\"my file.pdf\"");
        assert_eq!(cursor, text.len());
    }

    #[test]
    fn common_prefix_of_items() {
        let items = vec!["README.md".into(), "README.txt".into()];
        assert_eq!(common_prefix(&items), "README.");
    }

    #[test]
    fn visual_layout_wraps_and_tracks_cursor() {
        let text = "abcdef";
        let layout = visual_layout(text, 6, 4);
        assert_eq!(layout.rows.len(), 2);
        assert_eq!(layout.cursor_row, 1);
        assert_eq!(layout.cursor_column, 2);
    }

    #[test]
    fn visual_layout_counts_wide_chars() {
        let text = "中文输入";
        let layout = visual_layout(text, text.len(), 4);
        assert_eq!(layout.rows.len(), 2);
        assert_eq!(layout.cursor_row, 1);
        assert_eq!(layout.cursor_column, 3);
    }

    #[test]
    fn visual_layout_handles_newlines() {
        let text = "a\nb";
        let layout = visual_layout(text, 2, 10);
        assert_eq!(layout.rows.len(), 2);
        assert_eq!(layout.cursor_row, 1);
        assert_eq!(layout.cursor_column, 0);
    }
}
