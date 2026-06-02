use crate::{
    app::{ActiveDownloadLine, InputField},
    download::DownloadStage,
};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Padding, Paragraph},
};
use std::sync::OnceLock;
use std::time::Instant;

use super::{
    FILL_BLOCK, FILL_H_LINE, FILL_SHADE, FILL_SPACE, GLYPH_BLOCK, GLYPH_H_LINE, GLYPH_SHADE,
    GLYPH_SPACE, accent, accent_alt, bg, danger, eyebrow, focused_label, glyph_fill, info, line,
    line_soft, success, text_dim, text_faint, text_muted, warning,
};

pub const FOCUS_MARK: &str = "❯ ";
pub const FOCUS_PAD: &str = "  ";
pub const CHECK_ON: &str = "◉";
pub const CHECK_OFF: &str = "○";
pub const EXPANDED: &str = "▾";
pub const COLLAPSED: &str = "▸";
pub const SEPARATOR: &str = "  │  ";

pub struct Metric<'a> {
    pub label: &'a str,
    pub value: String,
    pub style: Style,
}

impl<'a> Metric<'a> {
    pub fn muted(label: &'a str, value: impl Into<String>) -> Self {
        Self::colored(label, value, text_muted())
    }

    pub fn accent(label: &'a str, value: impl Into<String>) -> Self {
        Self::colored(label, value, accent())
    }

    fn colored(label: &'a str, value: impl Into<String>, color: Color) -> Self {
        Self {
            label,
            value: value.into(),
            style: Style::default().fg(color),
        }
    }
}

pub struct FormItems<T> {
    items: Vec<ListItem<'static>>,
    focus: T,
    focused_index: usize,
}

impl<T: Copy + PartialEq> FormItems<T> {
    pub fn new(focus: T) -> Self {
        Self {
            items: Vec::new(),
            focus,
            focused_index: 0,
        }
    }

    pub fn push(&mut self, item: ListItem<'static>) {
        self.items.push(item);
    }

    pub fn push_focusable(&mut self, field: T, item: ListItem<'static>) {
        if self.focus == field {
            self.focused_index = self.items.len();
        }
        self.items.push(item);
    }

    pub fn into_parts(self) -> (Vec<ListItem<'static>>, usize) {
        (self.items, self.focused_index)
    }
}

pub fn scroll_window<T>(
    items: &[T],
    focused_index: usize,
    visible_height: usize,
) -> (usize, usize) {
    if items.is_empty() || visible_height == 0 || items.len() <= visible_height {
        return (0, items.len());
    }

    let focused_index = focused_index.min(items.len().saturating_sub(1));
    let half_visible = visible_height / 2;
    let mut start = focused_index.saturating_sub(half_visible);

    if start + visible_height > items.len() {
        start = items.len().saturating_sub(visible_height);
    }

    (start, (start + visible_height).min(items.len()))
}

/// Returns a muted `Span` showing how many items lie above or below the
/// visible window, e.g. `▲ 5 above · 12 below ▼`.
///
/// Returns `None` when the whole list fits in view (`start == 0 && end ==
/// total`) or the inputs are degenerate (`total == 0`, `start > end`).
pub(crate) fn scroll_indicator(start: usize, end: usize, total: usize) -> Option<Span<'static>> {
    if total == 0 || start > end || end > total {
        return None;
    }
    let above = start;
    let below = total.saturating_sub(end);
    if above == 0 && below == 0 {
        return None;
    }

    let text = match (above > 0, below > 0) {
        (true, true) => format!(" ▲ {above} above · {below} below ▼ "),
        (true, false) => format!(" ▲ {above} above "),
        (false, true) => format!(" {below} below ▼ "),
        (false, false) => unreachable!(),
    };
    Some(Span::styled(text, Style::default().fg(text_faint())))
}

