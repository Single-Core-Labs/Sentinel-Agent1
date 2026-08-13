//! Terminal theme: a swappable palette + capability detection.
//!
//! Render logic never hardcodes ANSI codes; it asks [`Theme::current`] for a
//! role color (`accent`, `success`, `error`, `muted`, ...). The palette is
//! resolved once from `[theme]` in `sentinel.toml` (via [`Theme::from_settings`])
//! and installed with [`Theme::install`]. Swap the palette without touching
//! `handler.rs` / `approval.rs` / `display.rs` by installing a different theme.
//!
//! Color fidelity degrades by capability: truecolor terminals get RGB,
//! 256-color terminals get indexed colors, and everything else falls back to
//! the ANSI 16 palette. Piped output stays uncolored because the `colored`
//! crate only emits codes on a TTY.

use colored::{Color, Colorize};
use sentinel_config::ThemeSettings;
use std::io::IsTerminal;
use std::sync::OnceLock;

/// What the terminal can actually render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermCap {
    /// ANSI 16-color (or no color). Flat accent, no gradient.
    Basic16,
    /// 256-color. Indexed accent, stepped gradient.
    Ansi256,
    /// Truecolor (`COLORTERM=truecolor`/`24bit`). RGB accent, smooth gradient.
    TrueColor,
}

/// Detect the terminal color capability from the environment.
pub fn detect_capability() -> TermCap {
    let colorterm = std::env::var("COLORTERM").unwrap_or_default().to_lowercase();
    if colorterm.contains("truecolor") || colorterm.contains("24bit") {
        return TermCap::TrueColor;
    }
    let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
    if term.contains("256color") || term.contains("xterm") && !term.contains("-m") {
        return TermCap::Ansi256;
    }
    TermCap::Basic16
}

/// Semantic roles rendered by the UI. Every role has a dedicated hue so the
/// same color never means two different things (e.g. warning vs denied).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Brand accent: prompt glyph, banner, active/thinking markers.
    Accent,
    /// Success / allowed / ok.
    Success,
    /// Hard failure.
    Error,
    /// Caution (e.g. yolo mode active).
    Warning,
    /// A permission *denial* — deliberately distinct from `Warning`.
    Deny,
    /// A policy veto — a stronger, separate failure hue.
    Veto,
    /// Secondary metadata (session id, token counts, prefixes).
    Muted,
    /// Neutral info / navigation (turns, links).
    Info,
    /// Inline code / code content.
    Code,
}

#[derive(Debug, Clone)]
struct Stop {
    basic: Color,
    fixed: u8,
    rgb: (u8, u8, u8),
}

/// The active palette. Cheap, `Send + Sync`, safe to share behind `Arc`.
#[derive(Debug, Clone)]
pub struct Theme {
    cap: TermCap,
    accent: Stop,
    success: Stop,
    error: Stop,
    warning: Stop,
    deny: Stop,
    veto: Stop,
    muted: Stop,
    info: Stop,
    code: Stop,
}

fn stop(basic: Color, fixed: u8, rgb: (u8, u8, u8)) -> Stop {
    Stop { basic, fixed, rgb }
}

impl Stop {
    fn color(&self, cap: TermCap) -> Color {
        match cap {
            TermCap::TrueColor => Color::TrueColor {
                r: self.rgb.0,
                g: self.rgb.1,
                b: self.rgb.2,
            },
            // Ansi256 goes through raw SGR (`paint_ansi256`), never `Color`.
            TermCap::Ansi256 | TermCap::Basic16 => self.basic,
        }
    }
}

/// The default brand palette: violet accent (distinct from the ubiquitous
/// green/cyan), amber for warnings, orange for denials, rose for vetoes.
impl Theme {
    pub fn default_for(cap: TermCap) -> Self {
        Self {
            cap,
            accent: stop(Color::BrightMagenta, 141, (167, 139, 250)),
            success: stop(Color::Green, 78, (74, 222, 128)),
            error: stop(Color::Red, 203, (248, 113, 113)),
            warning: stop(Color::Yellow, 221, (251, 191, 36)),
            deny: stop(Color::BrightRed, 208, (249, 115, 22)),
            veto: stop(Color::Magenta, 204, (244, 63, 94)),
            muted: stop(Color::White, 244, (148, 163, 184)),
            info: stop(Color::Cyan, 80, (45, 212, 191)),
            code: stop(Color::BrightCyan, 229, (253, 224, 71)),
        }
    }

