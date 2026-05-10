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

// cloudy-ui palette — Catppuccin Mocha
pub const ACCENT: Color = Color::Rgb(67, 171, 229);
pub const ACCENT_ALT: Color = Color::Rgb(217, 119, 87);
pub const INFO: Color = Color::Rgb(116, 199, 236);
pub const SUCCESS: Color = Color::Rgb(166, 227, 161);
pub const WARNING: Color = Color::Rgb(249, 226, 175);
pub const DANGER: Color = Color::Rgb(243, 139, 168);
pub const TEXT: Color = Color::Rgb(205, 214, 244);
pub const TEXT_MUTED: Color = Color::Rgb(186, 194, 222);
pub const TEXT_DIM: Color = Color::Rgb(166, 173, 200);
pub const TEXT_FAINT: Color = Color::Rgb(127, 132, 156);
pub const LINE: Color = Color::Rgb(69, 71, 90);
pub const LINE_SOFT: Color = Color::Rgb(38, 38, 58);
pub const BG_RAISED: Color = Color::Rgb(24, 24, 37);
pub const BG_SUNKEN: Color = Color::Rgb(17, 17, 27);

pub const FOCUS_MARK: &str = "▎ ";
pub const FOCUS_PAD: &str = "  ";
pub const CHECK_ON: &str = "◉";
pub const CHECK_OFF: &str = "○";

fn eyebrow_style() -> Style {
    Style::default().fg(TEXT_FAINT)
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
        .borders(Borders::TOP)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(LINE_SOFT))
        .title(Span::styled(
            format!(" {} ", title.to_uppercase()),
            eyebrow_style(),
        ))
        .title_alignment(Alignment::Left)
        .padding(Padding::new(1, 1, 1, 0))
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

    let version = format!("v{} ", env!("CARGO_PKG_VERSION"));
    let version_w = version.chars().count() as u16;
    let brand_text = " osu-collect";
    let brand_w = brand_text.chars().count() as u16;

    let layout = Layout::horizontal([
        Constraint::Length(brand_w + 1),
        Constraint::Min(0),
        Constraint::Length(version_w),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            brand_text,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))),
        layout[0],
    );

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(tabs.titles().len() * 2);
    for (i, title) in tabs.titles().iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default().fg(LINE)));
        }
        let style = if i == tabs.active() {
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_FAINT)
        };
        spans.push(Span::styled(title.clone(), style));
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

pub fn input_item(field: &InputField, focused: bool) -> ListItem<'static> {
    let value = if field.value.is_empty() {
        Span::styled(field.placeholder.clone(), Style::default().fg(TEXT_FAINT))
    } else {
        Span::styled(field.value.clone(), Style::default().fg(ACCENT))
    };

    let row_style = if focused {
        Style::default().fg(TEXT)
    } else {
        Style::default()
    };

    let spans = vec![
        focus_span(focused),
        Span::styled(format!("{}: ", field.label), Style::default().fg(TEXT_DIM)),
        value,
    ];

    ListItem::new(Line::from(spans)).style(row_style)
}

pub fn toggle_item(label: &str, state: bool, focused: bool) -> ListItem<'static> {
    let (marker, marker_style) = check_marker(state);
    let label_style = if focused {
        Style::default().fg(TEXT)
    } else {
        Style::default().fg(TEXT_MUTED)
    };
    let spans = vec![
        focus_span(focused),
        Span::styled(marker, marker_style),
        Span::styled(format!(" {label}"), label_style),
    ];
    ListItem::new(Line::from(spans))
}

pub fn cycle_item(
    label: &str,
    options: &[&str],
    selected: &str,
    focused: bool,
) -> ListItem<'static> {
    let label_style = if focused {
        Style::default().fg(TEXT)
    } else {
        Style::default().fg(TEXT_DIM)
    };
    let mut spans = vec![
        focus_span(focused),
        Span::styled(format!("{label}: "), label_style),
    ];
    for (i, &opt) in options.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default().fg(LINE)));
        }
        let style = if opt == selected {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_FAINT)
        };
        spans.push(Span::styled(opt.to_string(), style));
    }
    ListItem::new(Line::from(spans))
}

pub fn section_header(label: &str) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(label.to_uppercase(), eyebrow_style()),
    ]))
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
    let prefix = Span::styled(format!("t{}: ", index + 1), Style::default().fg(TEXT_FAINT));
    let line = Line::from(vec![
        prefix,
        Span::styled(status.message.clone(), thread_style(status)),
    ]);
    ListItem::new(line)
}

fn thread_style(status: &ThreadStatusLine) -> Style {
    if status.rate_limited {
        return Style::default().fg(WARNING);
    }

    if status.message.to_lowercase().contains("error") || status.message.starts_with("Failed") {
        return Style::default().fg(DANGER);
    }

    if status.message.starts_with("Done") {
        return Style::default().fg(SUCCESS);
    }

    if status.message.starts_with("Skipped") {
        return Style::default().fg(TEXT_FAINT);
    }

    Style::default().fg(TEXT_DIM)
}