/// Renders a scrollable form panel and returns the absolute caret position when
/// `cursor_col` is `Some` and the focused row is currently visible.
///
/// `cursor_col` is the column offset (within `inner`) of the caret on the
/// focused row — see [`input_cursor_col`]. The caller sets the terminal cursor
/// to the returned position; `None` means no caret should be shown.
pub fn render_scrollable_panel(
    frame: &mut Frame,
    area: Rect,
    title: &'static str,
    items: &[ListItem<'static>],
    focused_index: usize,
    cursor_col: Option<u16>,
) -> Option<(u16, u16)> {
    let block = panel_block(title);
    let inner = block.inner(area);
    let (start, end) = scroll_window(items, focused_index, inner.height as usize);
    let block = match scroll_indicator(start, end, items.len()) {
        Some(span) => block.title_top(Line::from(span).right_aligned()),
        None => block,
    };
    frame.render_widget(block, area);

    frame.render_widget(
        List::new(items[start..end].to_vec()).highlight_symbol(""),
        inner,
    );

    panel_cursor(inner, focused_index, start, end, cursor_col)
}

/// Column offset (within a panel's inner area) of the text caret for a focused
/// [`input_item`]: focus marker + `"label: "` + the caret offset into the value.
///
/// The caret is a char index, so its column is the number of chars to its left
/// (`field.caret()`), not the full value width.
pub fn input_cursor_col(field: &InputField) -> u16 {
    let label_width = field.label.to_lowercase().chars().count();
    // focus marker (2) + label + ": " (2) + caret offset within the value
    (2 + label_width + 2 + field.caret()) as u16
}

/// Maps a focused row + caret column to an absolute terminal position, or `None`
/// when no caret is requested or the row is scrolled out of view. The column is
/// clamped to the last cell of `inner` so a long value never parks the cursor
/// past the panel edge.
pub fn panel_cursor(
    inner: Rect,
    focused_index: usize,
    start: usize,
    end: usize,
    cursor_col: Option<u16>,
) -> Option<(u16, u16)> {
    let col = cursor_col?;
    if inner.width == 0 || inner.height == 0 || focused_index < start || focused_index >= end {
        return None;
    }
    let y = inner.y + (focused_index - start) as u16;
    let max_x = inner.x + inner.width - 1;
    let x = (inner.x + col).min(max_x);
    Some((x, y))
}

/// Callers must pass an already-uppercased, space-padded title constant
/// (e.g. `" OVERVIEW "`). This avoids per-call allocation; use the module-level
/// `PANEL_*` constants defined in each view module.
pub fn panel_block(title: &'static str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(line_soft()))
        .title(Span::styled(
            title,
            Style::default()
                .fg(accent_alt())
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::ITALIC),
        ))
        .title_alignment(Alignment::Left)
        .padding(Padding::new(1, 1, 0, 0))
}

pub fn render_separator(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let line = glyph_fill(&FILL_H_LINE, GLYPH_H_LINE, area.width as usize);
    frame.render_widget(
        Paragraph::new(line.into_owned()).style(Style::default().fg(line_soft())),
        area,
    );
}

pub fn focus_span(focused: bool) -> Span<'static> {
    if focused {
        Span::styled(FOCUS_MARK, Style::default().fg(accent()).bold())
    } else {
        Span::raw(FOCUS_PAD)
    }
}

pub fn check_marker(state: bool) -> (&'static str, Style) {
    if state {
        (CHECK_ON, Style::default().fg(accent()))
    } else {
        (CHECK_OFF, Style::default().fg(text_faint()))
    }
}

pub fn input_item(field: &InputField, focused: bool) -> ListItem<'static> {
    let value = if field.value.is_empty() {
        Span::styled(field.placeholder.clone(), Style::default().fg(text_faint()))
    } else {
        Span::styled(field.value.clone(), Style::default().fg(accent()))
    };

    ListItem::new(Line::from(vec![
        focus_span(focused),
        Span::styled(
            format!("{}: ", field.label.to_lowercase()),
            focused_label(focused),
        ),
        value,
    ]))
}

/// A stepper row showing a numeric value with an optional "recommended: N" chip.
///
/// `recommended` is shown as a dim chip when the current value differs; omitted
/// when `value == recommended` (the field is already at the suggested setting).
pub fn stepper_item(label: &str, value: u8, recommended: u8, focused: bool) -> ListItem<'static> {
    let mut s = String::with_capacity(3);
    s.push_str(&value.to_string());
    let value_span = Span::styled(s, Style::default().fg(accent()));

    let mut spans = vec![
        focus_span(focused),
        Span::styled(format!("{label}: "), focused_label(focused)),
        value_span,
    ];

    if value != recommended {
        let mut chip = String::with_capacity(16);
        chip.push_str("  recommended: ");
        chip.push_str(&recommended.to_string());
        spans.push(Span::styled(chip, Style::default().fg(text_faint())));
    }

    ListItem::new(Line::from(spans))
}

