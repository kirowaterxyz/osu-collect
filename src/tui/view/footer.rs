use super::{TabsView, components};
use ratatui::prelude::Modifier;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, BorderType, Borders},
};

pub struct FooterView<'a> {
    tabs: &'a TabsView,
}

impl<'a> FooterView<'a> {
    pub fn new(tabs: &'a TabsView) -> Self {
        Self { tabs }
    }
}

pub fn render(frame: &mut Frame, area: Rect, view: FooterView) {
    let help_text =
        " ← →: Navigate | ↑ ↓: Scroll | Space: Toggle | Enter: Start Download | q: Quit ";
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .title_bottom(
            Line::from(help_text).centered().style(
                Style::default()
                    .fg(Color::Rgb(108, 112, 134))
                    .add_modifier(Modifier::BOLD),
            ),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let tabs = components::tab_bar(view.tabs);
    frame.render_widget(tabs, inner);
}
