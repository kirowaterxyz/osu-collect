use crate::{
    app::{CollectionPage, collection::FailureReason},
    config::constants::status::RATE_LIMITED,
    download::{DownloadStage, DownloadSummary},
    utils::{format_bytes, pretty_path},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Gauge, List, ListItem, Paragraph, Wrap},
};

use super::widgets::{self, SEPARATOR};
use super::{
    FILL_BLOCK, FILL_SHADE, GLYPH_BLOCK, GLYPH_SHADE, accent, bg_raised, danger, eyebrow,
    glyph_fill, info, line_soft, spinner_char, success, text_dim, text_faint, text_muted, warning,
};

const INFO_HEIGHT: u16 = 8;
const GAUGE_HEIGHT: u16 = 3;

const PANEL_OVERVIEW: &str = " OVERVIEW ";
const PANEL_ACTIVE: &str = " ACTIVE ";
const PANEL_RESULTS: &str = " RESULTS ";

const KEY_COLLECTION: &str = "collection: ";
const KEY_UPLOADER: &str = "uploader: ";
const KEY_OUTPUT: &str = "output: ";
const KEY_SETTINGS: &str = "settings: ";
const KEY_STATUS: &str = "status: ";
const KEY_SPEED: &str = "  speed ";
const KEY_SIZE: &str = "  size ";

const VALUE_UNKNOWN: &str = "unknown";
const VALUE_PREPARING: &str = "preparing";
const VALUE_THREADS_SUFFIX: &str = "threads";

const STATUS_PENDING: &str = "pending";
const STATUS_RESOLVING: &str = "resolving";
const STATUS_RECHECKING: &str = "rechecking";
const STATUS_DOWNLOADING: &str = "downloading";
const STATUS_COMPLETED: &str = "completed";
const STATUS_FAILED: &str = "failed";

const SUMMARY_DONE: &str = "done";
const SUMMARY_PROGRESS: &str = "progress";

const ACTIVE_VERIFYING: &str = "verifying existing archives...";
const ACTIVE_FETCHING: &str = "fetching collection metadata...";
const ACTIVE_NONE: &str = "no active threads";
const PLACEHOLDER_PREPARING: &str = "preparing";
const PLACEHOLDER_RESOLVING: &str = "resolving collection";

const FAILED_SECTION_LABEL: &str = "FAILED";
const RATE_LIMITED_SECTION_LABEL: &str = "─── rate-limited ───";
const DONE_LABEL: &str = "done";

const RESULTS_DOWNLOADED: &str = "DOWNLOADED";
const RESULTS_SKIPPED: &str = "SKIPPED";
const RESULTS_FAILED: &str = "FAILED";
const RESULTS_UNVERIFIED: &str = "UNVERIFIED";
const RESULTS_OUTRO_1: &str = "Done! Check https://github.com/uwuclxdy/osu-collect#importing-beatmaps for how to import downloaded beatmaps into osu correctly";
const RESULTS_OUTRO_2: &str = "and leave a star while you're at it :3";

const LOW_DISK_PREFIX: &str = " low disk space: ";
const LOW_DISK_SUFFIX: &str = " available";

/// Minimum content-area height before switching to compact layout.
const COMPACT_HEIGHT: u16 = 12;

const COMPACT_ACTIVE: &str = "active: ";
const COMPACT_FAILED: &str = " failed: ";

pub fn render(frame: &mut Frame, area: Rect, page: &CollectionPage, tick: u64) {
    if area.height < COMPACT_HEIGHT {
        render_compact(frame, area, page, tick);
        return;
    }
    let show_disk_warning = should_render_disk_warning(page);

    let sections = match show_disk_warning {
        true => Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(INFO_HEIGHT),
            Constraint::Length(GAUGE_HEIGHT),
            Constraint::Min(0),
        ])
        .split(area),
        false => Layout::vertical([
            Constraint::Length(INFO_HEIGHT),
            Constraint::Length(GAUGE_HEIGHT),
            Constraint::Min(0),
        ])
        .split(area),
    };
    let mut idx = 0;
    if show_disk_warning {
        render_disk_warning(frame, sections[idx], page);
        idx += 1;
    }
    render_info(frame, sections[idx], page);
    render_gauge(frame, sections[idx + 1], page, tick);
    render_threads(frame, sections[idx + 2], page);
}

