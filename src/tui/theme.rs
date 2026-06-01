use crate::config::ThemeMode;
use ratatui::style::Color;
use std::sync::OnceLock;

/// All semantic color slots used by the TUI.
///
/// Obtain the active instance via [`theme()`].
#[derive(Debug, Clone)]
pub struct Theme {
    pub accent: Color,
    pub accent_alt: Color,
    pub info: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub text: Color,
    pub text_muted: Color,
    pub text_dim: Color,
    pub text_faint: Color,
    pub line: Color,
    pub line_soft: Color,
    pub bg: Color,
    pub bg_raised: Color,
}

static THEME: OnceLock<Theme> = OnceLock::new();

/// Return the process-wide [`Theme`] reference.
///
/// Must be initialized via [`init_theme`] before the first call.  If called
/// before initialization the truecolor default is returned (safe for tests).
pub fn theme() -> &'static Theme {
    THEME.get_or_init(Theme::default)
}

/// Initialize the process-wide theme from config and terminal capabilities.
///
/// Calling this more than once is a no-op — the first call wins.
pub fn init_theme(mode: ThemeMode) {
    let resolved = match mode {
        ThemeMode::Auto => {
            if terminal_supports_truecolor() {
                Theme::default()
            } else {
                Theme::sixteen()
            }
        }
        ThemeMode::Default => Theme::default(),
        ThemeMode::Sixteen => Theme::sixteen(),
        ThemeMode::ColorblindSafe => Theme::colorblind_safe(),
    };
    let _ = THEME.set(resolved);
}

/// Detect whether the running terminal supports 24-bit ("truecolor") color.
///
/// Checks `COLORTERM` first (most reliable), then falls back to inspecting
/// `TERM` for known 16-color-only values.  When neither env var is set the
/// function assumes truecolor is available, because ratatui will down-convert
/// to 256-color nearest-match automatically — only the basic-16 case needs
/// explicit switching.
fn terminal_supports_truecolor() -> bool {
    // COLORTERM=truecolor or 24bit → explicit truecolor support
    if let Ok(val) = std::env::var("COLORTERM") {
        let v = val.trim().to_ascii_lowercase();
        if v == "truecolor" || v == "24bit" {
            return true;
        }
        // COLORTERM set to something else → not truecolor
        return false;
    }

    // TERM values that are definitively 16-color only
    if let Ok(term) = std::env::var("TERM") {
        let t = term.trim().to_ascii_lowercase();
        if matches!(
            t.as_str(),
            "linux" | "ansi" | "vt100" | "vt220" | "dumb" | "cons25"
        ) {
            return false;
        }
    }

    // Default: assume capable (ratatui handles downconversion for 256c)
    true
}

impl Default for Theme {
    /// Default Catppuccin Mocha–style truecolor palette.
    fn default() -> Self {
        Self {
            accent: Color::Rgb(67, 171, 229),
            accent_alt: Color::Rgb(217, 119, 87),
            info: Color::Rgb(116, 199, 236),
            success: Color::Rgb(166, 227, 161),
            warning: Color::Rgb(249, 226, 175),
            danger: Color::Rgb(243, 139, 168),
            text: Color::Rgb(205, 214, 244),
            text_muted: Color::Rgb(186, 194, 222),
            text_dim: Color::Rgb(166, 173, 200),
            text_faint: Color::Rgb(127, 132, 156),
            line: Color::Rgb(69, 71, 90),
            line_soft: Color::Rgb(49, 50, 68),
            bg: Color::Rgb(30, 30, 46),
            bg_raised: Color::Rgb(24, 24, 37),
        }
    }
}

impl Theme {
    /// 16-color ANSI fallback.  Every slot maps to a semantically close basic
    /// ANSI color so the UI remains readable on `TERM=linux` or similar.
    ///
    /// `warning` → Yellow, `text_dim` → Gray — the two colors used together on
    /// the quit toast remain visually distinct on all 16-color terminals.
    ///
    /// `bg` and `bg_raised` both map to Black: 16-color terminals have no
    /// "slightly lighter black", so raised panels intentionally collapse to the
    /// base background and rely on their borders for visual separation.  This is
    /// not a bug — do not introduce a DarkGray fill here; solid gray blocks
    /// regress the look on capable terminals and look heavy on basic ones.
    pub fn sixteen() -> Self {
        Self {
            accent: Color::Blue,
            accent_alt: Color::Yellow,
            info: Color::Cyan,
            success: Color::Green,
            // Yellow for WARNING — distinct from text_dim (Gray) on BG (Black)
            warning: Color::Yellow,
            danger: Color::Red,
            text: Color::White,
            text_muted: Color::Gray,
            // Gray — "slightly muted" level; brighter than text_faint (DarkGray)
            text_dim: Color::Gray,
            // DarkGray — dimmest text level; visually subordinate to text_dim
            text_faint: Color::DarkGray,
            line: Color::DarkGray,
            line_soft: Color::Black,
            // Both collapse to Black — panels rely on borders for separation; see
            // the constructor doc above for the rationale.
            bg: Color::Black,
            bg_raised: Color::Black,
        }
    }

    /// Colorblind-safe palette tuned for deuteranopia and protanopia.
    ///
    /// Uses the Wong (2011) and IBM (2019) colorblind-safe palettes.
    /// Blue/yellow semantics replace red/green to avoid confusion:
    /// - `success` → blue (distinguishable from danger for CB users)
    /// - `danger` → orange-red (distinguishable from success for CB users)
    /// - `warning` → amber/yellow (remains distinct from success blue)
    pub fn colorblind_safe() -> Self {
        Self {
            // Sapphire blue — Wong #0072B2
            accent: Color::Rgb(0, 114, 178),
            // Orange — Wong #E69F00 (distinct from blue for all CB types)
            accent_alt: Color::Rgb(230, 159, 0),
            // Sky blue — Wong #56B4E9
            info: Color::Rgb(86, 180, 233),
            // Blue (not green) so CB users can distinguish success from danger
            // Wong #0072B2 shifted lighter: #009DE0
            success: Color::Rgb(0, 157, 224),
            // Amber/yellow — Wong #F0E442 (distinct from both blue and orange-red)
            warning: Color::Rgb(240, 228, 66),
            // Vermillion — Wong #D55E00 (orange-red, distinct from blue success)
            danger: Color::Rgb(213, 94, 0),
            // Neutral text — high contrast on dark BG
            text: Color::Rgb(220, 220, 235),
            text_muted: Color::Rgb(180, 185, 205),
            // text_dim used alongside warning (amber) on the quit toast:
            // #9CA3AF is a blue-gray — distinct from #F0E442 amber
            text_dim: Color::Rgb(156, 163, 175),
            text_faint: Color::Rgb(110, 115, 135),
            line: Color::Rgb(60, 65, 85),
            line_soft: Color::Rgb(42, 45, 62),
            // Dark navy BG — IBM recommended dark background
            bg: Color::Rgb(20, 22, 38),
            bg_raised: Color::Rgb(14, 16, 28),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tui_theme.rs"]
mod tests;
