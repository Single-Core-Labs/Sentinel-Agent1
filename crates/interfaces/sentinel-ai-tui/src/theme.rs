use ratatui::style::Color;

// ── Palette constants ────────────────────────────────────────────────────────

/// Near-black background used by Claude Code / Gemini CLI
pub const BG: Color = Color::Rgb(13, 13, 13);
/// Slightly lighter surface for subtle depth
pub const BG_SURFACE: Color = Color::Rgb(20, 20, 20);
/// Electric green — primary accent (matches Claude Code's green)
pub const GREEN: Color = Color::Rgb(74, 222, 128);
/// Dimmer green for decorators / separators
pub const GREEN_DIM: Color = Color::Rgb(34, 120, 70);
/// Blue-white — user messages
pub const USER_BLUE: Color = Color::Rgb(147, 197, 253);
/// Warm amber — warnings / thinking
pub const AMBER: Color = Color::Rgb(245, 166, 35);
/// Muted red — errors
pub const RED: Color = Color::Rgb(248, 113, 113);
/// Violet — tool names
pub const VIOLET: Color = Color::Rgb(167, 139, 250);
/// Cyan — info / observations
pub const CYAN: Color = Color::Rgb(34, 211, 238);
/// Primary text
pub const FG: Color = Color::Rgb(229, 231, 235);
/// Secondary text
pub const FG_MUTED: Color = Color::Rgb(107, 114, 128);
/// Very dim — borders, dividers
pub const FG_DIM: Color = Color::Rgb(55, 65, 81);
/// Input box border when active
pub const BORDER_ACTIVE: Color = Color::Rgb(74, 222, 128);

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
    pub bg: Color,
    pub bg_surface: Color,
}

pub struct ThemeConfig {
    pub name: &'static str,
    pub colors: ThemeColors,
    pub spinner_frames: &'static [&'static str],
    pub particle_chars: &'static [&'static str],
}

// ── Sentinel-Claude (default) ────────────────────────────────────────────────
/// The primary theme — dark near-black background, electric-green accent,
/// clean open layout.  Matches the visual language of Claude Code / Gemini CLI.
pub fn sentinel_claude_theme() -> ThemeConfig {
    ThemeConfig {
        name: "sentinel",
        colors: ThemeColors {
            foreground: FG,
            muted: FG_MUTED,
            accent: GREEN,
            accent_alt: CYAN,
            success: GREEN,
            error: RED,
            warning: AMBER,
            info: CYAN,
            border: FG_DIM,
            dim_border: Color::Rgb(35, 40, 48),
            tool_call_fg: VIOLET,
            plan_fg: GREEN,
            user_fg: USER_BLUE,
            assistant_fg: FG,
            approval_border: AMBER,
            spinner: GREEN,
            bg: BG,
            bg_surface: BG_SURFACE,
        },
        // Braille spinner — same cadence as Claude Code
        spinner_frames: &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"],
        particle_chars: &["·", "•", "◦", "∘", "○", "◌", "◎", "◉", "◈", "◆"],
    }
}

// ── Dark (legacy default) ────────────────────────────────────────────────────
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
            bg: Color::Rgb(10, 10, 15),
            bg_surface: Color::Rgb(17, 24, 39),
        },
        spinner_frames: &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
        particle_chars: &[
            "·", "•", "◦", "∘", "○", "◌", "◎", "◉", "◈", "◆", "◇", "▪", "▫", "▸", "▹",
        ],
    }
}

// ── High-contrast ────────────────────────────────────────────────────────────
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
            bg: Color::Black,
            bg_surface: Color::Rgb(20, 20, 20),
        },
        spinner_frames: &["|", "/", "-", "\\"],
        particle_chars: &[".", "*", "+", "x", "o", "0", "#", "@", "%", "&"],
    }
}

// ── Cyber ────────────────────────────────────────────────────────────────────
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
            bg: Color::Rgb(5, 10, 20),
            bg_surface: Color::Rgb(10, 18, 30),
        },
        spinner_frames: &["◐", "◓", "◑", "◒"],
        particle_chars: &["▓", "▒", "░", "█", "▄", "▀", "■", "□", "▪", "▫"],
    }
}

pub fn get_theme(name: &str) -> ThemeConfig {
    match name {
        "sentinel" | "claude" => sentinel_claude_theme(),
        "dark" => dark_theme(),
        "high-contrast" => high_contrast_theme(),
        "cyber" => cyber_theme(),
        _ => sentinel_claude_theme(),
    }
}