/// Compact render: overall gauge + active-download count + failed count.
///
/// Per-row breakdown, failed-maps collapsible, and session ETA are hidden.
/// The gauge alone tells the user "is it making progress."
fn render_compact(frame: &mut Frame, area: Rect, page: &CollectionPage, tick: u64) {
    let sections = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(area);
    render_gauge(frame, sections[0], page, tick);

    let active_count = page
        .active_downloads
        .iter()
        .flatten()
        .filter(|l| !l.stage.is_terminal())
        .count();
    let failed = page.stats.failed;

    let key_style = Style::default().fg(text_faint());
    let line = Line::from(vec![
        Span::styled(COMPACT_ACTIVE, key_style),
        Span::styled(active_count.to_string(), Style::default().fg(text_muted())),
        Span::styled(COMPACT_FAILED, key_style),
        Span::styled(
            failed.to_string(),
            if failed > 0 {
                Style::default().fg(danger())
            } else {
                Style::default().fg(text_muted())
            },
        ),
    ]);
    frame.render_widget(Paragraph::new(line), sections[1]);
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
            "{LOW_DISK_PREFIX}{}{LOW_DISK_SUFFIX}",
            format_bytes(available, "B")
        );
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(warning())),
            area,
        );
    }
}

fn render_info(frame: &mut Frame, area: Rect, page: &CollectionPage) {
    let status =
        if matches!(page.stage, DownloadStage::Downloading) && page.all_active_rate_limited() {
            RATE_LIMITED
        } else {
            stage_label(page.stage)
        };

    let speed = current_speed(page);
    let bytes = bytes_display(page);

    let key_style = Style::default().fg(text_faint());
    let value_style = Style::default().fg(text_muted());

    let mut status_spans = vec![
        Span::styled(KEY_STATUS, key_style),
        widgets::status_pill(status, status_color(page.stage, status)),
    ];
    if let Some(speed) = speed {
        status_spans.push(Span::styled(KEY_SPEED, key_style));
        status_spans.push(Span::styled(speed, Style::default().fg(success())));
    }
    if let Some(bytes) = bytes {
        status_spans.push(Span::styled(KEY_SIZE, key_style));
        status_spans.push(Span::styled(bytes, Style::default().fg(warning())));
    }

    let lines = vec![
        Line::from(vec![
            Span::styled(KEY_COLLECTION, key_style),
            Span::styled(page.title.as_str(), Style::default().fg(accent())),
        ]),
        Line::from(vec![
            Span::styled(KEY_UPLOADER, key_style),
            Span::styled(
                page.uploader.as_deref().unwrap_or(VALUE_UNKNOWN),
                value_style,
            ),
        ]),
        Line::from(vec![
            Span::styled(KEY_OUTPUT, key_style),
            Span::styled(
                page.output_dir
                    .as_deref()
                    .map(|p| pretty_path(p).into_owned())
                    .unwrap_or_else(|| VALUE_PREPARING.to_string()),
                value_style,
            ),
        ]),
        Line::from(vec![
            Span::styled(KEY_SETTINGS, key_style),
            Span::styled(
                {
                    let mut s = page.concurrent.to_string();
                    s.push(' ');
                    s.push_str(VALUE_THREADS_SUFFIX);
                    s
                },
                Style::default().fg(accent()),
            ),
        ]),
        Line::from(status_spans),
        Line::from(summary_spans(page)),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(widgets::panel_block(PANEL_OVERVIEW))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn stage_label(stage: DownloadStage) -> &'static str {
    match stage {
        DownloadStage::Pending => STATUS_PENDING,
        DownloadStage::Resolving => STATUS_RESOLVING,
        DownloadStage::Rechecking => STATUS_RECHECKING,
        DownloadStage::Downloading => STATUS_DOWNLOADING,
        DownloadStage::Completed => STATUS_COMPLETED,
        DownloadStage::Failed => STATUS_FAILED,
    }
}

fn current_speed(page: &CollectionPage) -> Option<String> {
    if !matches!(page.stage, DownloadStage::Downloading) {
        return None;
    }
    let speed = page.cumulative_speed();
    (speed >= 1.0).then(|| format_bytes(speed as u64, "B/s"))
}

fn bytes_display(page: &CollectionPage) -> Option<String> {
    if !matches!(
        page.stage,
        DownloadStage::Downloading | DownloadStage::Completed
    ) {
        return None;
    }
    page.stats.total_collection_bytes.map(|total| {
        format!(
            "{}/{}",
            format_bytes(page.total_downloaded_bytes(), "B"),
            format_bytes(total, "B"),
        )
    })
}

fn status_color(stage: DownloadStage, status: &str) -> Color {
    if status == RATE_LIMITED {
        warning()
    } else {
        widgets::status_style(stage).fg.unwrap_or(text_dim())
    }
}

fn summary_spans(page: &CollectionPage) -> Vec<Span<'static>> {
    let (label, downloaded, skipped, failed, unverified) = if let Some(summary) = &page.summary {
        (
            SUMMARY_DONE,
            summary.downloaded,
            summary.skipped,
            summary.failed,
            summary.unverified,
        )
    } else {
        (
            SUMMARY_PROGRESS,
            page.stats.downloaded,
            page.stats.skipped,
            page.stats.failed,
            page.stats.unverified,
        )
    };

    let displayed_skipped = skipped.saturating_add(unverified);
    let mut spans = vec![
        Span::styled(format!("{label}: "), Style::default().fg(text_faint())),
        Span::styled(
            {
                let mut s = downloaded.to_string();
                s.push_str(" downloaded");
                s
            },
            Style::default().fg(success()),
        ),
        Span::styled(SEPARATOR, Style::default().fg(line_soft())),
        Span::styled(
            {
                let mut s = displayed_skipped.to_string();
                s.push_str(" skipped");
                s
            },
            Style::default().fg(text_muted()),
        ),
        Span::styled(SEPARATOR, Style::default().fg(line_soft())),
        Span::styled(
            {
                let mut s = failed.to_string();
                s.push_str(" failed");
                s
            },
            if failed > 0 {
                Style::default().fg(danger())
            } else {
                Style::default().fg(text_muted())
            },
        ),
    ];
    if unverified > 0 {
        spans.push(Span::styled(SEPARATOR, Style::default().fg(line_soft())));
        spans.push(Span::styled(
            {
                let mut s = unverified.to_string();
                s.push_str(" unverified");
                s
            },
            Style::default().fg(warning()),
        ));
    }
    spans
}

