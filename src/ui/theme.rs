//! 主题系统 —— 定义终端颜色方案。
//!
//! 支持多种内置主题，可通过斜杠命令 /theme 动态切换。
//! 包含 11 种精心设计的配色方案，涵盖暗色与亮色。

use ratatui::style::Color;

/// 颜色主题定义
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub accent_dim: Color,
    pub sidebar_bg: Color,
    pub sidebar_fg: Color,
    pub input_bg: Color,
    pub input_border: Color,
    pub user_msg_color: Color,
    pub assistant_msg_color: Color,
    pub error_color: Color,
    pub success_color: Color,
    pub warning_color: Color,
    pub tab_active_bg: Color,
    pub tab_inactive_bg: Color,
    pub border_color: Color,
}

/// 默认暗色主题 —— "cyber-dark"
/// 深蓝黑底 + 青色强调，赛博朋克风格
pub const THEME_CYBER_DARK: Theme = Theme {
    name: String::new(),
    bg: Color::Rgb(18, 18, 24),
    fg: Color::Rgb(220, 220, 230),
    accent: Color::Rgb(0, 212, 255),
    accent_dim: Color::Rgb(0, 150, 180),
    sidebar_bg: Color::Rgb(14, 14, 20),
    sidebar_fg: Color::Rgb(180, 180, 200),
    input_bg: Color::Rgb(22, 22, 30),
    input_border: Color::Rgb(60, 60, 80),
    user_msg_color: Color::Rgb(100, 200, 255),
    assistant_msg_color: Color::Rgb(200, 220, 240),
    error_color: Color::Rgb(255, 80, 80),
    success_color: Color::Rgb(80, 255, 120),
    warning_color: Color::Rgb(255, 200, 60),
    tab_active_bg: Color::Rgb(30, 30, 42),
    tab_inactive_bg: Color::Rgb(18, 18, 28),
    border_color: Color::Rgb(50, 50, 70),
};

/// 亮色主题 —— "daylight"
/// 浅灰白底 + 蓝色强调，适合白天使用
pub const THEME_DAYLIGHT: Theme = Theme {
    name: String::new(),
    bg: Color::Rgb(245, 245, 250),
    fg: Color::Rgb(30, 30, 40),
    accent: Color::Rgb(0, 120, 210),
    accent_dim: Color::Rgb(0, 90, 160),
    sidebar_bg: Color::Rgb(235, 235, 245),
    sidebar_fg: Color::Rgb(60, 60, 80),
    input_bg: Color::Rgb(240, 240, 248),
    input_border: Color::Rgb(180, 180, 200),
    user_msg_color: Color::Rgb(0, 100, 200),
    assistant_msg_color: Color::Rgb(40, 40, 60),
    error_color: Color::Rgb(200, 30, 30),
    success_color: Color::Rgb(20, 160, 60),
    warning_color: Color::Rgb(200, 150, 20),
    tab_active_bg: Color::Rgb(225, 225, 240),
    tab_inactive_bg: Color::Rgb(245, 245, 250),
    border_color: Color::Rgb(200, 200, 215),
};

/// Dracula 主题 —— "dracula"
/// 经典紫色调暗色主题，Zeno Rocha 设计
pub const THEME_DRACULA: Theme = Theme {
    name: String::new(),
    bg: Color::Rgb(40, 42, 54),
    fg: Color::Rgb(248, 248, 242),
    accent: Color::Rgb(189, 147, 249),
    accent_dim: Color::Rgb(139, 92, 246),
    sidebar_bg: Color::Rgb(30, 32, 44),
    sidebar_fg: Color::Rgb(208, 208, 202),
    input_bg: Color::Rgb(54, 56, 70),
    input_border: Color::Rgb(76, 78, 100),
    user_msg_color: Color::Rgb(139, 233, 253),
    assistant_msg_color: Color::Rgb(248, 248, 242),
    error_color: Color::Rgb(255, 85, 85),
    success_color: Color::Rgb(80, 250, 123),
    warning_color: Color::Rgb(255, 184, 108),
    tab_active_bg: Color::Rgb(54, 56, 70),
    tab_inactive_bg: Color::Rgb(40, 42, 54),
    border_color: Color::Rgb(76, 78, 100),
};

