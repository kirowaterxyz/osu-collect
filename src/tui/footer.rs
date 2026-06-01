use crate::app::{
    App, ConfigField, HomeField, HomeTab, MessageKind, UpdatesField, UpdatesTab,
    messages::AppMessage,
};
use crate::config::constants::{CONFIG_TAB_INDEX, HOME_TAB_INDEX, UPDATES_TAB_INDEX};
use crate::download::DownloadStage;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::widgets;
use super::{
    accent, danger, info, line_soft, spinner_str, text_dim, text_faint, text_muted, warning,
};

const HINT_SEPARATOR: &str = "  ·  ";

const QUIT_PROMPT_WARN: &str = " ⚠ ";
const QUIT_PROMPT_TEXT: &str = "press q again to quit";
const QUIT_PROMPT_TEXT_DOWNLOADS: &str = "press q again to quit — active downloads will stop";

const DOWNLOAD_TAB_HINT_RUNNING: &str = "↑↓ scroll  ·  q abort  ·  ? help";
const DOWNLOAD_TAB_HINT_SETTLED: &str = "↑↓ scroll  ·  x/q close  ·  ? help";
const HINT_RETRY: &str = "r retry  ·  R retry all";

const HINT_MOVE: &str = "↑↓ move";
const HINT_SCROLL: &str = "↑↓ scroll";
const HINT_ENTER_TOGGLE: &str = "enter toggle";
const HINT_ENTER_OPEN: &str = "enter open";
const HINT_ENTER_CONFIRM: &str = "enter confirm";
const HINT_ENTER_DOWNLOAD: &str = "enter download";
const HINT_ESC_BACK: &str = "esc back";
const HINT_ESC_QUIT: &str = "esc quit";
const HINT_PLUS_MINUS: &str = "+/- adjust";
const HINT_ALL_NONE: &str = "a all / d none";
const HINT_SAVE: &str = "s save";
const HINT_QUIT: &str = "q quit";
const HINT_HELP: &str = "? help";

const PILL_INFO: &str = "info";
const PILL_ERROR: &str = "error";
const ERROR_DISMISS_HINT: &str = "  [x to dismiss]";

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    if app.home.quit_prompt {
        frame.render_widget(quit_prompt_paragraph(!app.downloads.is_empty()), area);
        return;
    }

    if let Some(msg) = current_message(app) {
        frame.render_widget(Paragraph::new(message_line(msg, app.tick_count)), area);
        return;
    }

    frame.render_widget(Paragraph::new(hint_line(&hint_for(app))), area);
}

fn current_message(app: &App) -> Option<&AppMessage> {
    match app.active_tab() {
        HOME_TAB_INDEX => app.home.message.as_ref(),
        UPDATES_TAB_INDEX => app.updates.message.as_ref(),
        CONFIG_TAB_INDEX => app.config.message.as_ref(),
        _ => None,
    }
}

fn hint_for(app: &App) -> String {
    match app.active_tab() {
        HOME_TAB_INDEX => home_hint(&app.home),
        UPDATES_TAB_INDEX => updates_hint(&app.updates),
        CONFIG_TAB_INDEX => config_hint(app.config.focus),
        _ => download_tab_hint(app),
    }
}

/// `x close` appears only when the active download page is settled
/// (`Completed` or `Failed`). In-progress pages keep the `q abort` hint.
/// A retry segment is appended whenever the active page has failed maps.
fn download_tab_hint(app: &App) -> String {
    let page = app.download_for_tab(app.active_tab());
    let settled = page
        .is_some_and(|page| matches!(page.stage, DownloadStage::Completed | DownloadStage::Failed));
    let base = if settled {
        DOWNLOAD_TAB_HINT_SETTLED
    } else {
        DOWNLOAD_TAB_HINT_RUNNING
    };
    let has_failed = page.is_some_and(|page| !page.failed_maps.is_empty());
    if has_failed {
        join(&[base, HINT_RETRY])
    } else {
        base.to_string()
    }
}

fn join(segments: &[&str]) -> String {
    segments.join(HINT_SEPARATOR)
}