/// Returns `(avg_speed_str, eta_str)` for the session gauge label, or `None`
/// if there is not yet enough data (elapsed < 1s, no bytes downloaded, or no
/// total size known).
fn session_eta(page: &CollectionPage) -> Option<(String, String)> {
    let start = page.session_start?;
    let elapsed = start.elapsed();
    if elapsed.as_secs_f64() < 1.0 {
        return None;
    }
    let bytes_done = page.stats.bytes_downloaded;
    if bytes_done == 0 {
        return None;
    }
    let total = page.stats.total_collection_bytes?;
    let speed = bytes_done as f64 / elapsed.as_secs_f64();
    let remaining = total.saturating_sub(bytes_done);
    let eta_secs = (remaining as f64 / speed) as u64;
    let speed_str = format_bytes(speed as u64, "B/s");
    let eta_str = format_eta(eta_secs);
    Some((speed_str, eta_str))
}

/// Format a duration in seconds as a compact human label: `45s`, `2m 30s`, `1h 12m`.
pub(crate) fn format_eta(secs: u64) -> String {
    if secs < 60 {
        let mut s = secs.to_string();
        s.push('s');
        s
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        format!("{m}m {s:02}s")
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        format!("{h}h {m:02}m")
    }
}

fn format_avg_verify(avg_us: u64) -> String {
    if avg_us < 1_000 {
        format!("{avg_us}us")
    } else if avg_us < 1_000_000 {
        format!("{:.1}ms", avg_us as f64 / 1_000.0)
    } else if avg_us < 60_000_000 {
        format!("{:.1}s", avg_us as f64 / 1_000_000.0)
    } else {
        format!("{:.1}m", avg_us as f64 / 60_000_000.0)
    }
}