/// Gruvbox 主题 —— "gruvbox"
/// 复古暖色调暗色主题，morhetz 设计
pub const THEME_GRUVBOX: Theme = Theme {
    name: String::new(),
    bg: Color::Rgb(40, 40, 40),
    fg: Color::Rgb(235, 219, 178),
    accent: Color::Rgb(184, 187, 38),
    accent_dim: Color::Rgb(142, 144, 26),
    sidebar_bg: Color::Rgb(30, 30, 30),
    sidebar_fg: Color::Rgb(189, 174, 147),
    input_bg: Color::Rgb(50, 50, 50),
    input_border: Color::Rgb(80, 80, 80),
    user_msg_color: Color::Rgb(131, 165, 152),
    assistant_msg_color: Color::Rgb(235, 219, 178),
    error_color: Color::Rgb(251, 73, 52),
    success_color: Color::Rgb(142, 192, 124),
    warning_color: Color::Rgb(250, 189, 47),
    tab_active_bg: Color::Rgb(60, 60, 60),
    tab_inactive_bg: Color::Rgb(40, 40, 40),
    border_color: Color::Rgb(80, 80, 80),
};

/// Solarized Dark 主题 —— "solarized-dark"
/// Ethan Schoonover 科学配色暗色主题
pub const THEME_SOLARIZED_DARK: Theme = Theme {
    name: String::new(),
    bg: Color::Rgb(0, 43, 54),
    fg: Color::Rgb(131, 148, 150),
    accent: Color::Rgb(181, 137, 0),
    accent_dim: Color::Rgb(143, 107, 0),
    sidebar_bg: Color::Rgb(7, 54, 66),
    sidebar_fg: Color::Rgb(101, 123, 131),
    input_bg: Color::Rgb(22, 55, 66),
    input_border: Color::Rgb(42, 65, 73),
    user_msg_color: Color::Rgb(38, 139, 210),
    assistant_msg_color: Color::Rgb(131, 148, 150),
    error_color: Color::Rgb(220, 50, 47),
    success_color: Color::Rgb(133, 153, 0),
    warning_color: Color::Rgb(181, 137, 0),
    tab_active_bg: Color::Rgb(22, 55, 66),
    tab_inactive_bg: Color::Rgb(0, 43, 54),
    border_color: Color::Rgb(42, 65, 73),
};

/// Nord 主题 —— "nord"
/// 北极蓝调暗色主题，Arctic Ice Studio 设计
pub const THEME_NORD: Theme = Theme {
    name: String::new(),
    bg: Color::Rgb(46, 52, 64),
    fg: Color::Rgb(216, 222, 233),
    accent: Color::Rgb(136, 192, 208),
    accent_dim: Color::Rgb(94, 129, 172),
    sidebar_bg: Color::Rgb(36, 42, 54),
    sidebar_fg: Color::Rgb(196, 202, 213),
    input_bg: Color::Rgb(56, 62, 74),
    input_border: Color::Rgb(76, 82, 94),
    user_msg_color: Color::Rgb(143, 188, 187),
    assistant_msg_color: Color::Rgb(216, 222, 233),
    error_color: Color::Rgb(191, 97, 106),
    success_color: Color::Rgb(163, 190, 140),
    warning_color: Color::Rgb(235, 203, 139),
    tab_active_bg: Color::Rgb(56, 62, 74),
    tab_inactive_bg: Color::Rgb(46, 52, 64),
    border_color: Color::Rgb(76, 82, 94),
};