fn home_hint(form: &HomeTab) -> String {
    let mut segments = vec![HINT_MOVE];
    match form.focus {
        HomeField::Download => segments.push(HINT_ENTER_DOWNLOAD),
        f if f.is_stepper() => segments.push(HINT_PLUS_MINUS),
        f if f.is_toggle() => segments.push(HINT_ENTER_TOGGLE),
        // text inputs: nothing to activate on this row
        _ => {}
    }
    if form.focus.is_text_input() {
        // `q` types into the field here; esc is the quit affordance.
        segments.push(HINT_ESC_QUIT);
    } else {
        segments.push(HINT_QUIT);
    }
    segments.push(HINT_HELP);
    join(&segments)
}

fn updates_hint(form: &UpdatesTab) -> String {
    let in_list = form.selection.in_collection_list || form.selection.in_beatmap_list;
    if in_list {
        return join(&[HINT_SCROLL, HINT_ENTER_TOGGLE, HINT_ALL_NONE, HINT_HELP]);
    }

    let mut segments = vec![HINT_MOVE];
    match form.selection.focus {
        UpdatesField::ClientType => segments.push(HINT_ENTER_TOGGLE),
        UpdatesField::Collections | UpdatesField::BeatmapList => segments.push(HINT_ENTER_OPEN),
        UpdatesField::Download => segments.push(HINT_ENTER_DOWNLOAD),
        UpdatesField::OsuPath => {}
    }
    // `q` types into the osu! path field; esc is the quit affordance there.
    if form.selection.focus == UpdatesField::OsuPath {
        segments.push(HINT_ESC_QUIT);
    } else {
        segments.push(HINT_QUIT);
    }
    segments.push(HINT_HELP);
    join(&segments)
}

fn config_hint(focus: ConfigField) -> String {
    let mut segments = vec![HINT_MOVE];
    match focus {
        ConfigField::AuthChip => segments.push(HINT_ENTER_CONFIRM),
        field if field.is_stepper() => segments.push(HINT_PLUS_MINUS),
        field if field.is_text_input() => segments.push(HINT_ESC_BACK),
        _ => segments.push(HINT_ENTER_TOGGLE),
    }
    segments.push(HINT_SAVE);
    segments.push(HINT_HELP);
    join(&segments)
}

fn quit_prompt_paragraph(has_downloads: bool) -> Paragraph<'static> {
    let text = if has_downloads {
        QUIT_PROMPT_TEXT_DOWNLOADS
    } else {
        QUIT_PROMPT_TEXT
    };
    Paragraph::new(Line::from(vec![
        Span::styled(QUIT_PROMPT_WARN, Style::default().fg(warning())),
        Span::styled(text, Style::default().fg(text_dim())),
    ]))
}

fn message_line(msg: &AppMessage, tick: u64) -> Line<'static> {
    let text = msg.text.trim_start().to_string();
    let muted = Style::default().fg(text_muted());
    match msg.kind {
        MessageKind::Loading => Line::from(vec![
            Span::styled(
                spinner_str(tick),
                Style::default().fg(accent()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(text, muted),
        ]),
        MessageKind::Info => Line::from(vec![
            Span::raw(" "),
            widgets::status_pill(PILL_INFO, info()),
            Span::raw(" "),
            Span::styled(text, muted),
        ]),
        MessageKind::Error => Line::from(vec![
            Span::raw(" "),
            widgets::status_pill(PILL_ERROR, danger()),
            Span::raw(" "),
            Span::styled(text, Style::default().fg(danger())),
            Span::styled(ERROR_DISMISS_HINT, Style::default().fg(text_faint())),
        ]),
    }
}

fn hint_line(hint: &str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let label_style = Style::default().fg(text_faint());
    let key_style = Style::default().fg(accent()).add_modifier(Modifier::BOLD);
    let separator_style = Style::default().fg(line_soft());

    for (index, segment) in hint.split('·').enumerate() {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }
        if index > 0 {
            spans.push(Span::styled("  │  ", separator_style));
        } else {
            spans.push(Span::raw(" "));
        }
        let mut parts = trimmed.splitn(2, ' ');
        let key = parts.next().unwrap_or("");
        let label = parts.next().unwrap_or("");
        spans.push(Span::styled(key.to_string(), key_style));
        if !label.is_empty() {
            spans.push(Span::styled(format!(" {label}"), label_style));
        }
    }

    Line::from(spans)
}

#[cfg(test)]
#[path = "../../tests/unit/tui_footer.rs"]
mod tests;