pub fn cycle_item(
    label: &str,
    options: &[&str],
    selected: &str,
    focused: bool,
) -> ListItem<'static> {
    let mut spans = vec![
        focus_span(focused),
        Span::styled(format!("{label}: "), focused_label(focused)),
    ];
    for (index, &option) in options.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", Style::default().fg(line())));
        }
        let style = if option == selected {
            Style::default().fg(accent()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(text_faint())
        };
        spans.push(Span::styled(option.to_string(), style));
    }
    ListItem::new(Line::from(spans))
}

pub fn section_header(label: &str) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            label.to_uppercase(),
            Style::default()
                .fg(accent_alt())
                .add_modifier(Modifier::BOLD),
        ),
    ]))
}

pub fn help_item(text: impl Into<String>) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled("  └ ", Style::default().fg(line())),
        Span::styled(text.into(), Style::default().fg(text_faint())),
    ]))
}

/// Builds a `[focus_span] [icon] [ label] [  detail] [suffix]` row.
///
/// Shared by [`row_item`] and [`disclosure_row`]; each caller supplies its own
/// focus span, icon, label style, and optional detail. The detail (when present)
/// is always rendered in `text_faint`. An optional pre-styled `suffix` span is
/// appended verbatim after the detail (the caller owns its leading spacing).
fn icon_label_row(
    focus: Span<'static>,
    icon: Span<'static>,
    label: &str,
    label_style: Style,
    detail: Option<String>,
    suffix: Option<Span<'static>>,
) -> ListItem<'static> {
    let mut spans = vec![focus, icon, Span::styled(format!(" {label}"), label_style)];
    if let Some(detail) = detail {
        spans.push(Span::styled(
            format!("  {detail}"),
            Style::default().fg(text_faint()),
        ));
    }
    if let Some(suffix) = suffix {
        spans.push(suffix);
    }
    ListItem::new(Line::from(spans))
}

pub fn disclosure_row(
    label: &str,
    detail: impl Into<String>,
    expanded: bool,
    focused: bool,
) -> ListItem<'static> {
    let marker = if expanded { EXPANDED } else { COLLAPSED };
    let label_style = if expanded {
        Style::default().fg(accent()).add_modifier(Modifier::BOLD)
    } else {
        focused_label(focused)
    };
    icon_label_row(
        focus_span(focused && !expanded),
        Span::styled(
            marker,
            Style::default().fg(if expanded { accent() } else { text_faint() }),
        ),
        label,
        label_style,
        Some(detail.into()),
        None,
    )
}

pub fn row_item(
    label: &str,
    detail: Option<&str>,
    state: bool,
    focused: bool,
) -> ListItem<'static> {
    row_item_with_suffix(label, detail, state, focused, None)
}

/// Like [`row_item`] but appends a pre-styled trailing `suffix` span after the
/// detail (e.g. the home tab's per-mirror latency readout). The base row —
/// focus marker, check glyph, label, and detail — is identical to [`row_item`].
pub fn row_item_with_suffix(
    label: &str,
    detail: Option<&str>,
    state: bool,
    focused: bool,
    suffix: Option<Span<'static>>,
) -> ListItem<'static> {
    let (marker, marker_style) = check_marker(state);
    icon_label_row(
        focus_span(focused),
        Span::styled(marker, marker_style),
        label,
        focused_label(focused),
        detail.map(str::to_string),
        suffix,
    )
}

/// A button row rendered as a filled pill, activated with `enter`.
///
/// `enabled` greys the label when the action is currently unavailable (e.g. no
/// maps selected). The button is still rendered so its position stays stable.
pub fn button_item(label: &str, focused: bool, enabled: bool) -> ListItem<'static> {
    let mut pill = String::with_capacity(label.len() + 4);
    pill.push_str("  ");
    pill.push_str(label);
    pill.push_str("  ");

    let style = if !enabled && !focused {
        Style::default().fg(text_faint())
    } else if !enabled {
        // focused but disabled: show dim accent so the row is visibly selected
        Style::default().fg(accent()).add_modifier(Modifier::DIM)
    } else if focused {
        Style::default()
            .fg(bg())
            .bg(accent())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(accent()).add_modifier(Modifier::BOLD)
    };

    ListItem::new(Line::from(vec![
        focus_span(focused),
        Span::styled(pill, style),
    ]))
}

