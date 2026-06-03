mod auth;
mod mirror_probe;
mod resolve;
mod scan;

pub use mirror_probe::{MirrorProbeEvent, ProbeResult, probe_url};
pub use scan::{
    FetchCompareSettings, FetchMissingResult, UpdatesEvent, collection_ids_for_scan,
    fetch_missing_beatmapsets, read_local_database, should_hide_failed_beatmapset,
    snapshot_diffs_for_scan,
};

pub use resolve::{HomeResolveEvent, handle_home_resolve_event};

use super::{
    App, AppCommand,
    messages::{set_error_message, set_info_message},
};
use crate::{
    app::failed_maps,
    config::Config,
    config::constants::HOME_TAB_INDEX,
    download::{self, DownloadEvent, DownloadHandle, DownloadId},
    tui::terminal::{cleanup_terminal, setup_terminal, spawn_input_thread},
    tui::{draw, init_theme},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{debug, info, trace, warn};

use auth::{AuthEvent, handle_auth_event, spawn_login_task, spawn_logout_task};
use mirror_probe::{handle_mirror_probe_event, schedule_probe};
use resolve::schedule_resolve;
use scan::{handle_updates_event, spawn_failed_map_recheck_task, spawn_scan_task};

pub async fn run(
    config: Config,
    startup_notice: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting application runtime loop");
    init_theme(config.display.theme);
    let validation_issue = config.validate().err().map(|e| e.to_string());
    let mut terminal = setup_terminal()?;
    let mut app = App::new(config);
    let mut notice = startup_notice;
    if let Some(msg) = validation_issue {
        warn!(error = %msg, "Configuration validation failed; surfacing to UI");
        if let Some(ref notice_text) = notice {
            set_error_message(&mut app.home.message, format!("{msg}\n{notice_text}"));
            notice = None;
        } else {
            set_error_message(&mut app.home.message, msg);
        }
    }
    if let Some(message) = notice.take() {
        set_info_message(&mut app.home.message, message);
    }

    let (download_tx, mut download_rx) = mpsc::unbounded_channel::<DownloadEvent>();
    let (updates_tx, mut updates_rx) = mpsc::unbounded_channel::<UpdatesEvent>();
    let (auth_tx, mut auth_rx) = mpsc::unbounded_channel::<AuthEvent>();
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<InputEvent>();
    let (home_resolve_tx, mut home_resolve_rx) = mpsc::unbounded_channel::<HomeResolveEvent>();
    let (mirror_probe_tx, mut mirror_probe_rx) = mpsc::unbounded_channel::<MirrorProbeEvent>();
    let input_handle = spawn_input_thread(input_tx.clone());

    let mut should_quit = false;
    let mut active_downloads: HashMap<DownloadId, DownloadHandle> = HashMap::new();
    let mut tasks = BackgroundTasks {
        login: None,
        resolve: None,
        resolve_cancel: None,
        home_resolve_tx,
        mirror_probe: None,
        mirror_probe_cancel: None,
        mirror_probe_tx: mirror_probe_tx.clone(),
    };

    // Home-tab startup work: probe mirror latency, and resolve the pre-filled
    // collection value (restored from the last run) so its status shows without
    // the user touching the field. `schedule_resolve` parses + debounces, so a
    // non-parseable prefill just clears.
    if app.active_tab == HOME_TAB_INDEX {
        schedule_probe(
            &mut tasks.mirror_probe,
            &mut tasks.mirror_probe_cancel,
            &tasks.mirror_probe_tx,
        );
        if !app.home.collection.value.trim().is_empty() {
            schedule_resolve(
                &app.home.collection.value,
                &mut tasks.resolve,
                &mut tasks.resolve_cancel,
                &tasks.home_resolve_tx,
            );
        }
    }

    while !should_quit {
        terminal.draw(|f| draw(f, &app))?;

        tokio::select! {
            Some(event) = download_rx.recv() => {
                trace!(?event, "Received download event");
                persist_failed_maps(&event);
                if let Some(completed_id) = download_finished_id(&event) {
                    debug!(download_id = completed_id, "Download handle finished; awaiting join");
                    if let Some(handle) = active_downloads.remove(&completed_id) {
                        tokio::spawn(async move {
                            handle.wait().await;
                        });
                    }
                }
                app.handle_download_event(event);
            }
            Some(event) = updates_rx.recv() => {
                trace!(?event, "Received updates event");
                handle_updates_event(event, &mut app, &updates_tx);
            }
            Some(event) = auth_rx.recv() => {
                trace!(?event, "Received auth event");
                if matches!(event, AuthEvent::LoginComplete(_)) {
                    tasks.login = None;
                }
                handle_auth_event(event, &mut app);
            }
            Some(input) = input_rx.recv() => {
                trace!(?input, "Processing input event");
                should_quit = handle_input(
                    input,
                    &mut app,
                    &download_tx,
                    &updates_tx,
                    &auth_tx,
                    &mut active_downloads,
                    &mut tasks,
                );
            }
            Some(event) = home_resolve_rx.recv() => {
                trace!(?event, "Received home resolve event");
                handle_home_resolve_event(event, &mut app.home);
            }
            Some(event) = mirror_probe_rx.recv() => {
                trace!(?event, "Received mirror probe event");
                handle_mirror_probe_event(event, &mut app.home);
            }
            else => break,
        }
    }

    if let Some(handle) = tasks.login.take() {
        handle.abort();
    }
    if let Some(handle) = tasks.resolve.take() {
        handle.abort();
    }
    if let Some(handle) = tasks.mirror_probe.take() {
        handle.abort();
    }

    app.home.quit_prompt = false;
    set_info_message(&mut app.home.message, "Quitting...");
    terminal.draw(|f| draw(f, &app))?;

    drop(download_rx);
    drop(updates_rx);
    drop(input_rx);
    signal_abort_downloads(&mut active_downloads);
    abort_and_wait_downloads(&mut active_downloads).await;

    drop(input_tx);
    if let Some(handle) = input_handle {
        let _ = handle.join();
    }
    cleanup_terminal(&mut terminal)?;

    Ok(())
}

fn handle_input(
    input: InputEvent,
    app: &mut App,
    download_tx: &mpsc::UnboundedSender<DownloadEvent>,
    updates_tx: &mpsc::UnboundedSender<UpdatesEvent>,
    auth_tx: &mpsc::UnboundedSender<AuthEvent>,
    downloads: &mut HashMap<DownloadId, DownloadHandle>,
    tasks: &mut BackgroundTasks,
) -> bool {
    match input {
        InputEvent::Key(key) => {
            handle_key_event(key, app, download_tx, updates_tx, auth_tx, downloads, tasks)
        }
        InputEvent::Resize => false,
        InputEvent::Tick => {
            app.clear_expired_messages();
            false
        }
    }
}

fn handle_key_event(
    key: KeyEvent,
    app: &mut App,
    download_tx: &mpsc::UnboundedSender<DownloadEvent>,
    updates_tx: &mpsc::UnboundedSender<UpdatesEvent>,
    auth_tx: &mpsc::UnboundedSender<AuthEvent>,
    downloads: &mut HashMap<DownloadId, DownloadHandle>,
    tasks: &mut BackgroundTasks,
) -> bool {
    trace!(?key, "Handling key event");
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        warn!("CTRL+C received; signalling abort for all downloads");
        signal_abort_downloads(downloads);
        return true;
    }

    match app.handle_key(key) {
        Some(AppCommand::StartDownload { id, request }) => {
            let handle = download::spawn_download(id, request, download_tx.clone());
            info!(download_id = id, "Spawned download from UI request");
            downloads.insert(id, handle);
        }
        Some(AppCommand::StartSelectiveDownload { id, request }) => {
            let handle = download::spawn_selective_download(id, request, download_tx.clone());
            info!(
                download_id = id,
                "Spawned selective download from Updates tab"
            );
            downloads.insert(id, handle);
        }
        Some(AppCommand::CancelDownload { id }) => {
            let was_running = if let Some(handle) = downloads.remove(&id) {
                handle.request_shutdown();
                tokio::spawn(async move {
                    handle.wait().await;
                });
                info!(download_id = id, "Requested shutdown for active download");
                true
            } else {
                false
            };
            app.handle_cancel_result(id, was_running);
        }
        Some(AppCommand::Login {
            client_id,
            client_secret,
        }) => {
            if let Some(prev) = tasks.login.take() {
                prev.abort();
            }
            tasks.login = Some(spawn_login_task(client_id, client_secret, auth_tx.clone()));
        }
        Some(AppCommand::CancelLogin) => {
            if let Some(handle) = tasks.login.take() {
                handle.abort();
                info!("Login cancelled by user");
            }
        }
        Some(AppCommand::Logout) => {
            spawn_logout_task(auth_tx.clone());
        }
        Some(AppCommand::ResolveCollectionUrl { value }) => {
            schedule_resolve(
                &value,
                &mut tasks.resolve,
                &mut tasks.resolve_cancel,
                &tasks.home_resolve_tx,
            );
        }
        Some(AppCommand::ProbeMirrors) => {
            schedule_probe(
                &mut tasks.mirror_probe,
                &mut tasks.mirror_probe_cancel,
                &tasks.mirror_probe_tx,
            );
        }
        Some(AppCommand::ScanLocalDatabase) => {
            spawn_scan_task(app, updates_tx.clone());
        }
        Some(AppCommand::RecheckFailedMaps) => {
            spawn_failed_map_recheck_task(app, updates_tx.clone());
        }
        Some(AppCommand::RetryFailedMap {
            download_id,
            beatmapset_id,
        }) => {
            if let Some((new_id, request)) =
                app.start_retry_download(download_id, vec![beatmapset_id])
            {
                let handle =
                    download::spawn_selective_download(new_id, request, download_tx.clone());
                info!(
                    source_download_id = download_id,
                    retry_download_id = new_id,
                    beatmapset_id,
                    "Spawned single-map retry download"
                );
                downloads.insert(new_id, handle);
            }
        }
        Some(AppCommand::RetryAllFailed { download_id }) => {
            let retryable_ids = app
                .downloads
                .iter()
                .find(|p| p.id == download_id)
                .map(|p| p.retryable_ids(None))
                .unwrap_or_default();
            if !retryable_ids.is_empty()
                && let Some((new_id, request)) =
                    app.start_retry_download(download_id, retryable_ids)
            {
                let handle =
                    download::spawn_selective_download(new_id, request, download_tx.clone());
                info!(
                    source_download_id = download_id,
                    retry_download_id = new_id,
                    "Spawned retry-all download"
                );
                downloads.insert(new_id, handle);
            }
        }
        Some(AppCommand::FocusOutputDir) => {
            app.focus_output_dir();
        }
        Some(AppCommand::Quit) => {
            if downloads.is_empty() {
                info!("No downloads active; exiting application");
            } else {
                info!("Quit confirmed; aborting downloads and exiting");
            }
            signal_abort_downloads(downloads);
            return true;
        }
        None => {}
    }

    false
}

