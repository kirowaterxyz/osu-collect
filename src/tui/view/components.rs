use crate::{
    app::{InputField, MessageKind, ThreadStatusLine, messages::AppMessage},
    download::DownloadStage,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, ListItem, Paragraph, Tabs, Wrap},
};

use super::TabsView;

pub fn tab_bar(tabs: &TabsView) -> Tabs<'static> {
    let titles: Vec<Line> = tabs
        .titles()
        .iter()
        .map(|title| Line::from(Span::raw(title.clone())))
        .collect();

    Tabs::new(titles)
        .select(tabs.active())
        .divider(" | ")
        .highlight_style(
            Style::default()
                .fg(Color::Rgb(137, 180, 250))
                .add_modifier(Modifier::BOLD),
        )
}

pub fn input_item(field: &InputField, focused: bool) -> ListItem<'static> {
    let value = if field.value.is_empty() {
        Span::styled(
            field.placeholder.clone(),
            Style::default().fg(Color::Rgb(108, 112, 134)),
        )
    } else {
        Span::raw(field.value.clone())
    };

    let spans = vec![
        Span::styled(
            if focused { "> " } else { "  " },
            Style::default().fg(Color::Rgb(137, 180, 250)),
        ),
        Span::styled(
            format!("{}: ", field.label),
            Style::default().fg(Color::Rgb(205, 214, 244)),
        ),
        value,
    ];

    let style = if focused {
        Style::default().fg(Color::Rgb(137, 180, 250))
    } else {
        Style::default()
    };

    ListItem::new(Line::from(spans)).style(style)
}

pub fn toggle_item(label: &str, state: bool, focused: bool) -> ListItem<'static> {
    let marker = if state { "[x]" } else { "[ ]" };
    let style = if focused {
        Style::default().fg(Color::Rgb(137, 180, 250))
    } else {
        Style::default()
    };
    let spans = vec![
        Span::styled(
            if focused { "> " } else { "  " },
            Style::default().fg(Color::Rgb(137, 180, 250)),
        ),
        Span::styled(marker, style),
        Span::styled(
            format!(" {label}"),
            Style::default().fg(Color::Rgb(205, 214, 244)),
        ),
    ];
    ListItem::new(Line::from(spans)).style(style)
}

pub fn select_item(label: &str, value: &str, focused: bool) -> ListItem<'static> {
    let style = if focused {
        Style::default().fg(Color::Rgb(137, 180, 250))
    } else {
        Style::default()
    };

    let spans = vec![
        Span::styled(
            if focused { "> " } else { "  " },
            Style::default().fg(Color::Rgb(137, 180, 250)),
        ),
        Span::styled(
            format!("{}: ", label),
            Style::default().fg(Color::Rgb(205, 214, 244)),
        ),
        Span::raw(value.to_string()),
    ];

    ListItem::new(Line::from(spans)).style(style)
}

pub fn status_style(stage: DownloadStage) -> Style {
    match stage {
        DownloadStage::Pending | DownloadStage::Resolving | DownloadStage::Rechecking => {
            Style::default().fg(Color::Rgb(249, 226, 175))
        }
        DownloadStage::Downloading => Style::default().fg(Color::Rgb(137, 180, 250)),
        DownloadStage::Completed => Style::default().fg(Color::Rgb(166, 227, 161)),
        DownloadStage::Failed => Style::default().fg(Color::Rgb(243, 139, 168)),
    }
}

pub fn thread_item(index: usize, status: &ThreadStatusLine) -> ListItem<'static> {
    let prefix = Span::styled(
        format!("Thread {}: ", index + 1),
        Style::default().fg(Color::Rgb(205, 214, 244)),
    );
    let line = Line::from(vec![
        prefix,
        Span::styled(status.message.clone(), thread_style(status)),
    ]);
    ListItem::new(line)
}

fn thread_style(status: &ThreadStatusLine) -> Style {
    if status.rate_limited {
        return Style::default().fg(Color::Rgb(249, 226, 175));
    }

    if status.message.to_lowercase().contains("error") || status.message.starts_with("Failed") {
        return Style::default().fg(Color::Rgb(243, 139, 168));
    }

    if status.message.starts_with("Done") {
        return Style::default().fg(Color::Rgb(166, 227, 161));
    }

    if status.message.starts_with("Skipped") {
        return Style::default().fg(Color::Rgb(203, 166, 247));
    }

    Style::default()
}

pub struct ConsoleMessage<'a> {
    pub message: Option<&'a AppMessage>,
    pub quit_prompt: bool,
    pub default_text: &'a str,
}

pub fn render_console(frame: &mut Frame, area: Rect, console: ConsoleMessage) {
    let (text, style) = if console.quit_prompt {
        (
            " Press q again to quit; all downloads will be cancelled.".to_string(),
            Style::default().fg(Color::Rgb(249, 226, 175)),
        )
    } else {
        match console.message {
            Some(msg) => {
                let style = match msg.kind {
                    MessageKind::Info => Style::default().fg(Color::Rgb(166, 227, 161)),
                    MessageKind::Error => Style::default().fg(Color::Rgb(243, 139, 168)),
                    MessageKind::Loading => Style::default().fg(Color::Rgb(249, 226, 175)),
                };
                (msg.text.clone(), style)
            }
            None => (
                console.default_text.to_string(),
                Style::default().fg(Color::Rgb(166, 173, 200)),
            ),
        }
    };

    let paragraph = Paragraph::new(text)
        .style(style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .title(" Console "),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}