pub fn summary_item(metrics: &[Metric<'_>]) -> ListItem<'static> {
    let mut spans = vec![Span::raw("  ")];
    for (index, metric) in metrics.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(SEPARATOR, Style::default().fg(line_soft())));
        }
        spans.push(Span::styled(metric.label.to_uppercase(), eyebrow()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(metric.value.clone(), metric.style));
    }
    ListItem::new(Line::from(spans))
}

pub fn status_pill(label: impl Into<String>, color: Color) -> Span<'static> {
    let label = label.into();
    let mut out = String::with_capacity(label.len() + 2);
    out.push(' ');
    out.push_str(&label);
    out.push(' ');
    Span::styled(out, Style::default().fg(color).add_modifier(Modifier::BOLD))
}

pub fn spacer() -> ListItem<'static> {
    ListItem::new(Line::from(""))
}

pub fn status_style(stage: DownloadStage) -> Style {
    match stage {
        DownloadStage::Pending | DownloadStage::Resolving | DownloadStage::Rechecking => {
            Style::default().fg(warning())
        }
        DownloadStage::Downloading => Style::default().fg(info()),
        DownloadStage::Completed => Style::default().fg(success()),
        DownloadStage::Failed => Style::default().fg(danger()),
    }
}

pub fn active_download_item(line: &ActiveDownloadLine, width: u16) -> ListItem<'static> {
    active_download_item_msg(line, &line.displayed_message(), width)
}

/// Like [`active_download_item`] but accepts an explicit message string.
///
/// Used by the rate-limited renderer to splice a countdown suffix into the
/// message before truncation without duplicating the progress-bar layout logic.
pub fn active_download_item_msg(
    line: &ActiveDownloadLine,
    message_text: &str,
    width: u16,
) -> ListItem<'static> {
    const BAR_WIDTH: u16 = 12;
    const LABEL_WIDTH: u16 = 5;
    const GAP: u16 = 1;
    const RESERVED_RIGHT: u16 = BAR_WIDTH + GAP + LABEL_WIDTH;

    let prefix = {
        let id_s = line.beatmapset_id.to_string();
        let pad = 7usize.saturating_sub(id_s.len());
        let mut s = String::with_capacity(3 + 7 + 1);
        s.push_str("  #");
        s.push_str(&id_s);
        for _ in 0..pad {
            s.push(' ');
        }
        s.push(' ');
        s
    };
    let prefix_w = prefix.len() as u16;
    let rate_limited = line.displayed_rate_limited();
    let bar_color = line.bar_color();

    let message_budget = width
        .saturating_sub(prefix_w)
        .saturating_sub(RESERVED_RIGHT)
        .saturating_sub(GAP);
    let (message, message_w) = truncate_to_width(message_text, message_budget);

    let mut spans = vec![
        Span::styled(prefix, Style::default().fg(text_faint())),
        Span::styled(message, message_style(line.stage, rate_limited)),
    ];

    let used = prefix_w.saturating_add(message_w);
    let pad = width.saturating_sub(used).saturating_sub(RESERVED_RIGHT) as usize;
    spans.push(Span::raw(
        glyph_fill(&FILL_SPACE, GLYPH_SPACE, pad).into_owned(),
    ));

    match line.progress_ratio() {
        Some(ratio) => {
            let filled = ((ratio * BAR_WIDTH as f32).round() as u16).min(BAR_WIDTH);
            let empty = BAR_WIDTH - filled;
            spans.push(Span::styled(
                glyph_fill(&FILL_BLOCK, GLYPH_BLOCK, filled as usize).into_owned(),
                Style::default().fg(bar_color),
            ));
            spans.push(Span::styled(
                glyph_fill(&FILL_SHADE, GLYPH_SHADE, empty as usize).into_owned(),
                Style::default().fg(line_soft()),
            ));
            let pct = (ratio * 100.0).round() as u16;
            spans.push(Span::styled(
                pct_label(pct),
                Style::default().fg(text_faint()),
            ));
        }
        None if matches!(line.stage, crate::download::BeatmapStage::Downloading) => {
            spans.extend(indeterminate_bar_spans(BAR_WIDTH, bar_color));
            spans.push(Span::styled("  ...", Style::default().fg(text_faint())));
        }
        None => {
            spans.push(Span::styled(
                glyph_fill(&FILL_SHADE, GLYPH_SHADE, BAR_WIDTH as usize).into_owned(),
                Style::default().fg(line_soft()),
            ));
            spans.push(Span::styled("     ", Style::default().fg(text_faint())));
        }
    }

    ListItem::new(Line::from(spans))
}

