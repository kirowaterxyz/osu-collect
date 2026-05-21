use super::{
    collection::CollectionPage,
    collection_state::{self, CollectionStateFile},
    config::{AuthLoginState, ConfigField, ConfigTab},
    home::{HomeField, HomeTab},
    messages::{clear_expired_message, set_error_message, set_info_message},
    snapshots,
    updates::{UpdatesAction, UpdatesTab, extract_collection_id},
};
use crate::{
    config::{
        Config,
        constants::{
            CONFIG_TAB_INDEX, HOME_TAB_INDEX, STATIC_TABS, TAB_CONFIG_LOWER, TAB_HOME_LOWER,
            TAB_UPDATES_LOWER, UPDATES_TAB_INDEX,
        },
        save_config,
    },
    download::{
        DownloadConfig, DownloadEvent, DownloadId, DownloadRequest, DownloadStage,
        SelectiveDownloadCollection, SelectiveDownloadRequest,
    },
    utils,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::borrow::Cow;
use std::path::PathBuf;
use tracing::debug;

pub struct App {
    pub home: HomeTab,
    pub updates: UpdatesTab,
    pub config: ConfigTab,
    pub downloads: Vec<CollectionPage>,
    pub active_tab: usize,
    pub collection_state: CollectionStateFile,
    pub collection_state_path: Option<PathBuf>,
    pub scan_handle: Option<tokio::task::JoinHandle<()>>,
    pub tick_count: u64,
    pub help_open: bool,
    next_download_id: DownloadId,
}

#[derive(Debug)]
pub enum AppCommand {
    StartDownload {
        id: DownloadId,
        request: DownloadRequest,
    },
    StartSelectiveDownload {
        id: DownloadId,
        request: SelectiveDownloadRequest,
    },
    CancelDownload {
        id: DownloadId,
    },
    Login {
        client_id: String,
        client_secret: String,
    },
    CancelLogin,
    Logout,
    ScanLocalDatabase,
    RecheckFailedMaps,
    /// Collection URL field changed; schedule a debounced metadata resolve.
    ResolveCollectionUrl {
        value: String,
    },
    /// Probe latency for all built-in mirrors.
    ProbeMirrors,
    Quit,
}

impl App {
    pub fn new(config: Config) -> Self {
        let state_path = collection_state::state_path();
        let coll_state = state_path
            .as_deref()
            .map(collection_state::load)
            .unwrap_or_default();
        Self {
            home: HomeTab::new(&config),
            updates: UpdatesTab::new(),
            config: ConfigTab::new(&config),
            downloads: Vec::new(),
            active_tab: HOME_TAB_INDEX,
            collection_state: coll_state,
            collection_state_path: state_path,
            scan_handle: None,
            tick_count: 0,
            help_open: false,
            next_download_id: 1,
        }
    }

    pub fn active_tab(&self) -> usize {
        self.active_tab
    }

    pub fn next_tab(&mut self) -> Option<AppCommand> {
        let total = self.total_tabs();
        self.active_tab = (self.active_tab + 1) % total;
        self.check_auto_scan()
    }

    pub fn prev_tab(&mut self) -> Option<AppCommand> {
        let total = self.total_tabs();
        if self.active_tab == 0 {
            self.active_tab = total - 1;
        } else {
            self.active_tab -= 1;
        }
        self.check_auto_scan()
    }

    fn check_auto_scan(&mut self) -> Option<AppCommand> {
        if self.active_tab == UPDATES_TAB_INDEX && self.updates.needs_initial_scan() {
            self.updates.scan.scan_generation = self.updates.scan.scan_generation.wrapping_add(1);
            Some(AppCommand::ScanLocalDatabase)
        } else if self.active_tab == HOME_TAB_INDEX {
            Some(AppCommand::ProbeMirrors)
        } else {
            None
        }
    }

    fn updates_list_open(&self) -> bool {
        self.active_tab() == UPDATES_TAB_INDEX
            && (self.updates.selection.in_collection_list || self.updates.selection.in_beatmap_list)
    }

    /// Closes the topmost open modal. Returns `true` if one was closed.
    /// `esc` and `q` call this before falling through to the quit flow.
    /// Extend this as new modal types are added.
    fn close_modal(&mut self) -> bool {
        if self.help_open {
            self.help_open = false;
            return true;
        }
        false
    }

    fn focus_next_field(&mut self) {
        match self.active_tab() {
            HOME_TAB_INDEX => self.home.next_field(),
            UPDATES_TAB_INDEX => self.updates.next_field(),
            CONFIG_TAB_INDEX => self.config.next_field(),
            _ => {}
        }
    }

    fn focus_prev_field(&mut self) {
        match self.active_tab() {
            HOME_TAB_INDEX => self.home.prev_field(),
            UPDATES_TAB_INDEX => self.updates.prev_field(),
            CONFIG_TAB_INDEX => self.config.prev_field(),
            _ => {}
        }
    }

    fn try_save_config(&mut self) {
        match self.config.build_config() {
            Ok(new_config) => {
                if let Err(err) = new_config.validate() {
                    set_error_message(&mut self.config.message, err.to_string());
                    return;
                }

                match save_config(&new_config) {
                    Ok(path) => {
                        let message = format!(
                            "Config saved to {} (applies on next launch)",
                            path.display()
                        );
                        set_info_message(&mut self.config.message, message);
                    }
                    Err(err) => set_error_message(&mut self.config.message, err.to_string()),
                }
            }
            Err(err) => set_error_message(&mut self.config.message, err),
        }
    }

    fn request_login(&mut self) -> Option<AppCommand> {
        if matches!(self.config.login_state, AuthLoginState::InProgress(_)) {
            self.config.set_login_failed();
            set_info_message(&mut self.config.message, "login cancelled");
            return Some(AppCommand::CancelLogin);
        }

        let Some((client_id, client_secret)) = crate::auth::bundled_credentials() else {
            set_error_message(
                &mut self.config.message,
                "login unavailable - build without credentials",
            );
            return None;
        };
        self.config.set_login_in_progress();
        Some(AppCommand::Login {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
        })
    }

    fn request_logout(&mut self) -> Option<AppCommand> {
        match self.config.login_state {
            AuthLoginState::LoggedIn => {
                self.config.set_loading("logging out...");
                Some(AppCommand::Logout)
            }
            AuthLoginState::LoggedOut => {
                set_info_message(&mut self.config.message, "already logged out");
                None
            }
            AuthLoginState::InProgress(_) => None,
        }
    }

    fn total_tabs(&self) -> usize {
        STATIC_TABS + self.downloads.len()
    }

    pub fn request_download(&mut self) -> Option<(DownloadId, DownloadRequest)> {
        match self.home.build_request(self.config.archive_validation) {
            Ok(request) => {
                if self.downloads.len() >= usize::MAX - 1 {
                    set_error_message(&mut self.home.message, "Too many downloads queued");
                    return None;
                }

                let id = self.next_download_id;
                self.next_download_id += 1;

                let placeholder_title = Self::placeholder_title(&request.collection_input, id);
                let concurrent = usize::from(request.config.concurrent.max(1));
                let mut page = CollectionPage::new(id, placeholder_title, concurrent);
                page.stage = DownloadStage::Resolving;
                self.downloads.push(page);
                self.active_tab = STATIC_TABS + self.downloads.len() - 1;

                set_info_message(&mut self.home.message, format!("Queued download #{id}"));

                Some((id, request))
            }
            Err(err) => {
                set_error_message(&mut self.home.message, err);
                None
            }
        }
    }

    pub fn request_selective_download(&mut self) -> Option<(DownloadId, SelectiveDownloadRequest)> {
        let beatmapset_ids = self.updates.selected_beatmapset_ids();
        if beatmapset_ids.is_empty() {
            self.updates.set_error("No beatmaps selected for download");
            return None;
        }

        let collection_ids: Vec<u32> = self
            .updates
            .selected_collection_ids()
            .into_iter()
            .filter_map(|id| u32::try_from(id).ok())
            .collect();

        if collection_ids.is_empty() {
            self.updates.set_error("No collections available");
            return None;
        }

        let mirrors = self.home.build_mirror_list();
        if mirrors.is_empty() {
            self.updates
                .set_error("No mirrors selected (configure in Home tab)");
            return None;
        }

        let concurrent = self.home.resolved_threads();

        let directory = if self.home.directory.value.trim().is_empty() {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| ".".to_string())
        } else {
            utils::expand_tilde(self.home.directory.value.trim())
        };

        if self.downloads.len() >= usize::MAX - 1 {
            self.updates.set_error("Too many downloads queued");
            return None;
        }

        let id = self.next_download_id;
        self.next_download_id += 1;

        let placeholder_title = if collection_ids.len() == 1 {
            format!("Update #{}", collection_ids[0])
        } else {
            format!("Update ({} collections)", collection_ids.len())
        };

        let concurrent_usize = usize::from(concurrent.max(1));
        let mut page = CollectionPage::new(id, placeholder_title, concurrent_usize);
        page.stage = DownloadStage::Resolving;
        self.downloads.push(page);
        self.active_tab = STATIC_TABS + self.downloads.len() - 1;

        set_info_message(
            &mut self.updates.message,
            format!(
                "Queued update download #{id} ({} beatmaps)",
                beatmapset_ids.len()
            ),
        );

        let config = DownloadConfig {
            directory,
            mirrors,
            concurrent,
            archive_validation: self.config.archive_validation,
        };

        let current_snapshots = snapshots::current_snapshots(
            self.updates.path.client_type,
            &self.updates.scan.local_collections_raw,
            self.updates.scan.local_beatmapsets.iter(),
            |name| extract_collection_id(name).and_then(|id| u32::try_from(id).ok()),
        );
        let snapshots: Vec<_> = collection_ids
            .iter()
            .filter_map(|collection_id| current_snapshots.get(collection_id).cloned())
            .collect();
        let collections = collection_ids
            .iter()
            .map(|collection_id| SelectiveDownloadCollection {
                id: *collection_id,
                name: snapshots
                    .iter()
                    .find(|snapshot| snapshot.collection_id.parse::<u32>() == Ok(*collection_id))
                    .map(|snapshot| snapshot.name.clone())
                    .unwrap_or_default(),
                beatmapset_ids: self
                    .updates
                    .selection
                    .cached_missing_sets
                    .iter()
                    .filter(|beatmap| beatmap.selected && beatmap.collection_id == *collection_id)
                    .map(|beatmap| beatmap.id)
                    .collect(),
            })
            .collect();

        let request = SelectiveDownloadRequest {
            collection_ids,
            beatmapset_ids,
            collections,
            config,
            snapshot_dir: snapshots::snapshots_dir(),
            snapshots,
        };

        Some((id, request))
    }

    /// Run `mutate` against the home form, then — only when focus is the
    /// collection field AND its value actually changed — return a
    /// `ResolveCollectionUrl` command carrying the new value.
    ///
    /// No-op keystrokes (backspace on an empty field, digits typed into the
    /// threads input) thus do not spawn a wasted resolve task.
    fn mutate_collection_then_resolve(
        &mut self,
        mutate: impl FnOnce(&mut HomeTab),
    ) -> Option<AppCommand> {
        let before = if self.home.focus == HomeField::Collection {
            Some(self.home.collection.value.clone())
        } else {
            None
        };
        mutate(&mut self.home);
        let before = before?;
        if self.home.focus != HomeField::Collection || self.home.collection.value == before {
            return None;
        }
        Some(AppCommand::ResolveCollectionUrl {
            value: self.home.collection.value.clone(),
        })
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<AppCommand> {
        // ctrl+c always quits unconditionally
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Some(AppCommand::Quit);
        }

        let is_quit_key = matches!(key.code, KeyCode::Char('q') | KeyCode::Esc);
        if self.home.quit_prompt && !is_quit_key {
            self.home.quit_prompt = false;
        }

        match key.code {
            KeyCode::Char('?') => {
                self.help_open = !self.help_open;
                return None;
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.close_modal() {
                    return None;
                }
                if self.active_tab() == UPDATES_TAB_INDEX && self.updates.handle_escape().is_some()
                {
                    return None;
                }
                return self.handle_quit_key();
            }
            KeyCode::Left => {
                if !self.updates_list_open()
                    && let Some(cmd) = self.prev_tab()
                {
                    return Some(cmd);
                }
            }
            KeyCode::Right => {
                if !self.updates_list_open()
                    && let Some(cmd) = self.next_tab()
                {
                    return Some(cmd);
                }
            }
            KeyCode::Tab => {
                // When focused on the directory input on the home tab, Tab
                // runs filesystem completion instead of switching tabs.
                if self.active_tab() == HOME_TAB_INDEX && self.home.focus == HomeField::Directory {
                    self.home.tab_complete_directory();
                    return None;
                }
                if !self.updates_list_open()
                    && let Some(cmd) = self.next_tab()
                {
                    return Some(cmd);
                }
            }
            KeyCode::BackTab => {
                if !self.updates_list_open()
                    && let Some(cmd) = self.prev_tab()
                {
                    return Some(cmd);
                }
            }
            KeyCode::Up => {
                if self.active_tab() == UPDATES_TAB_INDEX
                    && (self.updates.selection.in_collection_list
                        || self.updates.selection.in_beatmap_list)
                {
                    self.updates.scroll_up();
                } else if let Some(page) = self.active_download_page_mut() {
                    page.scroll_threads_up();
                } else {
                    self.focus_prev_field();
                }
            }
            KeyCode::Down => {
                if self.active_tab() == UPDATES_TAB_INDEX
                    && (self.updates.selection.in_collection_list
                        || self.updates.selection.in_beatmap_list)
                {
                    self.updates.scroll_down();
                } else if let Some(page) = self.active_download_page_mut() {
                    page.scroll_threads_down();
                } else {
                    self.focus_next_field();
                }
            }
            KeyCode::Enter => {
                if self.active_tab() == HOME_TAB_INDEX
                    && let Some((id, request)) = self.request_download()
                {
                    return Some(AppCommand::StartDownload { id, request });
                }
                if self.active_tab() == UPDATES_TAB_INDEX {
                    let in_list = self.updates.selection.in_collection_list
                        || self.updates.selection.in_beatmap_list;
                    if in_list {
                        return None;
                    }
                    // enter opens list panels; only attempt a download when no panel was opened
                    if self.updates.enter_opens_list() {
                        return None;
                    }
                    if !self.updates.is_scan_ready() {
                        return None;
                    }
                    match self.updates.handle_enter() {
                        UpdatesAction::Download => {
                            if let Some((id, request)) = self.request_selective_download() {
                                return Some(AppCommand::StartSelectiveDownload { id, request });
                            }
                        }
                        UpdatesAction::RecheckFailedMaps => {
                            return Some(AppCommand::RecheckFailedMaps);
                        }
                        UpdatesAction::None | UpdatesAction::RefreshAll => {}
                    }
                }
                if self.active_tab() == CONFIG_TAB_INDEX {
                    match self.config.focus {
                        ConfigField::LoginEntry => return self.request_login(),
                        ConfigField::LogoutEntry => return self.request_logout(),
                        _ => {}
                    }
                }
            }
            KeyCode::Char(' ') => match self.active_tab() {
                HOME_TAB_INDEX => {
                    if matches!(
                        self.home.focus,
                        HomeField::AutoOverwrite
                            | HomeField::MirrorNerinyan
                            | HomeField::MirrorOsuDirect
                            | HomeField::MirrorSayobot
                            | HomeField::MirrorNekoha
                            | HomeField::NoVideo
                    ) {
                        self.home.toggle_current();
                    } else if let Some(cmd) =
                        self.mutate_collection_then_resolve(|h| h.handle_char(' '))
                    {
                        return Some(cmd);
                    }
                }
                UPDATES_TAB_INDEX => match self.updates.toggle_current() {
                    UpdatesAction::RefreshAll => return Some(AppCommand::ScanLocalDatabase),
                    UpdatesAction::RecheckFailedMaps => {
                        return Some(AppCommand::RecheckFailedMaps);
                    }
                    UpdatesAction::None | UpdatesAction::Download => {}
                },
                CONFIG_TAB_INDEX => match self.config.focus {
                    ConfigField::LoginEntry | ConfigField::LogoutEntry => {}
                    field if field.is_text_input() => self.config.handle_char(' '),
                    _ => self.config.toggle_current(),
                },
                _ => {}
            },
            KeyCode::Char(ch) => match self.active_tab() {
                HOME_TAB_INDEX => {
                    // Stepper: +/- adjust thread count when threads field is focused.
                    if self.home.focus.is_stepper() {
                        match ch {
                            '+' => {
                                self.home.step_up();
                                return None;
                            }
                            '-' => {
                                self.home.step_down();
                                return None;
                            }
                            _ => {}
                        }
                    }
                    // `r` refreshes mirror latency when no text input is focused.
                    if ch == 'r' && !self.home.focus.is_text_input() {
                        return Some(AppCommand::ProbeMirrors);
                    }
                    if let Some(cmd) = self.mutate_collection_then_resolve(|h| h.handle_char(ch)) {
                        return Some(cmd);
                    }
                }
                UPDATES_TAB_INDEX => {
                    let in_list = self.updates.selection.in_collection_list
                        || self.updates.selection.in_beatmap_list;
                    // suppress global letter shortcuts when the osu! path text field is focused
                    let suppress_shortcuts = self.updates.is_typing() && !in_list;
                    if !suppress_shortcuts && ch == 'r' && self.updates.can_recheck_failed_maps() {
                        return Some(AppCommand::RecheckFailedMaps);
                    }
                    self.updates.handle_char(ch);
                }
                CONFIG_TAB_INDEX => {
                    let focus = self.config.focus;
                    // Stepper: +/- adjust thread count when threads field is focused.
                    if focus.is_stepper() {
                        match ch {
                            '+' => {
                                self.config.step_up();
                                return None;
                            }
                            '-' => {
                                self.config.step_down();
                                return None;
                            }
                            _ => {}
                        }
                    }
                    let is_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                    if !is_ctrl && !focus.is_text_input() {
                        match ch {
                            's' => self.try_save_config(),
                            _ => self.config.handle_char(ch),
                        }
                    } else {
                        self.config.handle_char(ch);
                    }
                }
                _ => {}
            },
            KeyCode::Backspace => match self.active_tab() {
                HOME_TAB_INDEX => {
                    if let Some(cmd) = self.mutate_collection_then_resolve(HomeTab::backspace) {
                        return Some(cmd);
                    }
                }
                UPDATES_TAB_INDEX => self.updates.backspace(),
                CONFIG_TAB_INDEX => self.config.backspace(),
                _ => {}
            },
            _ => {}
        }

        None
    }

    pub fn clear_expired_messages(&mut self) {
        clear_expired_message(&mut self.home.message);
        clear_expired_message(&mut self.updates.message);
        clear_expired_message(&mut self.config.message);
        self.tick_count = self.tick_count.wrapping_add(1);
    }

    pub fn handle_download_event(&mut self, event: DownloadEvent) {
        match event {
            DownloadEvent::CollectionReady {
                id,
                collection_name,
                uploader,
                total_maps,
                output_dir,
            } => {
                if let Some(page) = self.page_mut(id) {
                    page.set_title(collection_name.clone());
                    page.uploader = Some(uploader);
                    page.total_maps = total_maps;
                    page.download_target = total_maps;
                    page.output_dir = Some(output_dir);
                    page.stage = DownloadStage::Downloading;
                    page.push_log("Collection fetched");
                }
            }
            DownloadEvent::ResolveProgress { id, current, total } => {
                if let Some(page) = self.page_mut(id) {
                    page.resolve_progress = Some((current, total));
                }
            }
            DownloadEvent::CollectionSizeResolved { id, total_bytes } => {
                if let Some(page) = self.page_mut(id) {
                    page.stats.total_collection_bytes = Some(total_bytes);
                }
            }
            DownloadEvent::LowDiskSpace {
                id,
                available_bytes,
            } => {
                if let Some(page) = self.page_mut(id) {
                    page.low_disk_space = Some(available_bytes);
                }
            }
            DownloadEvent::VerifiedMapSizes { id, total_bytes } => {
                if let Some(page) = self.page_mut(id) {
                    page.stats.verified_bytes += total_bytes;
                }
            }
            DownloadEvent::BeatmapsRegistered { id, beatmap_ids } => {
                if let Some(page) = self.page_mut(id) {
                    page.register_beatmaps(&beatmap_ids);
                }
            }
            DownloadEvent::BeatmapProgress {
                id,
                beatmapset_id,
                downloaded,
                total,
            } => {
                if let Some(page) = self.page_mut(id) {
                    page.update_progress(beatmapset_id, downloaded, total);
                    page.update_active_progress(beatmapset_id, downloaded, total);
                }
            }
            DownloadEvent::BeatmapStatus {
                id,
                beatmapset_id,
                stage,
                message,
                rate_limited,
            } => {
                if let Some(page) = self.page_mut(id) {
                    page.update_status(beatmapset_id, stage, &message);
                    page.update_active_status(beatmapset_id, stage, &message, rate_limited);
                }
            }
            DownloadEvent::DownloadTarget { id, remaining } => {
                if let Some(page) = self.page_mut(id) {
                    page.download_target = remaining;
                }
            }
            DownloadEvent::OverallProgress {
                id,
                downloaded,
                skipped,
                failed,
                unverified,
            } => {
                if let Some(page) = self.page_mut(id) {
                    page.stats.downloaded = downloaded;
                    page.stats.skipped = skipped;
                    page.stats.failed = failed;
                    page.stats.unverified = unverified;
                }
            }
            DownloadEvent::Log { id, message } => {
                if let Some(page) = self.page_mut(id) {
                    page.push_log(&message);
                }
            }
            DownloadEvent::StageChanged { id, stage } => {
                if let Some(page) = self.page_mut(id) {
                    if page.stage != stage {
                        page.indeterminate_anim_start.set(None);
                    }
                    page.stage = stage;
                    if matches!(stage, DownloadStage::Completed | DownloadStage::Failed) {
                        page.clear_active_downloads();
                    }
                }
            }
            DownloadEvent::BeatmapVerified { id, duration_us } => {
                if let Some(page) = self.page_mut(id) {
                    page.stats.verify_total_count = page.stats.verify_total_count.saturating_add(1);
                    page.stats.verify_total_us =
                        page.stats.verify_total_us.saturating_add(duration_us);
                }
            }
            DownloadEvent::FailedMaps { id, failures } => {
                if let Some(page) = self.page_mut(id) {
                    page.set_failed_maps(failures);
                }
            }
            DownloadEvent::Finished { id, summary } => {
                if let Some(page) = self.page_mut(id) {
                    page.stage = DownloadStage::Completed;
                    page.summary = Some(summary);
                    page.push_log("Download finished");
                }
            }
            DownloadEvent::Failed { id, message } => {
                if let Some(page) = self.page_mut(id) {
                    page.stage = DownloadStage::Failed;
                    page.push_log(&format!("Error: {message}"));
                    page.summary = None;
                    page.clear_active_downloads();
                }
            }
        }
    }

    pub fn tab_titles(&self) -> Vec<Cow<'_, str>> {
        let mut titles = Vec::with_capacity(self.downloads.len() + STATIC_TABS);
        titles.push(Cow::Borrowed(TAB_HOME_LOWER));
        titles.push(Cow::Borrowed(TAB_UPDATES_LOWER));
        titles.push(Cow::Borrowed(TAB_CONFIG_LOWER));
        for page in &self.downloads {
            titles.push(Cow::Borrowed(page.title_lower()));
        }
        titles
    }

    pub fn download_for_tab(&self, tab_index: usize) -> Option<&CollectionPage> {
        if tab_index < STATIC_TABS {
            None
        } else {
            self.downloads.get(tab_index - STATIC_TABS)
        }
    }

    pub fn active_download_page_mut(&mut self) -> Option<&mut CollectionPage> {
        if self.active_tab < STATIC_TABS {
            None
        } else {
            self.downloads.get_mut(self.active_tab - STATIC_TABS)
        }
    }

    pub fn handle_cancel_result(&mut self, download_id: DownloadId, was_running: bool) {
        let title = self.remove_download_page(download_id);
        self.active_tab = 0;
        self.home.quit_prompt = false;

        let display = title.unwrap_or_else(|| format!("download #{download_id}"));
        if was_running {
            set_info_message(
                &mut self.home.message,
                format!("Cancelled download \"{}\"", display),
            );
        } else {
            set_info_message(
                &mut self.home.message,
                format!("No active download to cancel for \"{}\"", display),
            );
        }
    }

    fn page_mut(&mut self, id: DownloadId) -> Option<&mut CollectionPage> {
        self.downloads.iter_mut().find(|page| page.id == id)
    }

    fn remove_download_page(&mut self, download_id: DownloadId) -> Option<String> {
        if let Some(position) = self
            .downloads
            .iter()
            .position(|page| page.id == download_id)
        {
            let title = self.downloads[position].title.clone();
            self.downloads.remove(position);
            Some(title)
        } else {
            None
        }
    }

    fn handle_quit_key(&mut self) -> Option<AppCommand> {
        if self.active_tab() < STATIC_TABS {
            if self.home.quit_prompt {
                self.home.quit_prompt = false;
                return Some(AppCommand::Quit);
            }

            self.home.quit_prompt = true;
            debug!("Quit requested; showing confirmation prompt");
            return None;
        }

        self.home.quit_prompt = false;
        self.cancel_command_for_active_tab()
    }

    fn cancel_command_for_active_tab(&mut self) -> Option<AppCommand> {
        if self.active_tab < STATIC_TABS {
            return None;
        }

        let idx = self.active_tab - STATIC_TABS;
        if let Some(page) = self.downloads.get(idx) {
            return Some(AppCommand::CancelDownload { id: page.id });
        }

        self.active_tab = 0;
        None
    }

    fn placeholder_title(input: &str, download_id: DownloadId) -> String {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return format!("collection {download_id}");
        }

        match utils::parse_collection_id(trimmed) {
            Ok(collection_id) => format!("collection {collection_id}"),
            Err(_) => format!("collection {trimmed}"),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/app_state.rs"]
mod tests;
