//! 主题系统 —— 定义终端颜色方案。
//!
//! 支持多种内置主题，可通过斜杠命令 /theme 动态切换。

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

/// 根据名称获取主题
pub fn get_theme(name: &str) -> Theme {
    let mut theme = match name {
        "daylight" | "light" => THEME_DAYLIGHT,
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
}