fn indeterminate_bar_spans(width: u16, bar_color: Color) -> Vec<Span<'static>> {
    let width = width as usize;
    let segment = 4usize.min(width);
    let travel = width.saturating_sub(segment);
    let tick = animation_start().elapsed().as_millis() as usize / 90;
    let cycle = travel.saturating_mul(2).max(1);
    let phase = tick % cycle;
    let offset = if phase <= travel {
        phase
    } else {
        cycle.saturating_sub(phase)
    };

    let mut spans = Vec::new();
    if offset > 0 {
        spans.push(Span::styled(
            glyph_fill(&FILL_SHADE, GLYPH_SHADE, offset).into_owned(),
            Style::default().fg(line_soft()),
        ));
    }
    spans.push(Span::styled(
        glyph_fill(&FILL_BLOCK, GLYPH_BLOCK, segment).into_owned(),
        Style::default().fg(bar_color),
    ));
    let right = width.saturating_sub(offset).saturating_sub(segment);
    if right > 0 {
        spans.push(Span::styled(
            glyph_fill(&FILL_SHADE, GLYPH_SHADE, right).into_owned(),
            Style::default().fg(line_soft()),
        ));
    }
    spans
}

fn animation_start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

pub fn truncate_to_width(message: &str, budget: u16) -> (String, u16) {
    use unicode_width::UnicodeWidthChar as _;
    use unicode_width::UnicodeWidthStr as _;

    let budget = budget as usize;
    if budget == 0 {
        return (String::new(), 0);
    }
    let display_width = message.width();
    if display_width <= budget {
        return (message.to_string(), display_width as u16);
    }
    if budget == 1 {
        return ("…".to_string(), 1);
    }
    // Reserve 1 column for the ellipsis; accumulate chars until we'd overflow.
    let target = budget.saturating_sub(1);
    let mut out = String::with_capacity(message.len());
    let mut used = 0usize;
    for ch in message.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > target {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    (out, budget as u16)
}

// 101-entry table of " {:>3}%" strings for pct in 0..=100.
// Returned by `pct_label` to avoid per-frame allocation in `active_download_item`.
const PCT_LABELS: [&str; 101] = [
    "   0%", "   1%", "   2%", "   3%", "   4%", "   5%", "   6%", "   7%", "   8%", "   9%",
    "  10%", "  11%", "  12%", "  13%", "  14%", "  15%", "  16%", "  17%", "  18%", "  19%",
    "  20%", "  21%", "  22%", "  23%", "  24%", "  25%", "  26%", "  27%", "  28%", "  29%",
    "  30%", "  31%", "  32%", "  33%", "  34%", "  35%", "  36%", "  37%", "  38%", "  39%",
    "  40%", "  41%", "  42%", "  43%", "  44%", "  45%", "  46%", "  47%", "  48%", "  49%",
    "  50%", "  51%", "  52%", "  53%", "  54%", "  55%", "  56%", "  57%", "  58%", "  59%",
    "  60%", "  61%", "  62%", "  63%", "  64%", "  65%", "  66%", "  67%", "  68%", "  69%",
    "  70%", "  71%", "  72%", "  73%", "  74%", "  75%", "  76%", "  77%", "  78%", "  79%",
    "  80%", "  81%", "  82%", "  83%", "  84%", "  85%", "  86%", "  87%", "  88%", "  89%",
    "  90%", "  91%", "  92%", "  93%", "  94%", "  95%", "  96%", "  97%", "  98%", "  99%",
    " 100%",
];

fn pct_label(pct: u16) -> &'static str {
    PCT_LABELS[pct.min(100) as usize]
}

fn message_style(stage: crate::download::BeatmapStage, rate_limited: bool) -> Style {
    use crate::download::BeatmapStage;
    if rate_limited {
        return Style::default().fg(warning());
    }
    match stage {
        BeatmapStage::Failed | BeatmapStage::Aborted => Style::default().fg(danger()),
        BeatmapStage::Success => Style::default().fg(success()),
        BeatmapStage::Skipped => Style::default().fg(text_faint()),
        BeatmapStage::Pending | BeatmapStage::Downloading | BeatmapStage::Verifying => {
            Style::default().fg(text_dim())
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tui_widgets.rs"]
mod tests;