/// Monokai 主题 —— "monokai"
/// 经典 Sublime Text 代码编辑器暗色主题
pub const THEME_MONOKAI: Theme = Theme {
    name: String::new(),
    bg: Color::Rgb(39, 40, 34),
    fg: Color::Rgb(248, 248, 242),
    accent: Color::Rgb(166, 226, 46),
    accent_dim: Color::Rgb(118, 168, 26),
    sidebar_bg: Color::Rgb(29, 30, 24),
    sidebar_fg: Color::Rgb(208, 208, 202),
    input_bg: Color::Rgb(49, 50, 44),
    input_border: Color::Rgb(69, 70, 64),
    user_msg_color: Color::Rgb(102, 217, 239),
    assistant_msg_color: Color::Rgb(248, 248, 242),
    error_color: Color::Rgb(249, 38, 114),
    success_color: Color::Rgb(166, 226, 46),
    warning_color: Color::Rgb(230, 219, 116),
    tab_active_bg: Color::Rgb(49, 50, 44),
    tab_inactive_bg: Color::Rgb(39, 40, 34),
    border_color: Color::Rgb(69, 70, 64),
};

/// Tokyo Night 主题 —— "tokyo-night"
/// 流行的 Neovim 暗色主题，folke 设计
pub const THEME_TOKYO_NIGHT: Theme = Theme {
    name: String::new(),
    bg: Color::Rgb(26, 27, 38),
    fg: Color::Rgb(169, 177, 214),
    accent: Color::Rgb(122, 162, 247),
    accent_dim: Color::Rgb(61, 89, 161),
    sidebar_bg: Color::Rgb(22, 23, 34),
    sidebar_fg: Color::Rgb(139, 147, 184),
    input_bg: Color::Rgb(32, 33, 44),
    input_border: Color::Rgb(52, 53, 64),
    user_msg_color: Color::Rgb(125, 207, 255),
    assistant_msg_color: Color::Rgb(169, 177, 214),
    error_color: Color::Rgb(247, 118, 142),
    success_color: Color::Rgb(158, 206, 106),
    warning_color: Color::Rgb(224, 175, 104),
    tab_active_bg: Color::Rgb(32, 33, 44),
    tab_inactive_bg: Color::Rgb(26, 27, 38),
    border_color: Color::Rgb(52, 53, 64),
};

/// Catppuccin Mocha 主题 —— "catppuccin"
/// 现代柔和粉彩暗色主题，Catppuccin 社区设计
pub const THEME_CATPPUCCIN: Theme = Theme {
    name: String::new(),
    bg: Color::Rgb(30, 30, 46),
    fg: Color::Rgb(205, 214, 244),
    accent: Color::Rgb(203, 166, 247),
    accent_dim: Color::Rgb(148, 113, 203),
    sidebar_bg: Color::Rgb(24, 24, 37),
    sidebar_fg: Color::Rgb(186, 194, 222),
    input_bg: Color::Rgb(36, 36, 52),
    input_border: Color::Rgb(56, 56, 72),
    user_msg_color: Color::Rgb(137, 220, 235),
    assistant_msg_color: Color::Rgb(205, 214, 244),
    error_color: Color::Rgb(243, 139, 168),
    success_color: Color::Rgb(166, 227, 161),
    warning_color: Color::Rgb(249, 226, 175),
    tab_active_bg: Color::Rgb(36, 36, 52),
    tab_inactive_bg: Color::Rgb(30, 30, 46),
    border_color: Color::Rgb(56, 56, 72),
};

/// One Dark 主题 —— "one-dark"
/// Atom 编辑器经典暗色主题
pub const THEME_ONE_DARK: Theme = Theme {
    name: String::new(),
    bg: Color::Rgb(40, 44, 52),
    fg: Color::Rgb(171, 178, 191),
    accent: Color::Rgb(97, 175, 239),
    accent_dim: Color::Rgb(56, 110, 164),
    sidebar_bg: Color::Rgb(33, 37, 43),
    sidebar_fg: Color::Rgb(131, 138, 151),
    input_bg: Color::Rgb(48, 52, 60),
    input_border: Color::Rgb(68, 72, 80),
    user_msg_color: Color::Rgb(86, 182, 194),
    assistant_msg_color: Color::Rgb(171, 178, 191),
    error_color: Color::Rgb(224, 108, 117),
    success_color: Color::Rgb(152, 195, 121),
    warning_color: Color::Rgb(229, 192, 123),
    tab_active_bg: Color::Rgb(48, 52, 60),
    tab_inactive_bg: Color::Rgb(40, 44, 52),
    border_color: Color::Rgb(68, 72, 80),
};

