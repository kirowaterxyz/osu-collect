use std::borrow::Cow;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::config::constants::{DISK_DANGER_BYTES, DISK_WARN_BYTES};

use super::theme::blend;
use super::{accent, accent_alt, danger, format_free_space, text_dim, text_faint, warning};

const BRAND: &str = " osu!collect";
const VERSION: &str = concat!(" v", env!("CARGO_PKG_VERSION"), " ");

/// Aggregated state passed into the header renderer.
///
/// Build via [`StatusPill::compute`]; pass `None` when the pill should be hidden.
pub struct StatusPill {
    /// Free bytes on the output filesystem — always below [`DISK_WARN_BYTES`]
    /// (above that the pill is hidden, so this is never a "healthy" value).
    pub disk_free: u64,
}

impl StatusPill {
    /// Returns `None` unless free space is below [`DISK_WARN_BYTES`].
    ///
    /// The pill exists only to warn about low disk; above the warn threshold (or
    /// when free space is unknown) it stays hidden so the header is quiet during
    /// normal use.
    pub fn compute(disk_free: Option<u64>) -> Option<Self> {
        let disk_free = disk_free.filter(|&b| b < DISK_WARN_BYTES)?;
        Some(Self { disk_free })
    }

    /// Formatted pill text and the color each segment should use.
    ///
    /// Returns a list of `(text, color)` pairs — caller renders them as spans.
    pub fn segments(&self) -> Vec<(String, Color)> {
        let color = if self.disk_free < DISK_DANGER_BYTES {
            danger()
        } else {
            warning()
        };
        vec![(format_free_space(self.disk_free), color)]
    }

    /// Total display width of the pill (sum of all segment char lengths) plus leading space.
    pub fn display_width(&self) -> u16 {
        let segs = self.segments();
        if segs.is_empty() {
            return 0;
        }
        // 1 leading space + char count of all segments (ASCII, so byte len == char count)
        let chars: usize = segs.iter().map(|(s, _)| s.len()).sum();
        (1 + chars) as u16
    }
}

/// Header inputs other than the frame. Grouped into a struct so the brand
/// animation inputs (`tick`, `downloading`) ride along without a long argument
/// list. The `frame` is a separate `render` argument to keep its borrow tight.
pub struct RenderParams<'a, 't> {
    pub area: Rect,
    pub tabs: &'a [Cow<'t, str>],
    pub active: usize,
    pub pill: Option<&'a StatusPill>,
    /// Global frame tick; drives the brand shimmer phase.
    pub tick: u64,
    /// True while any download is in a non-terminal stage; idle renders the
    /// brand statically.
    pub downloading: bool,
    /// Ease-in ramp (0..1) for the shimmer. Rises from 0 when downloading
    /// begins so the animation fades in instead of cutting in.
    pub brand_ramp: f32,
}

pub fn render(frame: &mut Frame, params: RenderParams<'_, '_>) {
    let RenderParams {
        area,
        tabs,
        active,
        pill,
        tick,
        downloading,
        brand_ramp,
    } = params;

    if area.width == 0 || area.height == 0 {
        return;
    }

    let version_width = VERSION.len() as u16;
    let brand_width = BRAND.chars().count() as u16;
    let pill_width = pill.map(|p| p.display_width()).unwrap_or(0);

    let layout = Layout::horizontal([
        Constraint::Length(brand_width),
        Constraint::Min(0),
        Constraint::Length(pill_width),
        Constraint::Length(version_width),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(brand_spans(tick, downloading, brand_ramp))),
        layout[0],
    );

    let mut spans: Vec<Span<'_>> = Vec::with_capacity(tabs.len() * 3);
    // bullet separator between brand and first tab
    spans.push(Span::styled("  •  ", Style::default().fg(text_dim())));
    for (index, title) in tabs.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        let style = if index == active {
            Style::default()
                .fg(accent())
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(text_dim())
        };
        spans.push(Span::styled(title.clone(), style));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Left),
        layout[1],
    );

    if let Some(pill) = pill {
        let segs = pill.segments();
        if !segs.is_empty() {
            let mut pill_spans: Vec<Span<'static>> = Vec::with_capacity(segs.len() + 1);
            // Leading space so the pill has breathing room before the version.
            pill_spans.push(Span::styled(" ", Style::default().fg(text_faint())));
            for (text, color) in segs {
                pill_spans.push(Span::styled(text, Style::default().fg(color)));
            }
            frame.render_widget(
                Paragraph::new(Line::from(pill_spans)).alignment(Alignment::Right),
                layout[2],
            );
        }
    }

    frame.render_widget(
        Paragraph::new(VERSION)
            .style(Style::default().fg(text_faint()))
            .alignment(Alignment::Right),
        layout[3],
    );
}

/// Build the brand line. Idle: a static bold `accent_alt` wordmark. Downloading:
/// a subtle accent shimmer sweeps left-to-right across the letters, so the brand
/// reads as a quiet live-status cue rather than a loud spinner.
///
/// `ramp` (0..1) scales the shimmer depth so it fades in over the first frames of
/// a download instead of cutting straight to full strength — at `ramp == 0` every
/// letter sits on the base color (indistinguishable from idle).
fn brand_spans(tick: u64, downloading: bool, ramp: f32) -> Vec<Span<'static>> {
    let base = accent_alt();
    if !downloading {
        return vec![Span::styled(
            BRAND,
            Style::default().fg(base).add_modifier(Modifier::BOLD),
        )];
    }

    // A cosine wave crest travels across the brand. `WAVE_SPAN` controls the
    // sweep speed (smaller = faster); `MAX_MIX` caps how far each letter leans
    // toward the accent so it pulses instead of strobing. `ramp` eases the depth
    // up from 0 so the shimmer materializes gently rather than snapping on.
    const WAVE_SPAN: f32 = 2.6;
    const MAX_MIX: f32 = 0.65;
    let depth = MAX_MIX * ramp.clamp(0.0, 1.0);
    let highlight = accent();
    let crest = tick as f32 / WAVE_SPAN;
    let chars: Vec<char> = BRAND.chars().collect();
    let len = chars.len().max(1) as f32;

    chars
        .into_iter()
        .enumerate()
        .map(|(i, ch)| {
            // Distance of this column from the moving crest, wrapped over the
            // wordmark width so the sweep loops seamlessly.
            let phase = (i as f32 / len) * std::f32::consts::TAU;
            // 0..1 brightness: 1 at the crest, easing to 0 between sweeps.
            let mix = ((phase - crest).cos() * 0.5 + 0.5) * depth;
            let fg = blend(highlight, base, mix);
            Span::styled(
                ch.to_string(),
                Style::default().fg(fg).add_modifier(Modifier::BOLD),
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/tui_header.rs"]
mod tests;