fn render_gauge(frame: &mut Frame, area: Rect, page: &CollectionPage, tick: u64) {
    if matches!(
        page.stage,
        DownloadStage::Pending | DownloadStage::Resolving
    ) {
        if let Some((current, total)) = page.resolve_progress
            && total > 0
        {
            render_resolve_progress_gauge(frame, area, current, total, tick);
        } else {
            render_indeterminate_gauge(frame, area, page, page.stage, tick);
        }
        return;
    }

    let is_rechecking = matches!(page.stage, DownloadStage::Rechecking);
    let queue_remaining = page.download_target;
    let total_collection = page.total_maps.max(1);
    let downloaded = page.stats.downloaded as usize;
    let failed = page.stats.failed as usize;
    let verified = downloaded + page.stats.skipped as usize;
    let verified_display = verified.min(total_collection);
    let failed_display = failed.min(total_collection.saturating_sub(verified_display));
    let verified_ratio = (verified_display as f64 / total_collection as f64).clamp(0.0, 1.0);
    let failed_ratio = (failed_display as f64 / total_collection as f64).clamp(0.0, 1.0);

    let mut top_style = Style::default()
        .fg(text_muted())
        .add_modifier(Modifier::BOLD);
    if is_rechecking {
        top_style = top_style.fg(warning());
    }

    let top_title = if is_rechecking {
        format!(" rechecking {verified_display}/{total_collection} ")
    } else {
        let eta_suffix = session_eta(page)
            .map(|(speed, eta)| format!("  {speed}  ETA {eta}"))
            .unwrap_or_default();
        format!(" {downloaded} downloaded  {queue_remaining} queued{eta_suffix} ")
    };
    let verified_title = if let Some(avg_us) = page.avg_verify_us() {
        format!(
            " {verified_display}/{total_collection} verified ({} avg) ",
            format_avg_verify(avg_us)
        )
    } else {
        format!(" {verified_display}/{total_collection} verified ")
    };

    let block = Block::default()
        .title(Line::from(Span::styled(top_title, top_style)).left_aligned())
        .title_bottom(
            Line::from(Span::styled(
                verified_title,
                Style::default().fg(text_faint()),
            ))
            .right_aligned(),
        );

    let fill_color = if is_rechecking { warning() } else { accent() };

    if failed_display == 0 {
        frame.render_widget(
            Gauge::default()
                .block(block)
                .ratio(verified_ratio)
                .label(Span::raw(""))
                .gauge_style(Style::default().fg(fill_color).bg(bg_raised())),
            area,
        );
        return;
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let bar_width = inner.width as usize;
    let verified_cells = ((verified_ratio * bar_width as f64).round() as usize).min(bar_width);
    let failed_cells =
        ((failed_ratio * bar_width as f64).round() as usize).min(bar_width - verified_cells);
    let empty_cells = bar_width.saturating_sub(verified_cells + failed_cells);

    let bar = Line::from(vec![
        Span::styled(
            glyph_fill(&FILL_BLOCK, GLYPH_BLOCK, verified_cells).into_owned(),
            Style::default().fg(fill_color),
        ),
        Span::styled(
            glyph_fill(&FILL_BLOCK, GLYPH_BLOCK, failed_cells).into_owned(),
            Style::default().fg(danger()),
        ),
        Span::styled(
            glyph_fill(&FILL_SHADE, GLYPH_SHADE, empty_cells).into_owned(),
            Style::default().fg(bg_raised()),
        ),
    ]);
    frame.render_widget(Paragraph::new(bar), inner);
}

fn render_indeterminate_gauge(
    frame: &mut Frame,
    area: Rect,
    page: &CollectionPage,
    stage: DownloadStage,
    tick: u64,
) {
    let spinner = spinner_char(tick);
    let label = match stage {
        DownloadStage::Pending => PLACEHOLDER_PREPARING,
        _ => PLACEHOLDER_RESOLVING,
    };
    let title = format!(" {spinner} {label} ");
    render_indeterminate_block(frame, area, &title, page, tick);
}

fn render_resolve_progress_gauge(
    frame: &mut Frame,
    area: Rect,
    current: u32,
    total: u32,
    tick: u64,
) {
    let spinner = spinner_char(tick);
    let title = format!(" {spinner} resolving {current}/{total} collections ");
    let title_style = Style::default().fg(info()).add_modifier(Modifier::BOLD);
    let block = Block::default().title(Line::from(Span::styled(title, title_style)).left_aligned());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let bar_width = inner.width as usize;
    let ratio = if total == 0 {
        0.0
    } else {
        (current as f64 / total as f64).clamp(0.0, 1.0)
    };
    let filled = ((ratio * bar_width as f64).round() as usize).min(bar_width);
    let empty = bar_width.saturating_sub(filled);

    let bar = Line::from(vec![
        Span::styled(
            glyph_fill(&FILL_BLOCK, GLYPH_BLOCK, filled).into_owned(),
            Style::default().fg(info()),
        ),
        Span::styled(
            glyph_fill(&FILL_SHADE, GLYPH_SHADE, empty).into_owned(),
            Style::default().fg(bg_raised()),
        ),
    ]);
    let bar_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    frame.render_widget(Paragraph::new(bar), bar_area);
}

fn render_indeterminate_block(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    page: &CollectionPage,
    tick: u64,
) {
    let title_style = Style::default().fg(info()).add_modifier(Modifier::BOLD);
    let block = Block::default().title(Line::from(Span::styled(title, title_style)).left_aligned());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let bar_width = inner.width as usize;
    let chunk_width = (bar_width / 5).clamp(3, bar_width);
    let span = bar_width.saturating_sub(chunk_width).max(1);
    let start_tick = match page.indeterminate_anim_start.get() {
        Some(start_tick) => start_tick,
        None => {
            page.indeterminate_anim_start.set(Some(tick));
            tick
        }
    };
    let elapsed = tick.saturating_sub(start_tick) as usize;
    let start_offset = (bar_width / 3).min(span);
    let phase = (start_offset + elapsed) % (2 * span);
    let chunk_start = if phase <= span {
        phase
    } else {
        2 * span - phase
    };
    let chunk_end = (chunk_start + chunk_width).min(bar_width);
    let visible = chunk_end - chunk_start;
    let trailing = bar_width - chunk_start - visible;

    let bar = Line::from(vec![
        Span::styled(
            glyph_fill(&FILL_SHADE, GLYPH_SHADE, chunk_start).into_owned(),
            Style::default().fg(bg_raised()),
        ),
        Span::styled(
            glyph_fill(&FILL_BLOCK, GLYPH_BLOCK, visible).into_owned(),
            Style::default().fg(info()),
        ),
        Span::styled(
            glyph_fill(&FILL_SHADE, GLYPH_SHADE, trailing).into_owned(),
            Style::default().fg(bg_raised()),
        ),
    ]);
    frame.render_widget(Paragraph::new(bar), inner);
}

fn render_threads(frame: &mut Frame, area: Rect, page: &CollectionPage) {
    if matches!(page.stage, DownloadStage::Completed)
        && let Some(summary) = &page.summary
    {
        render_results_block(frame, area, summary);
        return;
    }

    let block = widgets::panel_block(PANEL_ACTIVE);
    let inner = block.inner(area);

    let row_width = inner.width;
    let mut items: Vec<ListItem> = Vec::new();

    if matches!(page.stage, DownloadStage::Downloading) {
        // Render non-rate-limited slots first (preserving slot order), then a separator when
        // both groups are non-empty, then the rate-limited rows with countdown.
        let mut non_rate_limited_count = 0usize;
        let mut rate_limited_count = 0usize;
        for line in page.active_downloads.iter().flatten() {
            if line.stage.is_terminal() {
                continue;
            }
            if line.displayed_rate_limited() {
                rate_limited_count += 1;
            } else {
                non_rate_limited_count += 1;
            }
        }

        for slot in &page.active_downloads {
            match slot {
                Some(line) if !line.stage.is_terminal() && line.displayed_rate_limited() => {
                    // rendered in second pass below
                }
                Some(line) => items.push(widgets::active_download_item(line, row_width)),
                None => items.push(ListItem::new("")),
            }
        }

        if rate_limited_count > 0 && non_rate_limited_count > 0 {
            items.push(ListItem::new(Line::from(vec![Span::styled(
                RATE_LIMITED_SECTION_LABEL,
                Style::default().fg(text_faint()),
            )])));
        }

        for slot in &page.active_downloads {
            if let Some(line) = slot
                && !line.stage.is_terminal()
                && line.displayed_rate_limited()
            {
                items.push(rate_limited_item(line, row_width));
            }
        }

        // Footer: show cumulative completed count below active/rate-limited rows.
        let done = page.stats.downloaded;
        if done > 0 {
            items.push(done_footer_item(done, page.stats.skipped));
        }
    } else if items.is_empty() && page.failed_maps.is_empty() {
        let (text, color) = match page.stage {
            DownloadStage::Rechecking => (ACTIVE_VERIFYING, warning()),
            DownloadStage::Pending | DownloadStage::Resolving => (ACTIVE_FETCHING, info()),
            _ => (ACTIVE_NONE, text_faint()),
        };
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(text, Style::default().fg(color)),
        ])));
    }

    if !page.failed_maps.is_empty() {
        let count = page.failed_maps.len();
        let detail = format!("({count})");
        items.push(widgets::disclosure_row(
            FAILED_SECTION_LABEL,
            detail,
            page.failed_section_expanded,
            false,
        ));
        if page.failed_section_expanded {
            for failure in &page.failed_maps {
                let (reason_label, reason_color) = failure_display(failure.reason);
                let id_str = format!("#{}", failure.beatmapset_id);
                let title_part = failure
                    .title
                    .as_deref()
                    .map(|t| format!(" · {t}"))
                    .unwrap_or_default();
                items.push(ListItem::new(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(id_str, Style::default().fg(text_faint())),
                    Span::styled(title_part, Style::default().fg(text_faint())),
                    Span::raw("  "),
                    Span::styled(reason_label, Style::default().fg(reason_color)),
                ])));
            }
        }
    }

    let visible_height = inner.height as usize;
    let total = items.len();
    page.thread_total_items.set(total);
    page.thread_visible_items.set(visible_height);

    let max_scroll = total.saturating_sub(visible_height);
    let start = page.thread_scroll.min(max_scroll);
    let end = (start + visible_height).min(total);

    let block = match widgets::scroll_indicator(start, end, total) {
        Some(span) => block.title_top(Line::from(span).right_aligned()),
        None => block,
    };
    frame.render_widget(block, area);

    frame.render_widget(
        List::new(items[start..end].to_vec()).highlight_symbol(""),
        inner,
    );
}