/// Everforest 主题 —— "everforest"
/// 森林绿调暗色主题，sainnhe 设计
pub const THEME_EVERFOREST: Theme = Theme {
    name: String::new(),
    bg: Color::Rgb(45, 53, 59),
    fg: Color::Rgb(211, 198, 170),
    accent: Color::Rgb(167, 192, 128),
    accent_dim: Color::Rgb(125, 150, 96),
    sidebar_bg: Color::Rgb(35, 43, 49),
    sidebar_fg: Color::Rgb(181, 168, 140),
    input_bg: Color::Rgb(55, 63, 69),
    input_border: Color::Rgb(75, 83, 89),
    user_msg_color: Color::Rgb(127, 187, 179),
    assistant_msg_color: Color::Rgb(211, 198, 170),
    error_color: Color::Rgb(230, 126, 128),
    success_color: Color::Rgb(167, 192, 128),
    warning_color: Color::Rgb(219, 188, 127),
    tab_active_bg: Color::Rgb(55, 63, 69),
    tab_inactive_bg: Color::Rgb(45, 53, 59),
    border_color: Color::Rgb(75, 83, 89),
};

/// 所有可用主题名称列表
pub const ALL_THEME_NAMES: &[&str] = &[
    "default",
    "cyber-dark",
    "daylight",
    "light",
    "dracula",
    "gruvbox",
    "solarized-dark",
    "nord",
    "monokai",
    "tokyo-night",
    "catppuccin",
    "one-dark",
    "everforest",
];

