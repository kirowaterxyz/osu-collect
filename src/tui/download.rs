use crate::{
    app::{CollectionPage, collection::FailureReason},
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
    glyph_fill, info, line_soft, spinner_str, success, text_dim, text_faint, text_muted, warning,
};

const INFO_HEIGHT: u16 = 8;
const GAUGE_HEIGHT: u16 = 3;
/// Horizontal margin (columns) applied to each side of the progress bar.
const GAUGE_H_MARGIN: u16 = 1;

const PANEL_OVERVIEW: &str = " OVERVIEW ";
const PANEL_ACTIVE: &str = " ACTIVE ";
const PANEL_RESULTS: &str = " RESULTS ";
const PANEL_FAILED: &str = " FAILED ";

/// Fixed height of the results-summary panel when it shares the completed view
/// with the failed-maps disclosure. Fits the counts line, both outro lines, and
/// the panel borders; the failed list takes the remaining space below.
const RESULTS_SUMMARY_HEIGHT: u16 = 8;

const KEY_COLLECTION: &str = "collection: ";
const KEY_UPLOADER: &str = "uploader: ";
const KEY_OUTPUT: &str = "output: ";
const KEY_SETTINGS: &str = "settings: ";
const KEY_STATUS: &str = "status: ";
const KEY_SPEED: &str = "  speed ";
const KEY_ETA: &str = "  eta ";
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
const SUMMARY_UNVERIFIED: &str = " unverified";

const ACTIVE_VERIFYING: &str = "verifying existing archives...";
const ACTIVE_FETCHING: &str = "fetching collection metadata...";
const ACTIVE_NONE: &str = "no active threads";
const PLACEHOLDER_PREPARING: &str = "preparing";
const PLACEHOLDER_RESOLVING: &str = "resolving collection";

const FAILED_SECTION_LABEL: &str = "FAILED";

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
    let show_disk_warning = should_render_disk_warning(page);

    let sections = if show_disk_warning {
        Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area)
    } else {
        Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(area)
    };
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

    if show_disk_warning {
        render_disk_warning(frame, sections[1], page);
        frame.render_widget(Paragraph::new(line), sections[2]);
    } else {
        frame.render_widget(Paragraph::new(line), sections[1]);
    }
}

fn should_render_disk_warning(page: &CollectionPage) -> bool {
    page.low_disk_space.is_some()
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
        let bytes = format_bytes(available, "B");
        let mut text =
            String::with_capacity(LOW_DISK_PREFIX.len() + bytes.len() + LOW_DISK_SUFFIX.len());
        text.push_str(LOW_DISK_PREFIX);
        text.push_str(&bytes);
        text.push_str(LOW_DISK_SUFFIX);
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(warning())),
            area,
        );
    }
}

fn render_info(frame: &mut Frame, area: Rect, page: &CollectionPage) {
    let rate_limited =
        matches!(page.stage, DownloadStage::Downloading) && page.all_active_rate_limited();
    let status = if rate_limited {
        crate::config::constants::status::RATE_LIMITED
    } else {
        stage_label(page.stage)
    };

    let speed = current_speed(page);
    let bytes = bytes_display(page);

    let key_style = Style::default().fg(text_faint());
    let value_style = Style::default().fg(text_muted());

    let mut status_spans = vec![
        Span::styled(KEY_STATUS, key_style),
        widgets::status_pill(status, status_color(page.stage, rate_limited)),
    ];
    if let Some(speed) = speed {
        status_spans.push(Span::styled(KEY_SPEED, key_style));
        status_spans.push(Span::styled(speed, Style::default().fg(success())));
        // ETA sits next to speed (both derived from cumulative_speed) so the
        // gauge can drop the duplicate — every figure lives in exactly one place.
        if let Some(eta) = session_eta(page) {
            status_spans.push(Span::styled(KEY_ETA, key_style));
            status_spans.push(Span::styled(eta, Style::default().fg(accent())));
        }
    }
    if let Some(bytes) = bytes {
        status_spans.push(Span::styled(KEY_SIZE, key_style));
        status_spans.push(Span::styled(bytes, Style::default().fg(warning())));
    }

    let mut lines = vec![
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
    ];
    // `settings: N threads` is static config noise once the run is live — speed
    // and ETA in the status line carry the actionable signal. Drop it while
    // Downloading; keep it for other stages where nothing else fills the row.
    if !matches!(page.stage, DownloadStage::Downloading) {
        lines.push(Line::from(vec![
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
        ]));
    }
    lines.push(Line::from(status_spans));
    lines.push(Line::from(summary_spans(page)));

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

fn status_color(stage: DownloadStage, rate_limited: bool) -> Color {
    if rate_limited {
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
                let mut s = skipped.to_string();
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
                s.push_str(SUMMARY_UNVERIFIED);
                s
            },
            Style::default().fg(warning()),
        ));
    }
    spans
}