fn render_results_block(frame: &mut Frame, area: Rect, summary: &DownloadSummary) {
    let eyebrow_style = eyebrow().add_modifier(Modifier::DIM);
    let displayed_skipped = summary.skipped.saturating_add(summary.unverified);
    let failed_style = if summary.failed > 0 {
        Style::default().fg(danger())
    } else {
        Style::default().fg(text_muted())
    };
    let mut spans = vec![
        Span::raw("  "),
        Span::styled(RESULTS_DOWNLOADED, eyebrow_style),
        Span::raw(" "),
        Span::styled(
            summary.downloaded.to_string(),
            Style::default().fg(accent()),
        ),
        Span::styled(SEPARATOR, Style::default().fg(line_soft())),
        Span::styled(RESULTS_SKIPPED, eyebrow_style),
        Span::raw(" "),
        Span::styled(
            displayed_skipped.to_string(),
            Style::default().fg(text_muted()),
        ),
        Span::styled(SEPARATOR, Style::default().fg(line_soft())),
        Span::styled(RESULTS_FAILED, eyebrow_style),
        Span::raw(" "),
        Span::styled(summary.failed.to_string(), failed_style),
    ];
    if summary.unverified > 0 {
        spans.push(Span::styled(SEPARATOR, Style::default().fg(line_soft())));
        spans.push(Span::styled(RESULTS_UNVERIFIED, eyebrow_style));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            summary.unverified.to_string(),
            Style::default().fg(warning()),
        ));
    }

    let lines = vec![
        Line::from(spans),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(RESULTS_OUTRO_1, Style::default().fg(text_muted())),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(RESULTS_OUTRO_2, Style::default().fg(text_faint())),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(widgets::panel_block(PANEL_RESULTS))
            .wrap(Wrap { trim: true }),
        area,
    );
}

