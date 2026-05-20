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
    ACCENT, ACCENT_ALT, DANGER, FILL_BLOCK, FILL_H_LINE, FILL_SHADE, FILL_SPACE, GLYPH_BLOCK,
    GLYPH_H_LINE, GLYPH_SHADE, GLYPH_SPACE, INFO, LINE, LINE_SOFT, SUCCESS, TEXT_DIM, TEXT_FAINT,
    TEXT_MUTED, WARNING, eyebrow, focused_label, glyph_fill,
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

#[allow(dead_code)]
impl<'a> Metric<'a> {
    pub fn muted(label: &'a str, value: impl Into<String>) -> Self {
        Self::colored(label, value, TEXT_MUTED)
    }

    pub fn accent(label: &'a str, value: impl Into<String>) -> Self {
        Self::colored(label, value, ACCENT)
    }

    pub fn success(label: &'a str, value: impl Into<String>) -> Self {
        Self::colored(label, value, SUCCESS)
    }

    pub fn danger(label: &'a str, value: impl Into<String>) -> Self {
        Self::colored(label, value, DANGER)
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

pub fn render_scrollable_panel(
    frame: &mut Frame,
    area: Rect,
    title: &'static str,
    items: &[ListItem<'static>],
    focused_index: usize,
) {
    let block = panel_block(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (start, end) = scroll_window(items, focused_index, inner.height as usize);
    frame.render_widget(
        List::new(items[start..end].to_vec()).highlight_symbol(""),
        inner,
    );
}

/// Callers must pass an already-uppercased, space-padded title constant
/// (e.g. `" OVERVIEW "`). This avoids per-call allocation; use the module-level
/// `PANEL_*` constants defined in each view module.
pub fn panel_block(title: &'static str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(LINE_SOFT))
        .title(Span::styled(
            title,
            Style::default()
                .fg(ACCENT_ALT)
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
        Paragraph::new(line.into_owned()).style(Style::default().fg(LINE_SOFT)),
        area,
    );
}

pub fn focus_span(focused: bool) -> Span<'static> {
    if focused {
        Span::styled(FOCUS_MARK, Style::default().fg(ACCENT).bold())
    } else {
        Span::raw(FOCUS_PAD)
    }
}

pub fn check_marker(state: bool) -> (&'static str, Style) {
    if state {
        (CHECK_ON, Style::default().fg(ACCENT))
    } else {
        (CHECK_OFF, Style::default().fg(TEXT_FAINT))
    }
}

pub fn input_item(field: &InputField, focused: bool) -> ListItem<'static> {
    let value = if field.value.is_empty() {
        Span::styled(field.placeholder.clone(), Style::default().fg(TEXT_FAINT))
    } else {
        Span::styled(field.value.clone(), Style::default().fg(ACCENT))
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
            spans.push(Span::styled("  ", Style::default().fg(LINE)));
        }
        let style = if option == selected {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_FAINT)
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
            Style::default().fg(ACCENT_ALT).add_modifier(Modifier::BOLD),
        ),
    ]))
}

pub fn help_item(text: impl Into<String>) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled("  └ ", Style::default().fg(LINE)),
        Span::styled(text.into(), Style::default().fg(TEXT_FAINT)),
    ]))
}

pub fn disclosure_row(
    label: &str,
    detail: impl Into<String>,
    expanded: bool,
    focused: bool,
) -> ListItem<'static> {
    let marker = if expanded { EXPANDED } else { COLLAPSED };
    let label_style = if expanded {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        focused_label(focused)
    };
    ListItem::new(Line::from(vec![
        focus_span(focused && !expanded),
        Span::styled(
            marker,
            Style::default().fg(if expanded { ACCENT } else { TEXT_FAINT }),
        ),
        Span::styled(format!(" {label}"), label_style),
        Span::styled(
            format!("  {}", detail.into()),
            Style::default().fg(TEXT_FAINT),
        ),
    ]))
}

pub fn row_item(
    label: &str,
    detail: Option<&str>,
    state: bool,
    focused: bool,
) -> ListItem<'static> {
    let (marker, marker_style) = check_marker(state);
    let mut spans = vec![
        focus_span(focused),
        Span::styled(marker, marker_style),
        Span::styled(format!(" {label}"), focused_label(focused)),
    ];
    if let Some(detail) = detail {
        spans.push(Span::styled(
            format!("  {detail}"),
            Style::default().fg(TEXT_FAINT),
        ));
    }
    ListItem::new(Line::from(spans))
}

pub fn summary_item(metrics: &[Metric<'_>]) -> ListItem<'static> {
    let mut spans = vec![Span::raw("  ")];
    for (index, metric) in metrics.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(SEPARATOR, Style::default().fg(LINE_SOFT)));
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
            Style::default().fg(WARNING)
        }
        DownloadStage::Downloading => Style::default().fg(INFO),
        DownloadStage::Completed => Style::default().fg(SUCCESS),
        DownloadStage::Failed => Style::default().fg(DANGER),
    }
}

pub fn active_download_item(line: &ActiveDownloadLine, width: u16) -> ListItem<'static> {
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
    let (message, message_w) = truncate_to_width(&line.displayed_message(), message_budget);

    let mut spans = vec![
        Span::styled(prefix, Style::default().fg(TEXT_FAINT)),
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
                Style::default().fg(LINE_SOFT),
            ));
            let pct = (ratio * 100.0).round() as u16;
            spans.push(Span::styled(
                pct_label(pct),
                Style::default().fg(TEXT_FAINT),
            ));
        }
        None if matches!(line.stage, crate::download::BeatmapStage::Downloading) => {
            spans.extend(indeterminate_bar_spans(BAR_WIDTH, bar_color));
            spans.push(Span::styled("  ...", Style::default().fg(TEXT_FAINT)));
        }
        None => {
            spans.push(Span::styled(
                glyph_fill(&FILL_SHADE, GLYPH_SHADE, BAR_WIDTH as usize).into_owned(),
                Style::default().fg(LINE_SOFT),
            ));
            spans.push(Span::styled("     ", Style::default().fg(TEXT_FAINT)));
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
            Style::default().fg(LINE_SOFT),
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
            Style::default().fg(LINE_SOFT),
        ));
    }
    spans
}

fn animation_start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

pub fn truncate_to_width(message: &str, budget: u16) -> (String, u16) {
    let budget = budget as usize;
    if budget == 0 {
        return (String::new(), 0);
    }
    let char_count = message.chars().count();
    if char_count <= budget {
        return (message.to_string(), char_count as u16);
    }
    if budget == 1 {
        return ("…".to_string(), 1);
    }
    let mut out: String = message.chars().take(budget.saturating_sub(1)).collect();
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
        return Style::default().fg(WARNING);
    }
    match stage {
        BeatmapStage::Failed | BeatmapStage::Aborted => Style::default().fg(DANGER),
        BeatmapStage::Success => Style::default().fg(SUCCESS),
        BeatmapStage::Skipped => Style::default().fg(TEXT_FAINT),
        BeatmapStage::Pending | BeatmapStage::Downloading | BeatmapStage::Verifying => {
            Style::default().fg(TEXT_DIM)
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tui_widgets.rs"]
mod tests;