/// 根据名称获取主题
///
/// 支持 11 种内置主题，未知名称回退到 cyber-dark。
/// "default" 和 "cyber-dark" 等价，"light" 是 "daylight" 的别名。
pub fn get_theme(name: &str) -> Theme {
    let mut theme = match name {
        "daylight" | "light" => THEME_DAYLIGHT,
        "dracula" => THEME_DRACULA,
        "gruvbox" => THEME_GRUVBOX,
        "solarized-dark" => THEME_SOLARIZED_DARK,
        "nord" => THEME_NORD,
        "monokai" => THEME_MONOKAI,
        "tokyo-night" => THEME_TOKYO_NIGHT,
        "catppuccin" => THEME_CATPPUCCIN,
        "one-dark" => THEME_ONE_DARK,
        "everforest" => THEME_EVERFOREST,
        _ => THEME_CYBER_DARK,
    };
    theme.name = name.to_string();
    theme
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_theme_cyber_dark_default() {
        let theme = get_theme("default");
        assert_eq!(theme.bg, Color::Rgb(18, 18, 24));
        assert_eq!(theme.accent, Color::Rgb(0, 212, 255));
    }

    #[test]
    fn get_theme_daylight_by_name() {
        let theme = get_theme("daylight");
        assert_eq!(theme.bg, Color::Rgb(245, 245, 250));
        assert_eq!(theme.fg, Color::Rgb(30, 30, 40));
    }

    #[test]
    fn get_theme_light_alias_returns_daylight() {
        let theme = get_theme("light");
        assert_eq!(theme.bg, Color::Rgb(245, 245, 250));
    }

    #[test]
    fn get_theme_unknown_falls_back_to_cyber_dark() {
        let theme = get_theme("nonexistent");
        assert_eq!(theme.bg, Color::Rgb(18, 18, 24));
    }

    #[test]
    fn get_theme_preserves_name_in_return() {
        let theme = get_theme("custom-theme");
        assert_eq!(theme.name, "custom-theme");
    }

    #[test]
    fn get_theme_cyber_dark_has_expected_colors() {
        let theme = get_theme("default");
        assert_eq!(theme.sidebar_bg, Color::Rgb(14, 14, 20));
        assert_eq!(theme.user_msg_color, Color::Rgb(100, 200, 255));
        assert_eq!(theme.assistant_msg_color, Color::Rgb(200, 220, 240));
        assert_eq!(theme.success_color, Color::Rgb(80, 255, 120));
        assert_eq!(theme.warning_color, Color::Rgb(255, 200, 60));
    }

    #[test]
    fn two_themes_have_different_accent_colors() {
        let dark = get_theme("default");
        let light = get_theme("daylight");
        assert_ne!(dark.accent, light.accent);
    }

    #[test]
    fn get_theme_empty_string_returns_cyber_dark() {
        let theme = get_theme("");
        assert_eq!(theme.bg, Color::Rgb(18, 18, 24));
    }

    #[test]
    fn get_theme_dracula() {
        let theme = get_theme("dracula");
        assert_eq!(theme.bg, Color::Rgb(40, 42, 54));
        assert_eq!(theme.accent, Color::Rgb(189, 147, 249));
    }

    #[test]
    fn get_theme_gruvbox() {
        let theme = get_theme("gruvbox");
        assert_eq!(theme.bg, Color::Rgb(40, 40, 40));
        assert_eq!(theme.accent, Color::Rgb(184, 187, 38));
    }

    #[test]
    fn get_theme_solarized_dark() {
        let theme = get_theme("solarized-dark");
        assert_eq!(theme.bg, Color::Rgb(0, 43, 54));
        assert_eq!(theme.accent, Color::Rgb(181, 137, 0));
    }

    #[test]
    fn get_theme_nord() {
        let theme = get_theme("nord");
        assert_eq!(theme.bg, Color::Rgb(46, 52, 64));
        assert_eq!(theme.accent, Color::Rgb(136, 192, 208));
    }

    #[test]
    fn get_theme_monokai() {
        let theme = get_theme("monokai");
        assert_eq!(theme.bg, Color::Rgb(39, 40, 34));
        assert_eq!(theme.accent, Color::Rgb(166, 226, 46));
    }

    #[test]
    fn get_theme_tokyo_night() {
        let theme = get_theme("tokyo-night");
        assert_eq!(theme.bg, Color::Rgb(26, 27, 38));
        assert_eq!(theme.accent, Color::Rgb(122, 162, 247));
    }

    #[test]
    fn get_theme_catppuccin() {
        let theme = get_theme("catppuccin");
        assert_eq!(theme.bg, Color::Rgb(30, 30, 46));
        assert_eq!(theme.accent, Color::Rgb(203, 166, 247));
    }

    #[test]
    fn get_theme_one_dark() {
        let theme = get_theme("one-dark");
        assert_eq!(theme.bg, Color::Rgb(40, 44, 52));
        assert_eq!(theme.accent, Color::Rgb(97, 175, 239));
    }

    #[test]
    fn get_theme_everforest() {
        let theme = get_theme("everforest");
        assert_eq!(theme.bg, Color::Rgb(45, 53, 59));
        assert_eq!(theme.accent, Color::Rgb(167, 192, 128));
    }

    #[test]
    fn all_themes_have_different_backgrounds() {
        let names = [
            "default", "daylight", "dracula", "gruvbox", "solarized-dark",
            "nord", "monokai", "tokyo-night", "catppuccin", "one-dark", "everforest",
        ];
        let mut seen = std::collections::HashSet::new();
        for name in &names {
            let theme = get_theme(name);
            let key = format!("{:?}", theme.bg);
            assert!(seen.insert(key), "主题 {} 的背景色与其他主题重复", name);
        }
    }

    #[test]
    fn all_theme_names_are_valid() {
        for name in ALL_THEME_NAMES {
            let theme = get_theme(name);
            assert!(!theme.name.is_empty());
        }
    }
}