fn persist_failed_maps(event: &DownloadEvent) {
    let DownloadEvent::FailedMaps { failures, .. } = event else {
        return;
    };
    if failures.is_empty() {
        return;
    }
    let Some(path) = failed_maps::failed_maps_path() else {
        warn!("failed maps path unavailable");
        return;
    };
    let ids: Vec<u32> = failures.iter().map(|f| f.beatmapset_id).collect();
    tokio::task::spawn_blocking(move || failed_maps::record_failures(&path, ids));
}

fn download_finished_id(event: &DownloadEvent) -> Option<DownloadId> {
    match event {
        DownloadEvent::Finished { id, .. } => Some(*id),
        DownloadEvent::Failed { id, .. } => Some(*id),
        _ => None,
    }
}

fn signal_abort_downloads(downloads: &mut HashMap<DownloadId, DownloadHandle>) {
    if downloads.is_empty() {
        return;
    }
    warn!(
        active = downloads.len(),
        "Signalling shutdown for active downloads"
    );
    for handle in downloads.values() {
        handle.request_shutdown();
    }
}

async fn abort_and_wait_downloads(downloads: &mut HashMap<DownloadId, DownloadHandle>) {
    if downloads.is_empty() {
        return;
    }

    warn!(
        remaining = downloads.len(),
        "Awaiting graceful shutdown for downloads"
    );
    for handle in downloads.values() {
        handle.request_shutdown();
    }

    let mut pending: Vec<DownloadHandle> = downloads.drain().map(|(_, handle)| handle).collect();
    for handle in pending.drain(..) {
        debug!("Waiting for download task to complete");
        handle.wait().await;
    }
}

#[derive(Clone, Debug)]
pub enum InputEvent {
    Key(KeyEvent),
    Resize,
    Tick,
}

/// Background task handles and their associated channels, kept by the runtime loop.
struct BackgroundTasks {
    login: Option<tokio::task::JoinHandle<()>>,
    resolve: Option<tokio::task::JoinHandle<()>>,
    resolve_cancel: Option<tokio::sync::watch::Sender<bool>>,
    home_resolve_tx: mpsc::UnboundedSender<HomeResolveEvent>,
    mirror_probe: Option<tokio::task::JoinHandle<()>>,
    mirror_probe_cancel: Option<tokio::sync::watch::Sender<bool>>,
    mirror_probe_tx: mpsc::UnboundedSender<MirrorProbeEvent>,
}
