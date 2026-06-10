use crate::config::ThemeMode;
use ratatui::style::Color;
use std::sync::OnceLock;

/// Capability tier reported by a resolved [`Theme`].
///
/// Widgets that need tier-dependent rendering (sub-cell progress glyphs
/// `▏▎▍▌▋▊▉` vs full blocks, slide-switch vs `[on]`/`[off]` toggle) can
/// branch on this value — all other glyphs are shared across tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// 24-bit RGB truecolor.
    Full,
    /// xterm-256 compatible colors.
    Compatible,
}

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
    /// Blurred panel borders, soft rules, scrollbar track (LINE: 49,50,68 / 238).
    pub line: Color,
    /// Focused panel borders (LINE_STRONG: 69,71,90 / 240).
    pub line_strong: Color,
    pub bg: Color,
    pub bg_raised: Color,
    /// Selected row tint (BG_HOVER: 40,40,56 / 236).
    pub bg_hover: Color,
    /// Toast / sunken surface (BG_SUNKEN: 17,17,27 / 233).
    pub bg_sunken: Color,
    tier: Tier,
}

impl Theme {
    /// Returns the capability tier this theme was built for.
    pub fn tier(&self) -> Tier {
        self.tier
    }
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
                Theme::full()
            } else {
                Theme::compatible()
            }
        }
        ThemeMode::Full => Theme::full(),
        ThemeMode::Compatible => Theme::compatible(),
    };
    let _ = THEME.set(resolved);
}

/// Detect whether the running terminal supports 24-bit ("truecolor") color.
///
/// Returns `true` only when `$COLORTERM` (trimmed, lowercased) is exactly
/// `"truecolor"`.  Any other value — unset, empty, `"24bit"`, or anything
/// else — returns `false` and the compatible xterm-256 palette is used.
fn terminal_supports_truecolor() -> bool {
    std::env::var("COLORTERM")
        .map(|v| v.trim().eq_ignore_ascii_case("truecolor"))
        .unwrap_or(false)
}

impl Default for Theme {
    fn default() -> Self {
        Theme::full()
    }
}

impl Theme {
    /// Full Catppuccin Mocha truecolor (RGB) palette.
    pub fn full() -> Self {
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
            line: Color::Rgb(49, 50, 68),
            line_strong: Color::Rgb(69, 71, 90),
            bg: Color::Rgb(30, 30, 46),
            bg_raised: Color::Rgb(24, 24, 37),
            bg_hover: Color::Rgb(40, 40, 56),
            bg_sunken: Color::Rgb(17, 17, 27),
            tier: Tier::Full,
        }
    }

    /// xterm-256 compatible palette — maps every slot to the nearest indexed color.
    ///
    /// `bg` and `bg_raised` both collapse to xterm 235 / 234 which are close
    /// enough; panels rely on borders (not fill) for visual separation.
    pub fn compatible() -> Self {
        Self {
            accent: Color::Indexed(75),
            accent_alt: Color::Indexed(173),
            info: Color::Indexed(117),
            success: Color::Indexed(151),
            warning: Color::Indexed(223),
            danger: Color::Indexed(211),
            text: Color::Indexed(189),
            text_muted: Color::Indexed(189),
            text_dim: Color::Indexed(145),
            text_faint: Color::Indexed(102),
            line: Color::Indexed(238),
            line_strong: Color::Indexed(240),
            bg: Color::Indexed(235),
            bg_raised: Color::Indexed(234),
            bg_hover: Color::Indexed(236),
            bg_sunken: Color::Indexed(233),
            tier: Tier::Compatible,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tui_theme.rs"]
mod tests;
