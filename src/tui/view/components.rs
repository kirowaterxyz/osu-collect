use crate::{
    app::{InputField, ThreadStatusLine},
    download::DownloadStage,
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, ListItem, Padding, Paragraph},
};

use super::TabsView;

// cloudy-ui catppuccin mocha palette
pub const ACCENT: Color = Color::Rgb(67, 171, 229); // sapphire-cyan primary
pub const ACCENT_ALT: Color = Color::Rgb(217, 119, 87); // claude orange (secondary)
pub const INFO: Color = Color::Rgb(116, 199, 236);
pub const SUCCESS: Color = Color::Rgb(166, 227, 161);
pub const WARNING: Color = Color::Rgb(249, 226, 175);
pub const DANGER: Color = Color::Rgb(243, 139, 168);
pub const TEXT: Color = Color::Rgb(205, 214, 244);
pub const TEXT_MUTED: Color = Color::Rgb(186, 194, 222);
pub const TEXT_DIM: Color = Color::Rgb(166, 173, 200);
pub const TEXT_FAINT: Color = Color::Rgb(127, 132, 156);
pub const LINE: Color = Color::Rgb(69, 71, 90);
pub const LINE_SOFT: Color = Color::Rgb(49, 50, 68);
pub const BG: Color = Color::Rgb(30, 30, 46);
pub const BG_RAISED: Color = Color::Rgb(24, 24, 37);
#[allow(dead_code)]
pub const BG_SUNKEN: Color = Color::Rgb(17, 17, 27);

pub const FOCUS_MARK: &str = "▎ ";
pub const FOCUS_PAD: &str = "  ";
pub const CHECK_ON: &str = "◉";
pub const CHECK_OFF: &str = "○";
pub const EXPANDED: &str = "▾";
pub const COLLAPSED: &str = "▸";

pub struct Metric<'a> {
    pub label: &'a str,
    pub value: String,
    pub style: Style,
}

#[allow(dead_code)]
impl<'a> Metric<'a> {
    pub fn muted(label: &'a str, value: impl Into<String>) -> Self {
        Self {
            label,
            value: value.into(),
            style: Style::default().fg(TEXT_MUTED),
        }
    }

    pub fn accent(label: &'a str, value: impl Into<String>) -> Self {
        Self {
            label,
            value: value.into(),
            style: Style::default().fg(ACCENT),
        }
    }

    pub fn success(label: &'a str, value: impl Into<String>) -> Self {
        Self {
            label,
            value: value.into(),
            style: Style::default().fg(SUCCESS),
        }
    }

    pub fn danger(label: &'a str, value: impl Into<String>) -> Self {
        Self {
            label,
            value: value.into(),
            style: Style::default().fg(DANGER),
        }
    }
}

pub fn eyebrow_style() -> Style {
    Style::default().fg(TEXT_FAINT).add_modifier(Modifier::BOLD)
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

pub fn panel_block(title: &'static str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(LINE_SOFT))
        .title(Span::styled(
            format!(" {} ", title.to_uppercase()),
            Style::default().fg(ACCENT_ALT).add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Left)
        .padding(Padding::new(1, 1, 0, 0))
}

pub fn render_separator(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let line: String = "─".repeat(area.width as usize);
    let paragraph = Paragraph::new(line).style(Style::default().fg(LINE_SOFT));
    frame.render_widget(paragraph, area);
}

pub fn render_header(frame: &mut Frame, area: Rect, tabs: &TabsView) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let version = format!(" v{} ", env!("CARGO_PKG_VERSION"));
    let version_width = version.chars().count() as u16;
    let brand = " osu-collect ";
    let brand_width = brand.chars().count() as u16;

    let layout = Layout::horizontal([
        Constraint::Length(brand_width),
        Constraint::Min(0),
        Constraint::Length(version_width),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            brand,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))),
        layout[0],
    );

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(tabs.titles().len() * 3);
    for (index, title) in tabs.titles().iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  │  ", Style::default().fg(LINE_SOFT)));
        }
        let title = title.to_lowercase();
        if index == tabs.active() {
            spans.push(Span::styled(
                title,
                Style::default().fg(ACCENT_ALT).add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(title, Style::default().fg(TEXT_FAINT)));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Left),
        layout[1],
    );

    frame.render_widget(
        Paragraph::new(version)
            .style(Style::default().fg(TEXT_FAINT))
            .alignment(Alignment::Right),
        layout[2],
    );
}

/// Renders a scroll position indicator in the top-right corner of `area`.
pub fn render_scroll_indicator(frame: &mut Frame, area: Rect, start: usize, total: usize) {
    if total == 0 || area.width < 6 {
        return;
    }
    let text = format!(" {}/{} ", start + 1, total);
    let indicator_width = text.len() as u16;
    if indicator_width >= area.width {
        return;
    }
    let indicator_area = Rect {
        x: area.x + area.width - indicator_width,
        y: area.y,
        width: indicator_width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(TEXT_FAINT)),
        indicator_area,
    );
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
            field_label_style(focused),
        ),
        value,
    ]))
}

pub fn toggle_item(label: &str, state: bool, focused: bool) -> ListItem<'static> {
    row_item(label, None, state, focused)
}

pub fn cycle_item(
    label: &str,
    options: &[&str],
    selected: &str,
    focused: bool,
) -> ListItem<'static> {
    let mut spans = vec![
        focus_span(focused),
        Span::styled(format!("{label}: "), field_label_style(focused)),
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
        Span::raw("  "),
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
    let label_style = if focused || expanded {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_MUTED)
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
    let label_style = if focused {
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_MUTED)
    };
    let mut spans = vec![
        focus_span(focused),
        Span::styled(marker, marker_style),
        Span::styled(format!(" {label}"), label_style),
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
            spans.push(Span::styled("  │  ", Style::default().fg(LINE_SOFT)));
        }
        spans.push(Span::styled(metric.label.to_uppercase(), eyebrow_style()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(metric.value.clone(), metric.style));
    }
    ListItem::new(Line::from(spans))
}

pub fn status_pill(label: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(
        format!(" {} ", label.into()),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}

pub fn spacer() -> ListItem<'static> {
    ListItem::new(Line::from(""))
}

pub fn focus_span(focused: bool) -> Span<'static> {
    if focused {
        Span::styled(FOCUS_MARK, Style::default().fg(ACCENT))
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

pub fn thread_item(index: usize, status: &ThreadStatusLine) -> ListItem<'static> {
    let line = Line::from(vec![
        Span::styled(
            format!("  t{:<2} ", index + 1),
            Style::default().fg(TEXT_FAINT),
        ),
        Span::styled(status.message.clone(), thread_style(status)),
    ]);
    ListItem::new(line)
}

fn field_label_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(TEXT_MUTED).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_DIM)
    }
}

fn thread_style(status: &ThreadStatusLine) -> Style {
    if status.rate_limited {
        return Style::default().fg(WARNING);
    }

    let message = status.message.to_lowercase();
    if message.contains("error") || message.starts_with("failed") {
        return Style::default().fg(DANGER);
    }

    if message.starts_with("done") {
        return Style::default().fg(SUCCESS);
    }

    if message.starts_with("skipped") {
        return Style::default().fg(TEXT_FAINT);
    }

    Style::default().fg(TEXT_DIM)
}