/// Builds the spans for the `done (N) [· skipped (M)]` footer row.
///
/// Split from the `ListItem` constructor so tests can inspect content and colors
/// without depending on ratatui internals.
pub(crate) fn done_footer_spans(done: u32, skipped: u32) -> Vec<Span<'static>> {
    let mut spans = vec![
        Span::raw("  "),
        Span::styled(DONE_LABEL, Style::default().fg(success())),
        Span::styled(" (", Style::default().fg(text_dim())),
        Span::styled(done.to_string(), Style::default().fg(text_dim())),
        Span::styled(")", Style::default().fg(text_dim())),
    ];
    if skipped > 0 {
        spans.push(Span::styled(
            " · skipped (",
            Style::default().fg(text_dim()),
        ));
        spans.push(Span::styled(
            skipped.to_string(),
            Style::default().fg(text_dim()),
        ));
        spans.push(Span::styled(")", Style::default().fg(text_dim())));
    }
    spans
}

/// Builds the `done (N) [· skipped (M)]` footer row shown below active threads.
///
/// Renders whenever at least one beatmapset has completed successfully.
/// The skipped segment is appended only when `skipped > 0`.
fn done_footer_item(done: u32, skipped: u32) -> ListItem<'static> {
    ListItem::new(Line::from(done_footer_spans(done, skipped)))
}