    /// Resolve the palette for a `[theme]` config block. `name` picks a preset
    /// (which only changes the brand accent), `accent` overrides it outright.
    pub fn from_settings(settings: &ThemeSettings) -> Self {
        let mut t = Self::default_for(detect_capability());
        match settings.name.as_str() {
            "paper" => {
                t.accent = stop(Color::BrightBlue, 75, (96, 165, 250));
            }
            "warp" => {
                t.accent = stop(Color::BrightMagenta, 213, (240, 130, 220));
            }
            "gemini" => {
                t.accent = stop(Color::BrightBlue, 39, (91, 158, 255));
            }
            _ => {}
        }
        if let Some(hex) = settings.accent.as_deref()
            && let Some(s) = parse_accent(hex)
        {
            t.accent = s;
        }
        t
    }

    /// Install `t` as the process-wide theme used by the renderers. Entry
    /// points call this once at startup with the config-resolved theme.
    pub fn install(t: Theme) {
        let _ = THEME.set(t);
    }

    /// The active theme — installed, or auto-detected default if not installed.
    pub fn current() -> &'static Theme {
        THEME.get_or_init(|| Theme::default_for(detect_capability()))
    }

    fn color(&self, role: Role) -> Color {
        let s = self.stop_for(role);
        s.color(self.cap)
    }

    fn index(&self, role: Role) -> u8 {
        self.stop_for(role).fixed
    }

    fn stop_for(&self, role: Role) -> &Stop {
        match role {
            Role::Accent => &self.accent,
            Role::Success => &self.success,
            Role::Error => &self.error,
            Role::Warning => &self.warning,
            Role::Deny => &self.deny,
            Role::Veto => &self.veto,
            Role::Muted => &self.muted,
            Role::Info => &self.info,
            Role::Code => &self.code,
        }
    }

    fn paint(&self, role: Role, style: Style, text: &str) -> String {
        if self.cap == TermCap::Ansi256 {
            return self.paint_ansi256(self.index(role), style, text);
        }
        let c = text.color(self.color(role));
        let c = match style {
            Style::Plain => c,
            Style::Bold => c.bold(),
            Style::Dim => c.dimmed(),
        };
        c.to_string()
    }

    /// Emit raw 256-color SGR (`colored` 2.x has no indexed-color variant).
    /// Gated on the same tty/`NO_COLOR` logic `colored` uses.
    fn paint_ansi256(&self, n: u8, style: Style, text: &str) -> String {
        if !colored::control::SHOULD_COLORIZE.should_colorize() {
            return text.to_string();
        }
        ensure_virtual_terminal();
        let mut codes = format!("38;5;{}", n);
        if matches!(style, Style::Bold) {
            codes.push_str(";1");
        }
        if matches!(style, Style::Dim) {
            codes.push_str(";2");
        }
        format!("\x1b[{}m{}\x1b[0m", codes, text)
    }

    pub fn accent(&self, text: &str) -> String {
        self.paint(Role::Accent, Style::Plain, text)
    }
    pub fn accent_bold(&self, text: &str) -> String {
        self.paint(Role::Accent, Style::Bold, text)
    }
    pub fn success(&self, text: &str) -> String {
        self.paint(Role::Success, Style::Plain, text)
    }
    pub fn success_bold(&self, text: &str) -> String {
        self.paint(Role::Success, Style::Bold, text)
    }
    pub fn error(&self, text: &str) -> String {
        self.paint(Role::Error, Style::Plain, text)
    }
    pub fn error_bold(&self, text: &str) -> String {
        self.paint(Role::Error, Style::Bold, text)
    }
    pub fn warning(&self, text: &str) -> String {
        self.paint(Role::Warning, Style::Plain, text)
    }
    pub fn deny(&self, text: &str) -> String {
        self.paint(Role::Deny, Style::Plain, text)
    }
    pub fn deny_bold(&self, text: &str) -> String {
        self.paint(Role::Deny, Style::Bold, text)
    }
    pub fn veto(&self, text: &str) -> String {
        self.paint(Role::Veto, Style::Plain, text)
    }
    pub fn veto_bold(&self, text: &str) -> String {
        self.paint(Role::Veto, Style::Bold, text)
    }
    pub fn muted(&self, text: &str) -> String {
        self.paint(Role::Muted, Style::Dim, text)
    }
    pub fn info(&self, text: &str) -> String {
        self.paint(Role::Info, Style::Plain, text)
    }
    pub fn code(&self, text: &str) -> String {
        self.paint(Role::Code, Style::Plain, text)
    }
    /// Neutral bold (plain foreground, just weight).
    pub fn bold(&self, text: &str) -> String {
        text.bold().to_string()
    }

    /// Render a wordmark/gradient banner line. Smooth gradient on truecolor,
    /// stepped on 256-color, flat accent on basic terminals.
    pub fn gradient(&self, text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            return String::new();
        }
        let n = chars.len();
        chars
            .into_iter()
            .enumerate()
            .map(|(i, ch)| {
                let t = if n == 1 { 0.0 } else { i as f32 / (n - 1) as f32 };
                match self.cap {
                    TermCap::Ansi256 => {
                        let idx = lerp_u8(self.accent.fixed, 213, t);
                        self.paint_ansi256(idx, Style::Bold, &ch.to_string())
                    }
                    TermCap::TrueColor => ch
                        .to_string()
                        .color(Color::TrueColor {
                            r: lerp_u8(167, 250, t),
                            g: lerp_u8(139, 124, t),
                            b: lerp_u8(250, 160, t),
                        })
                        .bold()
                        .to_string(),
                    TermCap::Basic16 => self.accent(&ch.to_string()),
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy)]
enum Style {
    Plain,
    Bold,
    Dim,
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

static VT_ENABLED: OnceLock<()> = OnceLock::new();

/// Enable Windows Virtual Terminal processing once, so raw SGR (256-color)
/// renders correctly on modern Windows consoles.
fn ensure_virtual_terminal() {
    let _ = VT_ENABLED.get_or_init(|| {
        #[cfg(windows)]
        {
            let _ = colored::control::set_virtual_terminal(true);
        }
        #[cfg(not(windows))]
        {
            ()
        }
    });
}

fn parse_accent(hex: &str) -> Option<Stop> {
    let hex = hex.trim();
    if let Some(rest) = hex.strip_prefix('#')
        && rest.len() == 6
    {
        let r = u8::from_str_radix(&rest[0..2], 16).ok()?;
        let g = u8::from_str_radix(&rest[2..4], 16).ok()?;
        let b = u8::from_str_radix(&rest[4..6], 16).ok()?;
        return Some(stop(nearest_basic(r, g, b), nearest_fixed(r, g, b), (r, g, b)));
    }
    // Named ANSI accents: carry an rgb proxy so truecolor terminals render a
    // sensible hue instead of the default violet.
    let (named, rgb) = match hex.to_ascii_lowercase().as_str() {
        "black" => (Color::Black, (80, 80, 80)),
        "red" => (Color::Red, (205, 49, 49)),
        "green" => (Color::Green, (13, 188, 121)),
        "yellow" => (Color::Yellow, (229, 229, 16)),
        "blue" => (Color::Blue, (36, 114, 200)),
        "magenta" => (Color::Magenta, (188, 63, 188)),
        "cyan" => (Color::Cyan, (17, 168, 205)),
        "white" => (Color::White, (229, 229, 229)),
        "brightred" => (Color::BrightRed, (255, 94, 94)),
        "brightgreen" => (Color::BrightGreen, (64, 255, 150)),
        "brightyellow" => (Color::BrightYellow, (255, 235, 66)),
        "brightblue" => (Color::BrightBlue, (94, 156, 255)),
        "brightmagenta" => (Color::BrightMagenta, (255, 94, 255)),
        "brightcyan" => (Color::BrightCyan, (94, 255, 255)),
        "brightwhite" => (Color::BrightWhite, (255, 255, 255)),
        _ => return None,
    };
    Some(Stop {
        basic: named,
        fixed: nearest_fixed(rgb.0, rgb.1, rgb.2),
        rgb,
    })
}

fn nearest_fixed(r: u8, g: u8, b: u8) -> u8 {
    // 6x6x6 color cube starts at fixed index 16.
    let ri = ((r as u16 * 5 + 127) / 255) as u8;
    let gi = ((g as u16 * 5 + 127) / 255) as u8;
    let bi = ((b as u16 * 5 + 127) / 255) as u8;
    16 + 36 * ri + 6 * gi + bi
}

fn nearest_basic(r: u8, g: u8, b: u8) -> Color {
    // Pick the closest of the 8 saturated ANSI colors by luminance-weighted
    // distance — good enough for a 16-color fallback.
    const COLORS: [(u8, u8, u8, Color); 8] = [
        (0, 0, 0, Color::Black),
        (128, 0, 0, Color::Red),
        (0, 128, 0, Color::Green),
        (128, 128, 0, Color::Yellow),
        (0, 0, 128, Color::Blue),
        (128, 0, 128, Color::Magenta),
        (0, 128, 128, Color::Cyan),
        (192, 192, 192, Color::White),
    ];
    let mut best = Color::White;
    let mut best_d = f32::MAX;
    for (cr, cg, cb, c) in COLORS {
        let d = ((r as f32 - cr as f32).powi(2) * 0.3
            + (g as f32 - cg as f32).powi(2) * 0.59
            + (b as f32 - cb as f32).powi(2) * 0.11)
            .sqrt();
        if d < best_d {
            best_d = d;
            best = c;
        }
    }
    best
}

static THEME: OnceLock<Theme> = OnceLock::new();

/// True when stdout is an interactive terminal (used to gate in-place `\r`
/// spinner redraws — never write control codes into piped output).
pub fn stdout_is_tty() -> bool {
    std::io::stdout().is_terminal()
}
