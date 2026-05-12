use crate::{
    app::CollectionPage,
    config::constants::{GB, KB, MAX_TRUNCATED_CHARS, MB},
    download::{DownloadStage, DownloadSummary},
    utils::format_bytes,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Gauge, List, ListItem, Paragraph, Wrap},
};

use super::{DownloadView, components};

const INFO_HEIGHT: u16 = 8;
const GAUGE_HEIGHT: u16 = 3;

pub fn render(frame: &mut Frame, area: Rect, view: DownloadView) {
    let page = view.page;
    let show_disk_warning = should_render_disk_warning(page);

    let mut constraints = Vec::with_capacity(4);
    if show_disk_warning {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(INFO_HEIGHT));
    constraints.push(Constraint::Length(GAUGE_HEIGHT));
    constraints.push(Constraint::Min(0));

    let sections = Layout::vertical(constraints).split(area);
    let mut index = 0;
    if show_disk_warning {
        render_disk_warning(frame, sections[index], page);
        index += 1;
    }
    let info_area = sections[index];
    index += 1;
    let gauge_area = sections[index];
    index += 1;
    let threads_area = sections[index];

    render_info(frame, info_area, page);
    render_gauge(frame, gauge_area, page);
    render_threads(frame, threads_area, page);
}

fn should_render_disk_warning(page: &CollectionPage) -> bool {
    page.low_disk_space.is_some()
        && page.stats.downloaded == 0
        && matches!(
            page.stage,
            DownloadStage::Pending
                | DownloadStage::Resolving
                | DownloadStage::Rechecking
                | DownloadStage::Downloading
        )
}

fn render_disk_warning(frame: &mut Frame, area: Rect, page: &CollectionPage) {
    if let Some(available) = page.low_disk_space {
        let text = format!(" low disk space: {} available", format_bytes(available));
        let paragraph = Paragraph::new(text).style(Style::default().fg(components::WARNING));
        frame.render_widget(paragraph, area);
    }
}

fn render_info(frame: &mut Frame, area: Rect, page: &CollectionPage) {
    let status =
        if matches!(page.stage, DownloadStage::Downloading) && page.all_threads_rate_limited() {
            "rate limited"
        } else {
            stage_label(page.stage)
        };
    let speed_display = current_speed(page);
    let bytes_display = if matches!(
        page.stage,
        DownloadStage::Downloading | DownloadStage::Completed
    ) {
        page.stats
            .total_collection_bytes
            .map(|total| format_bytes_progress(page.total_downloaded_bytes(), total))
    } else {
        None
    };

    let key_style = Style::default().fg(components::TEXT_FAINT);
    let value_style = Style::default().fg(components::TEXT_MUTED);
    let mut status_spans = vec![
        Span::styled("status: ", key_style),
        components::status_pill(status, status_color(page.stage, status)),
    ];
    if let Some(speed) = speed_display {
        status_spans.push(Span::styled("  speed ", key_style));
        status_spans.push(Span::styled(
            speed,
            Style::default().fg(components::SUCCESS),
        ));
    }
    if let Some(bytes) = bytes_display {
        status_spans.push(Span::styled("  size ", key_style));
        status_spans.push(Span::styled(
            bytes,
            Style::default().fg(components::WARNING),
        ));
    }

    let lines = vec![
        Line::from(vec![
            Span::styled("collection: ", key_style),
            Span::styled(page.title.clone(), Style::default().fg(components::ACCENT)),
        ]),
        Line::from(vec![
            Span::styled("uploader: ", key_style),
            Span::styled(
                page.uploader.as_deref().unwrap_or("unknown").to_owned(),
                value_style,
            ),
        ]),
        Line::from(vec![
            Span::styled("output: ", key_style),
            Span::styled(
                page.output_dir.as_deref().unwrap_or("preparing").to_owned(),
                value_style,
            ),
        ]),
        Line::from(vec![
            Span::styled("settings: ", key_style),
            Span::styled(
                format!("{} threads", page.thread_statuses.len()),
                Style::default().fg(components::ACCENT),
            ),
            Span::styled("  │  ", Style::default().fg(components::LINE_SOFT)),
            Span::styled(
                "failed maps appear below",
                Style::default().fg(components::TEXT_FAINT),
            ),
        ]),
        Line::from(status_spans),
        Line::from(summary_spans(page)),
    ];

    let paragraph = Paragraph::new(lines)
        .block(components::panel_block("overview"))
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn stage_label(stage: DownloadStage) -> &'static str {
    match stage {
        DownloadStage::Pending => "pending",
        DownloadStage::Resolving => "resolving",
        DownloadStage::Rechecking => "rechecking",
        DownloadStage::Downloading => "downloading",
        DownloadStage::Completed => "completed",
        DownloadStage::Failed => "failed",
    }
}