/// Builds a list row for a rate-limited download slot.
///
/// Uses the standard `active_download_item_msg` layout but appends a `Ns`
/// countdown suffix so the user sees how long until the mirror is eligible again.
/// When no cooldown deadline is recorded, delegates to the plain item builder.
fn rate_limited_item(
    line: &crate::app::collection::ActiveDownloadLine,
    width: u16,
) -> ListItem<'static> {
    let base = line.displayed_message();
    let msg = match line.cooldown_secs_remaining() {
        Some(secs) => {
            let s = secs.to_string();
            let mut buf = String::with_capacity(base.len() + 1 + s.len() + 1);
            buf.push_str(&base);
            buf.push(' ');
            buf.push_str(&s);
            buf.push('s');
            buf
        }
        None => base,
    };
    widgets::active_download_item_msg(line, &msg, width)
}

/// Returns `(display_label, color)` for a failure reason.
///
/// Transient errors (`NetworkError`, `RateLimited`) use warning color; definitive
/// failures use danger color.
fn failure_display(reason: FailureReason) -> (&'static str, ratatui::style::Color) {
    match reason {
        FailureReason::NetworkError | FailureReason::RateLimited => (reason.label(), warning()),
        FailureReason::NotFound | FailureReason::ValidationFailed | FailureReason::Unknown => {
            (reason.label(), danger())
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tui_download.rs"]
mod tests;
