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

const INFO_HEIGHT: u16 = 7;
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
        let text = format!(
            " not enough free space in target directory ({} available). free up space before downloading.",
            format_bytes(available)
        );
        let paragraph = Paragraph::new(text).style(Style::default().fg(components::WARNING));
        frame.render_widget(paragraph, area);
    }
}

fn render_info(frame: &mut Frame, area: Rect, page: &CollectionPage) {
    let stage_label = match page.stage {
        DownloadStage::Pending => "pending",
        DownloadStage::Resolving => "resolving",
        DownloadStage::Rechecking => "rechecking existing maps",
        DownloadStage::Downloading => "downloading",
        DownloadStage::Completed => "completed",
        DownloadStage::Failed => "failed",
    };
    let status =
        if matches!(page.stage, DownloadStage::Downloading) && page.all_threads_rate_limited() {
            "rate limited"
        } else {
            stage_label
        };

    let speed_display = if matches!(page.stage, DownloadStage::Downloading) {
        let speed = page.cumulative_speed();
        if speed >= 1.0 {
            Some(format_speed(speed))
        } else {
            None
        }
    } else {
        None
    };

    let counts_line = |label: &str, downloaded: u32, skipped: u32, failed: u32, unverified: u32| {
        let displayed_skipped = skipped.saturating_add(unverified);
        let mut parts = vec![
            format!("{downloaded} downloaded"),
            format!("{displayed_skipped} skipped"),
            format!("{failed} failed"),
        ];
        if unverified > 0 {
            parts.push(format!("{unverified} unverified"));
        }
        format!(" {label}: {}", parts.join(" · "))
    };

    let summary_line = if let Some(summary) = &page.summary {
        counts_line(
            "done",
            summary.downloaded,
            summary.skipped,
            summary.failed,
            summary.unverified,
        )
    } else {
        counts_line(
            "progress",
            page.stats.downloaded,
            page.stats.skipped,
            page.stats.failed,
            page.stats.unverified,
        )
    };

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

    let label_style = Style::default().fg(components::TEXT_DIM);

    let mut status_spans = vec![
        Span::styled("status:     ", label_style),
        Span::styled(status, components::status_style(page.stage)),
    ];
    if let Some(speed) = speed_display {
        status_spans.push(Span::styled(" @ ", label_style));
        status_spans.push(Span::styled(
            speed,
            Style::default().fg(components::SUCCESS),
        ));
    }
    if let Some(bytes) = bytes_display {
        status_spans.push(Span::styled(" (", label_style));
        status_spans.push(Span::styled(
            bytes,
            Style::default().fg(components::WARNING),
        ));
        status_spans.push(Span::styled(")", label_style));
    }

    let lines = vec![
        Line::from(vec![
            Span::styled("collection: ", label_style),
            Span::styled(page.title.clone(), Style::default().fg(components::ACCENT)),
        ]),
        Line::from(vec![
            Span::styled("uploader:   ", label_style),
            Span::styled(
                page.uploader.as_deref().unwrap_or("unknown").to_owned(),
                Style::default().fg(components::TEXT_MUTED),
            ),
        ]),
        Line::from(vec![
            Span::styled("output:     ", label_style),
            Span::styled(
                page.output_dir
                    .as_deref()
                    .unwrap_or("preparing...")
                    .to_owned(),
                Style::default().fg(components::TEXT_MUTED),
            ),
        ]),
        Line::from(status_spans),
        Line::from(Span::styled(
            summary_line,
            Style::default().fg(components::TEXT_MUTED),
        )),
    ];

    let paragraph = Paragraph::new(lines)
        .block(components::panel_block("overview"))
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
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

    format!("{:.2}/{:.2} GB", downloaded_gb, total_gb)
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
        top_style = top_style.add_modifier(Modifier::BOLD);
    }

    let downloaded_title = format!(" {downloaded} downloaded · {queue_remaining} in queue ");
    let verified_title = format!(" {verified_display}/{total_collection} maps verified ");

    let block = Block::default()
        .title(Line::from(Span::styled(downloaded_title, top_style)).left_aligned())
        .title_bottom(
            Line::from(Span::styled(
                verified_title,
                Style::default().fg(components::TEXT_DIM),
            ))
            .right_aligned(),
        );

    let gauge = Gauge::default()
        .block(block)
        .ratio(ratio)
        .label(Span::raw(""))
        .gauge_style(Style::default().fg(components::ACCENT).bg(components::LINE));

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

    for (idx, status) in page.thread_statuses.iter().enumerate() {
        if status.should_display() {
            items.push(components::thread_item(idx, status));
        }
    }

    if items.is_empty() && page.failed_maps.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "no active threads",
            Style::default().fg(components::TEXT_FAINT),
        ))));
    }

    if matches!(page.stage, DownloadStage::Completed | DownloadStage::Failed)
        && !page.failed_maps.is_empty()
    {
        let header = format!("failed maps ({})", page.failed_maps.len());
        items.push(ListItem::new(Line::from(Span::styled(
            header,
            Style::default()
                .fg(components::DANGER)
                .add_modifier(Modifier::BOLD),
        ))));

        for failure in &page.failed_maps {
            let reason = summarize_failure(&failure.reason);
            items.push(ListItem::new(Line::from(Span::styled(
                format!("  #{} - {}", failure.id, reason),
                Style::default().fg(components::DANGER),
            ))));
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
    let label_style = Style::default().fg(components::TEXT_DIM);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("downloaded: ", label_style),
            Span::styled(
                summary.downloaded.to_string(),
                Style::default().fg(components::SUCCESS),
            ),
        ]),
        Line::from(vec![
            Span::styled("skipped:    ", label_style),
            Span::styled(
                displayed_skipped.to_string(),
                Style::default().fg(components::TEXT_MUTED),
            ),
        ]),
        Line::from(vec![
            Span::styled("failed:     ", label_style),
            Span::styled(
                summary.failed.to_string(),
                if summary.failed > 0 {
                    Style::default().fg(components::DANGER)
                } else {
                    Style::default().fg(components::TEXT_MUTED)
                },
            ),
        ]),
    ];

    if summary.unverified > 0 {
        lines.push(Line::from(vec![
            Span::styled("unverified: ", label_style),
            Span::styled(
                summary.unverified.to_string(),
                Style::default()
                    .fg(components::WARNING)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    let paragraph = Paragraph::new(lines).block(components::panel_block("results"));
    frame.render_widget(paragraph, area);
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
