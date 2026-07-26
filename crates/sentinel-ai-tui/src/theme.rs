use ratatui::style::Color;

pub struct ThemeColors {
    pub foreground: Color,
    pub muted: Color,
    pub accent: Color,
    pub accent_alt: Color,
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub info: Color,
    pub border: Color,
    pub dim_border: Color,
    pub tool_call_fg: Color,
    pub plan_fg: Color,
    pub user_fg: Color,
    pub assistant_fg: Color,
    pub approval_border: Color,
    pub spinner: Color,
}

pub struct ThemeConfig {
    pub name: &'static str,
    pub colors: ThemeColors,
    pub spinner_frames: &'static [&'static str],
    pub particle_chars: &'static [&'static str],
}

pub fn dark_theme() -> ThemeConfig {
    ThemeConfig {
        name: "dark",
        colors: ThemeColors {
            foreground: Color::Rgb(226, 232, 240),
            muted: Color::Rgb(100, 116, 139),
            accent: Color::Rgb(249, 115, 22),
            accent_alt: Color::Rgb(14, 165, 233),
            success: Color::Rgb(34, 197, 94),
            error: Color::Rgb(239, 68, 68),
            warning: Color::Rgb(245, 158, 11),
            info: Color::Rgb(56, 189, 248),
            border: Color::Rgb(51, 65, 85),
            dim_border: Color::Rgb(30, 41, 59),
            tool_call_fg: Color::Rgb(167, 139, 250),
            plan_fg: Color::Rgb(52, 211, 153),
            user_fg: Color::Rgb(147, 197, 253),
            assistant_fg: Color::Rgb(226, 232, 240),
            approval_border: Color::Rgb(245, 158, 11),
            spinner: Color::Rgb(249, 115, 22),
        },
        spinner_frames: &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
        particle_chars: &["·", "•", "◦", "∘", "○", "◌", "◎", "◉", "◈", "◆", "◇", "▪", "▫", "▸", "▹"],
    }
}

pub fn high_contrast_theme() -> ThemeConfig {
    ThemeConfig {
        name: "high-contrast",
        colors: ThemeColors {
            foreground: Color::Rgb(255, 255, 255),
            muted: Color::Rgb(170, 170, 170),
            accent: Color::Rgb(255, 255, 0),
            accent_alt: Color::Rgb(0, 255, 255),
            success: Color::Rgb(0, 255, 0),
            error: Color::Rgb(255, 68, 68),
            warning: Color::Rgb(255, 170, 0),
            info: Color::Rgb(0, 170, 255),
            border: Color::Rgb(136, 136, 136),
            dim_border: Color::Rgb(68, 68, 68),
            tool_call_fg: Color::Rgb(255, 136, 255),
            plan_fg: Color::Rgb(136, 255, 170),
            user_fg: Color::Rgb(136, 204, 255),
            assistant_fg: Color::Rgb(255, 255, 255),
            approval_border: Color::Rgb(255, 170, 0),
            spinner: Color::Rgb(255, 255, 0),
        },
        spinner_frames: &["|", "/", "-", "\\"],
        particle_chars: &[".", "*", "+", "x", "o", "0", "#", "@", "%", "&"],
    }
}

pub fn cyber_theme() -> ThemeConfig {
    ThemeConfig {
        name: "cyber",
        colors: ThemeColors {
            foreground: Color::Rgb(224, 240, 255),
            muted: Color::Rgb(74, 106, 138),
            accent: Color::Rgb(0, 255, 208),
            accent_alt: Color::Rgb(255, 0, 110),
            success: Color::Rgb(0, 255, 159),
            error: Color::Rgb(255, 0, 110),
            warning: Color::Rgb(255, 214, 0),
            info: Color::Rgb(0, 200, 255),
            border: Color::Rgb(13, 59, 92),
            dim_border: Color::Rgb(7, 30, 46),
            tool_call_fg: Color::Rgb(255, 0, 110),
            plan_fg: Color::Rgb(0, 255, 208),
            user_fg: Color::Rgb(0, 200, 255),
            assistant_fg: Color::Rgb(224, 240, 255),
            approval_border: Color::Rgb(255, 214, 0),
            spinner: Color::Rgb(0, 255, 208),
        },
        spinner_frames: &["◐", "◓", "◑", "◒"],
        particle_chars: &["▓", "▒", "░", "█", "▄", "▀", "■", "□", "▪", "▫"],
    }
}

pub fn get_theme(name: &str) -> ThemeConfig {
    match name {
        "dark" => dark_theme(),
        "high-contrast" => high_contrast_theme(),
        "cyber" => cyber_theme(),
        _ => dark_theme(),
    }
}
