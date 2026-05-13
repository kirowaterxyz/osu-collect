mod components;
mod config;
mod download;
mod footer;
mod home;
mod updates;

use crate::app::{App, CollectionPage, ConfigTab, HomeTab, UpdatesTab};
use crate::config::constants::{CONFIG_TAB_INDEX, HOME_TAB_INDEX, UPDATES_TAB_INDEX};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Style,
    widgets::Block,
};

pub fn draw(frame: &mut Frame, app: &App) {
    let view = AppView::from(app);
    let area = frame.area();

    if area.width == 0 || area.height == 0 {
        return;
    }

    // fill background with mocha bg (slightly lighter than sunken)
    frame.render_widget(
        Block::default().style(Style::default().bg(components::BG)),
        area,
    );

    // compact: collapse separators for terminals under 14 rows
    let compact = area.height < 14;

    let chunks: Vec<_> = if compact {
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)
        .to_vec()
    } else {
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area)
        .to_vec()
    };

    let (header_area, content_area, status_area) = if compact {
        (chunks[0], chunks[1], chunks[2])
    } else {
        components::render_separator(frame, chunks[1]);
        components::render_separator(frame, chunks[3]);
        (chunks[0], chunks[2], chunks[4])
    };

    components::render_header(frame, header_area, &view.tabs);

    match view.active_tab {
        HOME_TAB_INDEX => home::render(frame, content_area, view.home),
        UPDATES_TAB_INDEX => updates::render(frame, content_area, view.updates),
        CONFIG_TAB_INDEX => config::render(frame, content_area, view.config),
        _ => {
            if let Some(download_view) = view.download {
                download::render(frame, content_area, download_view);
            } else {
                home::render(frame, content_area, view.home);
            }
        }
    }

    let footer_view = footer::FooterView::for_tab(&view);
    footer::render(frame, status_area, footer_view);
}

#[derive(Clone, Copy)]
pub struct HomeView<'a> {
    pub form: &'a HomeTab,
}

impl<'a> From<&'a HomeTab> for HomeView<'a> {
    fn from(form: &'a HomeTab) -> Self {
        Self { form }
    }
}

#[derive(Clone, Copy)]
pub struct UpdatesView<'a> {
    pub form: &'a UpdatesTab,
}

impl<'a> From<&'a UpdatesTab> for UpdatesView<'a> {
    fn from(form: &'a UpdatesTab) -> Self {
        Self { form }
    }
}

#[derive(Clone, Copy)]
pub struct DownloadView<'a> {
    pub page: &'a CollectionPage,
}

impl<'a> From<&'a CollectionPage> for DownloadView<'a> {
    fn from(page: &'a CollectionPage) -> Self {
        Self { page }
    }
}

#[derive(Clone, Copy)]
pub struct ConfigView<'a> {
    pub form: &'a ConfigTab,
    pub quit_prompt: bool,
}

impl<'a> ConfigView<'a> {
    fn new(form: &'a ConfigTab, quit_prompt: bool) -> Self {
        Self { form, quit_prompt }
    }
}

pub struct TabsView {
    titles: Vec<String>,
    active_tab: usize,
}

impl TabsView {
    fn new(titles: Vec<String>, active_tab: usize) -> Self {
        Self { titles, active_tab }
    }

    pub fn titles(&self) -> &[String] {
        &self.titles
    }

    pub fn active(&self) -> usize {
        self.active_tab
    }
}

pub struct AppView<'a> {
    pub home: HomeView<'a>,
    pub updates: UpdatesView<'a>,
    pub config: ConfigView<'a>,
    pub download: Option<DownloadView<'a>>,
    pub tabs: TabsView,
    pub active_tab: usize,
    pub tick_count: u64,
}

impl<'a> From<&'a App> for AppView<'a> {
    fn from(app: &'a App) -> Self {
        let active_tab = app.active_tab();
        let download = app.download_for_tab(active_tab).map(DownloadView::from);

        Self {
            home: HomeView::from(&app.home),
            updates: UpdatesView::from(&app.updates),
            config: ConfigView::new(&app.config, app.home.quit_prompt),
            download,
            tabs: TabsView::new(app.tab_titles(), active_tab),
            active_tab,
            tick_count: app.tick_count,
        }
    }
}
