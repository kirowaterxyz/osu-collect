use super::{
    collection::CollectionPage,
    collection_state::{self, CollectionStateFile},
    config::{AuthLoginState, ConfigField, ConfigTab},
    failed_maps,
    home::{HomeField, HomeTab},
    messages::{clear_expired_message, set_error_message, set_info_message},
    snapshots,
    updates::{UpdatesAction, UpdatesTab, extract_collection_id},
};
use crate::{
    config::{
        Config, RetryFailedOnDownload,
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
use std::collections::HashSet;
use std::path::{Path, PathBuf};
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
    /// Pending confirmation for "retry N failed maps?" when count > 50.
    pub confirm_retry: Option<RetryAllConfirmModal>,
    /// Pre-download prompt: previously failed beatmapsets in
    /// `failed-beatmapsets.json` intersect with the collection the user just
    /// submitted. Surfaces only when the config is `Ask`.
    pub confirm_retry_on_start: Option<RetryOnStartModal>,
    /// Override for the on-disk failed-maps file, set by tests. Production
    /// callers always pass `None` and the path is resolved at use-site.
    pub(crate) failed_maps_path_override: Option<PathBuf>,
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
    /// Retry a single failed beatmapset from a download page.
    RetryFailedMap {
        download_id: DownloadId,
        beatmapset_id: u32,
    },
    /// Retry all retryable failed maps for a download page (excludes NotFound).
    RetryAllFailed {
        download_id: DownloadId,
    },
    /// Collection URL field changed; schedule a debounced metadata resolve.
    ResolveCollectionUrl {
        value: String,
    },
    /// Probe latency for all built-in mirrors.
    ProbeMirrors,
    Quit,
}

/// State for the "retry N failed maps?" confirm modal shown when `R` is pressed
/// with more than 50 retryable failures.
#[derive(Debug)]
pub struct RetryAllConfirmModal {
    pub download_id: DownloadId,
    pub retryable_count: usize,
}

/// State for the pre-download retry prompt. Surfaces under `Ask` when
/// previously failed beatmaps for this collection are persisted on disk.
///
/// `enter` proceeds with retry, `n` proceeds without, `esc` cancels.
/// The pending request is dispatched on `enter`/`n` (with the
/// `include_previously_failed` flag set accordingly) and discarded on `esc`.
#[derive(Debug)]
pub struct RetryOnStartModal {
    pub id: DownloadId,
    pub failed_count: usize,
    pub pending: DownloadRequest,
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
            confirm_retry: None,
            confirm_retry_on_start: None,
            failed_maps_path_override: None,
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
        if self.confirm_retry_on_start.is_some() {
            self.cancel_retry_on_start();
            return true;
        }
        if self.confirm_retry.is_some() {
            self.confirm_retry = None;
            return true;
        }
        if self.help_open {
            self.help_open = false;
            return true;
        }
        false
    }

    /// Whether any modal is currently blocking input.
    pub fn any_modal_open(&self) -> bool {
        self.help_open || self.confirm_retry.is_some() || self.confirm_retry_on_start.is_some()
    }

    /// Cancel a pending pre-download retry prompt. Drops the queued page that
    /// `request_download` allocated for the prospective download so the tab
    /// list returns to its prior shape.
    fn cancel_retry_on_start(&mut self) {
        let Some(modal) = self.confirm_retry_on_start.take() else {
            return;
        };
        self.remove_download_page(modal.id);
        self.active_tab = HOME_TAB_INDEX;
        set_info_message(&mut self.home.message, "download cancelled");
    }

    fn focus_next_field(&mut self) {
        match self.active_tab() {
            HOME_TAB_INDEX => {
                self.home.close_dropdown();
                self.home.next_field();
                if self.home.focus == HomeField::Collection {
                    self.home.open_dropdown();
                }
            }
            UPDATES_TAB_INDEX => self.updates.next_field(),
            CONFIG_TAB_INDEX => self.config.next_field(),
            _ => {}
        }
    }

    fn focus_prev_field(&mut self) {
        match self.active_tab() {
            HOME_TAB_INDEX => {
                self.home.close_dropdown();
                self.home.prev_field();
                if self.home.focus == HomeField::Collection {
                    self.home.open_dropdown();
                }
            }
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
        let mut request = match self.home.build_request(self.config.archive_validation) {
            Ok(request) => request,
            Err(err) => {
                set_error_message(&mut self.home.message, err);
                return None;
            }
        };

        if self.downloads.len() >= usize::MAX - 1 {
            set_error_message(&mut self.home.message, "Too many downloads queued");
            return None;
        }

        let collection_id = utils::parse_collection_id(request.collection_input.trim()).ok();
        let failed_count = collection_id
            .map(|id| self.previously_failed_count(id))
            .unwrap_or(0);

        // No prior failures for this collection — skip the modal entirely.
        if failed_count == 0 {
            return Some(self.queue_download(request));
        }

        match self.config.retry_failed_on_download {
            RetryFailedOnDownload::Yes => {
                request.include_previously_failed = true;
                Some(self.queue_download(request))
            }
            RetryFailedOnDownload::No => {
                request.include_previously_failed = false;
                Some(self.queue_download(request))
            }
            RetryFailedOnDownload::Ask => {
                let id = self.next_download_id;
                self.next_download_id += 1;
                self.confirm_retry_on_start = Some(RetryOnStartModal {
                    id,
                    failed_count,
                    pending: request,
                });
                None
            }
        }
    }

    /// Allocate a `CollectionPage` for `request` and return the id + request
    /// to dispatch to the pipeline.
    fn queue_download(&mut self, request: DownloadRequest) -> (DownloadId, DownloadRequest) {
        let id = self.next_download_id;
        self.next_download_id += 1;
        self.push_pending_page(id, &request);
        set_info_message(&mut self.home.message, format!("Queued download #{id}"));
        (id, request)
    }

    /// Allocate a `CollectionPage` for an id reserved earlier by the retry
    /// prompt and dispatch the queued download.
    fn dispatch_pending(
        &mut self,
        id: DownloadId,
        request: DownloadRequest,
    ) -> (DownloadId, DownloadRequest) {
        self.push_pending_page(id, &request);
        set_info_message(&mut self.home.message, format!("Queued download #{id}"));
        (id, request)
    }

    fn push_pending_page(&mut self, id: DownloadId, request: &DownloadRequest) {
        let placeholder_title = Self::placeholder_title(&request.collection_input, id);
        let concurrent = usize::from(request.config.concurrent.max(1));
        let mut page = CollectionPage::new(id, placeholder_title, concurrent);
        page.stage = DownloadStage::Resolving;
        page.download_config = Some(request.config.clone());
        self.downloads.push(page);
        self.active_tab = STATIC_TABS + self.downloads.len() - 1;
    }

    /// Count beatmaps in `failed-beatmapsets.json` that belong to
    /// `collection_id`. The persisted file is not collection-scoped, so we
    /// pull the resolved id list from the `HomeTab` auto-resolve cache and
    /// intersect.
    ///
    /// Returns 0 when:
    /// - the failed-maps file path is unavailable, OR
    /// - no resolved collection metadata is cached for `collection_id` (the
    ///   user hit `enter` before the 300 ms debounce fired). Suppressing the
    ///   prompt in that case matches "no prior context to compare" — the
    ///   pipeline will retry persisted failures in its normal flow.
    fn previously_failed_count(&self, collection_id: u32) -> usize {
        let path = self
            .failed_maps_path_override
            .clone()
            .or_else(failed_maps::failed_maps_path);
        let Some(path) = path else { return 0 };

        let Some((cached_id, ids)) = self.home.resolved_collection.as_ref() else {
            return 0;
        };
        if *cached_id != collection_id {
            return 0;
        }

        let resolved_set: HashSet<u32> = ids.iter().copied().collect();
        intersect_failed_ids(&path, &resolved_set).len()
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
        // config is stored after it is built below; we'll set it there
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

        // store config snapshot for potential retry
        if let Some(page) = self.downloads.last_mut() {
            page.download_config = Some(config.clone());
        }

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

        // Pre-download retry prompt intercepts enter/n/esc.
        // `enter` dispatches the queued request with retry enabled; `n`
        // dispatches without; `esc` cancels the whole download.
        if self.confirm_retry_on_start.is_some() {
            match key.code {
                KeyCode::Enter => {
                    let modal = self.confirm_retry_on_start.take()?;
                    let mut request = modal.pending;
                    request.include_previously_failed = true;
                    let (id, request) = self.dispatch_pending(modal.id, request);
                    return Some(AppCommand::StartDownload { id, request });
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    let modal = self.confirm_retry_on_start.take()?;
                    let mut request = modal.pending;
                    request.include_previously_failed = false;
                    let (id, request) = self.dispatch_pending(modal.id, request);
                    return Some(AppCommand::StartDownload { id, request });
                }
                KeyCode::Esc => {
                    self.cancel_retry_on_start();
                    return None;
                }
                _ => return None,
            }
        }

        // Confirm-retry modal intercepts enter/esc/q and nothing else.
        if let Some(modal) = &self.confirm_retry {
            let download_id = modal.download_id;
            match key.code {
                KeyCode::Enter => {
                    self.confirm_retry = None;
                    return Some(AppCommand::RetryAllFailed { download_id });
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.confirm_retry = None;
                    return None;
                }
                _ => return None,
            }
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
                if self.active_tab() == HOME_TAB_INDEX
                    && self.home.focus == HomeField::Collection
                    && self.home.dropdown_open
                {
                    self.home.close_dropdown();
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
                if self.active_tab() == HOME_TAB_INDEX
                    && self.home.focus == HomeField::Collection
                    && self.home.dropdown_open
                {
                    self.home.dropdown_prev();
                } else if self.active_tab() == UPDATES_TAB_INDEX
                    && (self.updates.selection.in_collection_list
                        || self.updates.selection.in_beatmap_list)
                {
                    self.updates.scroll_up();
                } else if let Some(page) = self.active_download_page_mut() {
                    if page.failed_section_expanded && !page.failed_maps.is_empty() {
                        page.failed_focus_prev();
                    } else {
                        page.scroll_threads_up();
                    }
                } else {
                    self.focus_prev_field();
                }
            }
            KeyCode::Down => {
                if self.active_tab() == HOME_TAB_INDEX
                    && self.home.focus == HomeField::Collection
                    && self.home.dropdown_open
                {
                    self.home.dropdown_next();
                } else if self.active_tab() == UPDATES_TAB_INDEX
                    && (self.updates.selection.in_collection_list
                        || self.updates.selection.in_beatmap_list)
                {
                    self.updates.scroll_down();
                } else if let Some(page) = self.active_download_page_mut() {
                    if page.failed_section_expanded && !page.failed_maps.is_empty() {
                        page.failed_focus_next();
                    } else {
                        page.scroll_threads_down();
                    }
                } else {
                    self.focus_next_field();
                }
            }
            KeyCode::Enter => {
                if self.active_tab() == HOME_TAB_INDEX
                    && self.home.focus == HomeField::Collection
                    && self.home.dropdown_open
                {
                    if let Some(url) = self.home.dropdown_accept() {
                        return Some(AppCommand::ResolveCollectionUrl { value: url });
                    }
                    return None;
                }
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
                _ => {
                    if let Some(page) = self.active_download_page_mut() {
                        page.toggle_failed_section();
                    }
                }
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
                _ => {
                    if let Some(cmd) = self.handle_download_tab_key(ch) {
                        return Some(cmd);
                    }
                }
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
                cooldown_until,
            } => {
                if let Some(page) = self.page_mut(id) {
                    page.update_status(beatmapset_id, stage, &message);
                    page.update_active_status(
                        beatmapset_id,
                        stage,
                        &message,
                        rate_limited,
                        cooldown_until,
                    );
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
                    // auto-expand only the first time failures appear — if the
                    // user manually collapsed the section, don't reopen it on
                    // a follow-up batch
                    let was_empty = page.failed_maps.is_empty();
                    page.set_failed_maps(failures);
                    if was_empty && !page.failed_maps.is_empty() {
                        page.failed_section_expanded = true;
                    }
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

    /// Handle `r`/`R` on the active download tab. Letter suppression never
    /// applies here (there are no text inputs on download pages).
    /// Allocate a new download page for a retry batch and return the ID + request.
    /// Returns `None` if the source page is missing or has no stored config.
    ///
    /// The output directory is reused from the original download so the files
    /// land in the same folder without requiring a new resolve step.
    pub fn start_retry_download(
        &mut self,
        source_download_id: DownloadId,
        ids: Vec<u32>,
    ) -> Option<(DownloadId, SelectiveDownloadRequest)> {
        let page = self.downloads.iter().find(|p| p.id == source_download_id)?;
        let config = page.download_config.clone()?;
        let output_dir = page
            .output_dir
            .clone()
            .unwrap_or_else(|| config.directory.clone());

        if self.downloads.len() >= usize::MAX - 1 {
            return None;
        }

        let retry_config = DownloadConfig {
            directory: output_dir,
            mirrors: config.mirrors.clone(),
            concurrent: config.concurrent,
            archive_validation: config.archive_validation,
        };

        let new_id = self.next_download_id;
        self.next_download_id += 1;

        let title = format!("retry #{source_download_id}");
        let concurrent = usize::from(retry_config.concurrent.max(1));
        let mut retry_page = CollectionPage::new(new_id, title.clone(), concurrent);
        retry_page.stage = DownloadStage::Resolving;
        retry_page.download_config = Some(retry_config.clone());
        self.downloads.push(retry_page);
        self.active_tab = STATIC_TABS + self.downloads.len() - 1;

        let request = SelectiveDownloadRequest {
            collection_ids: vec![],
            beatmapset_ids: ids.clone(),
            collections: vec![SelectiveDownloadCollection {
                id: 0,
                name: title,
                beatmapset_ids: ids,
            }],
            config: retry_config,
            snapshot_dir: None,
            snapshots: vec![],
        };
        Some((new_id, request))
    }

    fn handle_download_tab_key(&mut self, ch: char) -> Option<AppCommand> {
        let page = self.active_download_page_mut()?;
        match ch {
            'r' => {
                // retry focused row; skip NotFound silently
                let focused = page.failed_focus?;
                let ids = page.retryable_ids(Some(focused));
                if ids.is_empty() {
                    return None;
                }
                let beatmapset_id = ids[0];
                let download_id = page.id;
                page.remove_failed_map(beatmapset_id);
                Some(AppCommand::RetryFailedMap {
                    download_id,
                    beatmapset_id,
                })
            }
            'R' => {
                let retryable = page.retryable_ids(None);
                if retryable.is_empty() {
                    return None;
                }
                let download_id = page.id;
                let count = retryable.len();
                if count > 50 {
                    self.confirm_retry = Some(RetryAllConfirmModal {
                        download_id,
                        retryable_count: count,
                    });
                    None
                } else {
                    Some(AppCommand::RetryAllFailed { download_id })
                }
            }
            _ => None,
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

/// Load the persisted failed-maps file at `path` and intersect its ids with
/// `collection_ids`. Returns the intersection.
pub(crate) fn intersect_failed_ids(path: &Path, collection_ids: &HashSet<u32>) -> Vec<u32> {
    let file = failed_maps::load(path);
    file.beatmapset_ids
        .iter()
        .copied()
        .filter(|id| collection_ids.contains(id))
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/app_state.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/retry_keybind.rs"]
mod retry_keybind_tests;

#[cfg(test)]
#[path = "../../tests/unit/retry_on_download.rs"]
mod retry_on_download_tests;
