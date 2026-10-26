//! 斜杠命令注册表 —— 命令面板、内联提示与执行逻辑共用。
//!
//! 把命令元数据集中在一个地方，渲染层只负责展示，App 层只负责执行，
//! 避免出现「面板里列出的命令实际不可用」的割裂体验。

/// 一条可执行命令的静态描述。
#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    /// 命令名，包含前导 `/`
    pub name: &'static str,
    /// 面板中的图标
    pub icon: &'static str,
    /// 参数提示（无参数时为空）
    pub args: &'static str,
    /// 一句话说明
    pub desc: &'static str,
    /// 从命令面板选择时是否先补全参数再执行
    pub fill_args: bool,
}

impl CommandSpec {
    /// 面板中显示的完整命令（含参数提示）。
    pub fn usage(&self) -> String {
        if self.args.is_empty() {
            self.name.to_string()
        } else {
            format!("{} {}", self.name, self.args)
        }
    }
}

/// 全部内置命令。
pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "/new",
        icon: "＋",
        args: "",
        desc: "新建会话标签页",
        fill_args: false,
    },
    CommandSpec {
        name: "/close",
        icon: "✕",
        args: "",
        desc: "关闭当前会话",
        fill_args: false,
    },
    CommandSpec {
        name: "/clear",
        icon: "⌫",
        args: "",
        desc: "清空当前对话与用量",
        fill_args: false,
    },
    CommandSpec {
        name: "/theme",
        icon: "◐",
        args: "[主题]",
        desc: "切换主题，无参数时循环切换",
        fill_args: true,
    },
    CommandSpec {
        name: "/model",
        icon: "◆",
        args: "[模型]",
        desc: "切换模型，无参数时列出可用模型",
        fill_args: true,
    },
    CommandSpec {
        name: "/agent",
        icon: "◉",
        args: "[名称]",
        desc: "切换 Agent，无参数时列出全部 Agent",
        fill_args: true,
    },
    CommandSpec {
        name: "/save",
        icon: "▣",
        args: "",
        desc: "保存当前对话到记忆库",
        fill_args: false,
    },
    CommandSpec {
        name: "/history",
        icon: "☰",
        args: "",
        desc: "列出已保存的对话",
        fill_args: false,
    },
    CommandSpec {
        name: "/load",
        icon: "↧",
        args: "<ID>",
        desc: "从记忆库加载对话",
        fill_args: true,
    },
    CommandSpec {
        name: "/fav",
        icon: "★",
        args: "",
        desc: "收藏或取消收藏当前对话",
        fill_args: false,
    },
    CommandSpec {
        name: "/export",
        icon: "⇪",
        args: "[路径]",
        desc: "导出为 Markdown 文件",
        fill_args: false,
    },
    CommandSpec {
        name: "/help",
        icon: "?",
        args: "",
        desc: "查看快捷键与命令帮助",
        fill_args: false,
    },
    CommandSpec {
        name: "/quit",
        icon: "⏻",
        args: "",
        desc: "退出哪吒网络安全",
        fill_args: false,
    },
];

/// 按关键词过滤命令，空关键词返回全部。
///
/// 排序优先级：命令名前缀匹配 > 命令名包含 > 说明包含。
pub fn filter(query: &str) -> Vec<&'static CommandSpec> {
    let query = query.trim().trim_start_matches('/').to_lowercase();
    if query.is_empty() {
        return COMMANDS.iter().collect();
    }

    let mut matched: Vec<(u8, &'static CommandSpec)> = COMMANDS
        .iter()
        .filter_map(|spec| {
            let name = spec.name.trim_start_matches('/').to_lowercase();
            let rank = if name.starts_with(&query) {
                0
            } else if name.contains(&query) {
                1
            } else if spec.desc.to_lowercase().contains(&query)
                || spec.args.to_lowercase().contains(&query)
            {
                2
            } else {
                return None;
            };
            Some((rank, spec))
        })
        .collect();

    matched.sort_by_key(|(rank, spec)| (*rank, spec.name));
    matched.into_iter().map(|(_, spec)| spec).collect()
}

/// 按完整命令名查找（忽略参数与大小写）。
pub fn find(name: &str) -> Option<&'static CommandSpec> {
    let name = name.split_whitespace().next().unwrap_or(name);
    COMMANDS
        .iter()
        .find(|spec| spec.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_empty_returns_all_commands() {
        assert_eq!(filter("").len(), COMMANDS.len());
    }

    #[test]
    fn filter_by_prefix_ranks_first() {
        let result = filter("cl");
        assert_eq!(result.first().map(|c| c.name), Some("/clear"));
    }

    #[test]
    fn filter_accepts_leading_slash() {
        assert_eq!(filter("/theme").first().map(|c| c.name), Some("/theme"));
    }

    #[test]
    fn filter_matches_description() {
        let result = filter("收藏");
        assert!(result.iter().any(|c| c.name == "/fav"));
    }

    #[test]
    fn find_ignores_arguments() {
        assert_eq!(find("/theme dracula").map(|c| c.name), Some("/theme"));
    }

    #[test]
    fn find_unknown_returns_none() {
        assert!(find("/nope").is_none());
    }

    #[test]
    fn usage_includes_arguments_when_present() {
        let spec = find("/theme").unwrap();
        assert_eq!(spec.usage(), "/theme [主题]");
        assert_eq!(find("/new").unwrap().usage(), "/new");
    }

    #[test]
    fn every_command_name_is_unique() {
        let mut names: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), COMMANDS.len());
    }
}