/// Returns the session ETA label, or `None` if there is not yet enough data
/// (speed < 1 B/s or no total size known). Speed comes from `cumulative_speed()`
/// — the same rolling average shown in the OVERVIEW panel — so the ETA derived
/// here always agrees with the displayed speed.
fn session_eta(page: &CollectionPage) -> Option<String> {
    let speed = page.cumulative_speed();
    if speed < 1.0 {
        return None;
    }
    let total = page.stats.total_collection_bytes?;
    let bytes_done = page.stats.bytes_downloaded;
    let remaining = total.saturating_sub(bytes_done);
    let eta_secs = (remaining as f64 / speed) as u64;
    Some(format_eta(eta_secs))
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

fn render_gauge(frame: &mut Frame, area: Rect, page: &CollectionPage, tick: u64) {
    // Inset the bar by GAUGE_H_MARGIN on each side; titles stay at the outer edge.
    let bar_area = Rect {
        x: area.x.saturating_add(GAUGE_H_MARGIN),
        y: area.y,
        width: area.width.saturating_sub(GAUGE_H_MARGIN * 2),
        height: area.height,
    };

    if matches!(
        page.stage,
        DownloadStage::Pending | DownloadStage::Resolving
    ) {
        if let Some((current, total)) = page.resolve_progress
            && total > 0
        {
            render_resolve_progress_gauge(frame, bar_area, current, total, tick);
        } else {
            render_indeterminate_gauge(frame, bar_area, page, page.stage, tick);
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
        format!(" {downloaded} downloaded  {queue_remaining} queued ")
    };
    let verified_title = format!(" {verified_display}/{total_collection} verified ");

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
            bar_area,
        );
        return;
    }

    let inner = block.inner(bar_area);
    frame.render_widget(block, bar_area);
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
    let spinner = spinner_str(tick).trim();
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
    let spinner = spinner_str(tick).trim();
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
        // No failures: the results block owns the whole panel area as before.
        // With failures, keep the failed-maps disclosure visible and navigable
        // (r/R retry, enter expand, ↑↓) by splitting the area — results summary
        // on top, the scrollable failed list below.
        if page.failed_maps.is_empty() {
            render_results_block(frame, area, summary);
        } else {
            let [summary_area, failed_area] = Layout::vertical([
                Constraint::Length(RESULTS_SUMMARY_HEIGHT),
                Constraint::Min(0),
            ])
            .areas(area);
            render_results_block(frame, summary_area, summary);
            render_failed_section(frame, failed_area, page);
        }
        return;
    }

    let block = widgets::panel_block(PANEL_ACTIVE);
    let inner = block.inner(area);

    let row_width = inner.width;
    let mut items: Vec<ListItem> = Vec::new();

    if matches!(page.stage, DownloadStage::Downloading) {
        // Single pass: collect non-rate-limited and rate-limited rows separately,
        // then emit non-rate-limited first and rate-limited last with no separator.
        // Each rate-limited row already carries warning() bar color + a per-row
        // countdown, so no divider is needed as a third signal.
        let mut normal: Vec<ListItem> = Vec::new();
        let mut throttled: Vec<ListItem> = Vec::new();

        for slot in &page.active_downloads {
            match slot {
                Some(line) if !line.stage.is_terminal() && line.displayed_rate_limited() => {
                    throttled.push(rate_limited_item(line, row_width));
                }
                Some(line) => normal.push(widgets::active_download_item(line, row_width)),
                None => normal.push(ListItem::new("")),
            }
        }

        items.extend(normal);
        items.extend(throttled);
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
        push_failed_rows(&mut items, page);
    }

    render_scrollable_panel(frame, area, block, inner, items, page);
}

/// Render the `FAILED (n)` disclosure as a standalone scrollable panel.
///
/// Used in the completed view (alongside the results summary) so the failed
/// list stays expandable and navigable post-completion. Shares the same
/// `thread_scroll` / `thread_total_items` / `thread_visible_items` bookkeeping
/// as the ACTIVE panel, so ↑↓ navigation and `r`/`R` retry keep working.
fn render_failed_section(frame: &mut Frame, area: Rect, page: &CollectionPage) {
    let block = widgets::panel_block(PANEL_FAILED);
    let inner = block.inner(area);
    let mut items: Vec<ListItem> = Vec::new();
    push_failed_rows(&mut items, page);
    render_scrollable_panel(frame, area, block, inner, items, page);
}

/// Clamp `thread_scroll` to the item count, attach a scroll indicator, and draw
/// the visible window. Records `thread_total_items` / `thread_visible_items` so
/// the app-side scroll/nav keybinds operate on the same bounds.
fn render_scrollable_panel(
    frame: &mut Frame,
    area: Rect,
    block: Block<'static>,
    inner: Rect,
    items: Vec<ListItem<'static>>,
    page: &CollectionPage,
) {
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

/// Append the `FAILED (n)` disclosure header and, when expanded, one row per
/// failed beatmapset. Shared by the ACTIVE panel and the completed-view panel
/// so both render identical rows.
fn push_failed_rows(items: &mut Vec<ListItem<'static>>, page: &CollectionPage) {
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

fn render_results_block(frame: &mut Frame, area: Rect, summary: &DownloadSummary) {
    let eyebrow_style = eyebrow().add_modifier(Modifier::DIM);
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
            summary.skipped.to_string(),
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