fn current_speed(page: &CollectionPage) -> Option<String> {
    if !matches!(page.stage, DownloadStage::Downloading) {
        return None;
    }

    let speed = page.cumulative_speed();
    if speed >= 1.0 {
        Some(format_speed(speed))
    } else {
        None
    }
}

fn status_color(stage: DownloadStage, status: &str) -> ratatui::style::Color {
    if status == "rate limited" {
        components::WARNING
    } else {
        components::status_style(stage)
            .fg
            .unwrap_or(components::TEXT_DIM)
    }
}

fn summary_spans(page: &CollectionPage) -> Vec<Span<'static>> {
    let (label, downloaded, skipped, failed, unverified) = if let Some(summary) = &page.summary {
        (
            "done",
            summary.downloaded,
            summary.skipped,
            summary.failed,
            summary.unverified,
        )
    } else {
        (
            "progress",
            page.stats.downloaded,
            page.stats.skipped,
            page.stats.failed,
            page.stats.unverified,
        )
    };
    let displayed_skipped = skipped.saturating_add(unverified);
    let mut spans = vec![
        Span::styled(
            format!("{label}: "),
            Style::default().fg(components::TEXT_FAINT),
        ),
        Span::styled(
            format!("{downloaded} downloaded"),
            Style::default().fg(components::SUCCESS),
        ),
        Span::styled("  │  ", Style::default().fg(components::LINE_SOFT)),
        Span::styled(
            format!("{displayed_skipped} skipped"),
            Style::default().fg(components::TEXT_MUTED),
        ),
        Span::styled("  │  ", Style::default().fg(components::LINE_SOFT)),
        Span::styled(
            format!("{failed} failed"),
            if failed > 0 {
                Style::default().fg(components::DANGER)
            } else {
                Style::default().fg(components::TEXT_MUTED)
            },
        ),
    ];
    if unverified > 0 {
        spans.push(Span::styled(
            "  │  ",
            Style::default().fg(components::LINE_SOFT),
        ));
        spans.push(Span::styled(
            format!("{unverified} unverified"),
            Style::default().fg(components::WARNING),
        ));
    }
    spans
}

