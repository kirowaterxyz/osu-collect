use std::borrow::Cow;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::theme::blend;
use super::{accent, accent_alt, text_dim};

const BRAND: &str = " osu!collect";
const VERSION: &str = concat!(" v", env!("CARGO_PKG_VERSION"), " ");

/// Header inputs other than the frame. Grouped into a struct so the brand
/// animation inputs (`tick`, `downloading`) ride along without a long argument
/// list. The `frame` is a separate `render` argument to keep its borrow tight.
pub struct RenderParams<'a, 't> {
    pub area: Rect,
    pub tabs: &'a [Cow<'t, str>],
    pub active: usize,
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
        tick,
        downloading,
        brand_ramp,
    } = params;

    if area.width == 0 || area.height == 0 {
        return;
    }

    let version_width = VERSION.len() as u16;
    let brand_width = BRAND.chars().count() as u16;

    let layout = Layout::horizontal([
        Constraint::Length(brand_width),
        Constraint::Min(0),
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
            Style::default().fg(accent()).bold().underlined()
        } else {
            Style::default().fg(text_dim())
        };
        spans.push(Span::styled(title.clone(), style));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Left),
        layout[1],
    );

    frame.render_widget(
        Paragraph::new(VERSION)
            .style(Style::default().fg(text_dim()))
            .alignment(Alignment::Right),
        layout[2],
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
        return vec![Span::styled(BRAND, Style::default().fg(base).bold())];
    }

    // A cosine wave crest travels across the brand. `WAVE_PERIOD` is the tick
    // count for one full left-to-right sweep; `MAX_MIX` caps how far each letter
    // leans toward the accent so it pulses instead of strobing. `ramp` eases the
    // depth up from 0 so the shimmer materializes gently rather than snapping on.
    const WAVE_PERIOD: f32 = 16.0;
    const MAX_MIX: f32 = 0.65;
    let depth = MAX_MIX * ramp.clamp(0.0, 1.0);
    let highlight = accent();

    // Normalize the sweep to a 0..1 cycle, then ease-out (`1 - (1 - t)²`) so the
    // crest starts fast and decelerates toward the end of each pass before
    // wrapping — the sweep settles instead of cutting off at a constant speed.
    let raw = (tick as f32 / WAVE_PERIOD).fract();
    let eased = 1.0 - (1.0 - raw) * (1.0 - raw);
    let crest = eased * std::f32::consts::TAU;
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
            Span::styled(ch.to_string(), Style::default().fg(fg).bold())
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/tui_header.rs"]
mod tests;