fn format_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= MB {
        format!("{:.2} MB/s", bytes_per_sec / MB)
    } else if bytes_per_sec >= KB {
        format!("{:.1} KB/s", bytes_per_sec / KB)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

fn format_bytes_progress(downloaded: u64, total: u64) -> String {
    let downloaded_gb = downloaded as f64 / GB;
    let total_gb = total as f64 / GB;

    format!("{downloaded_gb:.2}/{total_gb:.2} GB")
}

fn render_gauge(frame: &mut Frame, area: Rect, page: &CollectionPage) {
    let queue_remaining = page.download_target;
    let total_collection = page.total_maps.max(1);
    let downloaded = page.stats.downloaded as usize;
    let verified = downloaded + page.stats.skipped as usize;
    let verified_display = verified.min(total_collection);
    let ratio = (verified_display as f64 / total_collection as f64).clamp(0.0, 1.0);

    let mut top_style = Style::default().fg(components::TEXT_DIM);
    if !page.progress_label_style_locked || page.progress_label_bold_when_locked {
        top_style = top_style
            .fg(components::TEXT_MUTED)
            .add_modifier(Modifier::BOLD);
    }

    let downloaded_title = format!(" {downloaded} downloaded  {queue_remaining} queued ");
    let verified_title = format!(" {verified_display}/{total_collection} verified ");

    let block = Block::default()
        .title(Line::from(Span::styled(downloaded_title, top_style)).left_aligned())
        .title_bottom(
            Line::from(Span::styled(
                verified_title,
                Style::default().fg(components::TEXT_FAINT),
            ))
            .right_aligned(),
        );

    let gauge = Gauge::default()
        .block(block)
        .ratio(ratio)
        .label(Span::raw(""))
        .gauge_style(
            Style::default()
                .fg(components::ACCENT)
                .bg(components::BG_RAISED),
        );

    frame.render_widget(gauge, area);
}

fn render_threads(frame: &mut Frame, area: Rect, page: &CollectionPage) {
    if matches!(page.stage, DownloadStage::Completed)
        && let Some(summary) = &page.summary
    {
        render_results_block(frame, area, summary);
        return;
    }

    let mut items: Vec<ListItem> = Vec::new();

    for (index, status) in page.thread_statuses.iter().enumerate() {
        if status.should_display() {
            items.push(components::thread_item(index, status));
        }
    }

    if items.is_empty() && page.failed_maps.is_empty() {
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "no active threads",
                Style::default().fg(components::TEXT_FAINT),
            ),
        ])));
    }

    if matches!(page.stage, DownloadStage::Completed | DownloadStage::Failed)
        && !page.failed_maps.is_empty()
    {
        items.push(components::section_header("failed"));

        for failure in &page.failed_maps {
            items.push(ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("#{}", failure.id),
                    Style::default().fg(components::TEXT_FAINT),
                ),
                Span::styled("  ", Style::default()),
                Span::styled(
                    summarize_failure(&failure.reason),
                    Style::default().fg(components::DANGER),
                ),
            ])));
        }
    }

    let block = components::panel_block("threads");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let visible_height = inner.height as usize;
    let (start, end) =
        components::scroll_window(&items, items.len().saturating_sub(1), visible_height);
    let visible_items = items[start..end].to_vec();

    let list = List::new(visible_items).highlight_symbol("");
    frame.render_widget(list, inner);
}

fn render_results_block(frame: &mut Frame, area: Rect, summary: &DownloadSummary) {
    let displayed_skipped = summary.skipped.saturating_add(summary.unverified);
    let failed_style = if summary.failed > 0 {
        Style::default().fg(components::DANGER)
    } else {
        Style::default().fg(components::TEXT_MUTED)
    };
    let mut spans = vec![
        Span::raw("  "),
        Span::styled("DOWNLOADED", eyebrow_style()),
        Span::raw(" "),
        Span::styled(
            summary.downloaded.to_string(),
            Style::default().fg(components::ACCENT),
        ),
        Span::styled("  │  ", Style::default().fg(components::LINE_SOFT)),
        Span::styled("SKIPPED", eyebrow_style()),
        Span::raw(" "),
        Span::styled(
            displayed_skipped.to_string(),
            Style::default().fg(components::TEXT_MUTED),
        ),
        Span::styled("  │  ", Style::default().fg(components::LINE_SOFT)),
        Span::styled("FAILED", eyebrow_style()),
        Span::raw(" "),
        Span::styled(summary.failed.to_string(), failed_style),
    ];
    if summary.unverified > 0 {
        spans.push(Span::styled(
            "  │  ",
            Style::default().fg(components::LINE_SOFT),
        ));
        spans.push(Span::styled("UNVERIFIED", eyebrow_style()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            summary.unverified.to_string(),
            Style::default().fg(components::WARNING),
        ));
    }

    let paragraph = Paragraph::new(Line::from(spans)).block(components::panel_block("results"));
    frame.render_widget(paragraph, area);
}

fn eyebrow_style() -> Style {
    Style::default()
        .fg(components::TEXT_FAINT)
        .add_modifier(Modifier::BOLD | Modifier::DIM)
}

fn summarize_failure(reason: &str) -> String {
    if reason.is_empty() {
        return "unknown error".to_string();
    }

    let mut truncated: String = reason.chars().take(MAX_TRUNCATED_CHARS).collect();
    if reason.chars().count() > MAX_TRUNCATED_CHARS {
        if truncated.len() >= 3 {
            truncated.truncate(truncated.len().saturating_sub(3));
        }
        truncated.push_str("...");
    }
    truncated
}
