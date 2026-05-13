use super::{
    CleanupTracker, DownloadConfig, DownloadError, DownloadEvent, DownloadHandle, DownloadId,
    DownloadRequest, DownloadStage, DownloadSummary, SelectiveDownloadRequest, ShutdownToken,
    http_client,
    lock::ActiveDownloadRegistry,
    passes::{FailureReport, PassCoordinator},
    session::{DownloadSession, PipelineFlavor, PrepareCollectionParams, PrepareSelectiveParams},
    status_helpers::{
        fail_status, finished_status, log_status, low_disk_space_status, stage_status,
    },
};
use crate::{
    app::snapshots,
    config::constants::DEFAULT_PROGRESS_WATCHDOG_SECS,
    core::collection::{create_collection_db, model::Collection},
    mirrors::{MirrorEndpoint, MirrorPool},
    utils::{AppError, check_available_space, is_low_disk_space},
    worker::{DownloadContext, DownloadContextConfig, StatusSink},
};
use dashmap::DashSet;
use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::Duration,
};
use tokio::fs;
use tokio::sync::mpsc::UnboundedSender;
use tracing::Instrument;
use tracing::{error, info, info_span, warn};

static DOWNLOAD_REGISTRY: LazyLock<ActiveDownloadRegistry> =
    LazyLock::new(ActiveDownloadRegistry::new);

fn spawn_download_task<F, Fut>(
    id: DownloadId,
    span: tracing::Span,
    tx: UnboundedSender<DownloadEvent>,
    runner: F,
) -> DownloadHandle
where
    F: FnOnce(ShutdownToken, StatusSink) -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), DownloadError>> + Send,
{
    let shutdown = ShutdownToken::new();
    let shutdown_worker = shutdown.clone();
    let handle_token = shutdown.clone();
    let status = StatusSink::from_sender(tx);
    let failure_status = status.clone();

    let join_handle = tokio::spawn(
        async move {
            info!("Download task started");
            match runner(shutdown_worker.clone(), status).await {
                Ok(()) => {
                    shutdown_worker.mark_completed();
                    info!("Download task completed");
                }
                Err(err) => {
                    shutdown_worker.mark_completed();
                    error!(error = %err, "Download task failed");
                    fail_status(&failure_status, id, err.to_string());
                }
            }
        }
        .instrument(span),
    );

    DownloadHandle {
        shutdown: handle_token,
        join_handle,
    }
}

pub fn spawn_download(
    id: DownloadId,
    request: DownloadRequest,
    tx: UnboundedSender<DownloadEvent>,
) -> DownloadHandle {
    let mirror_count = request.config.mirrors.len();
    let concurrent = request.config.concurrent;
    let span = info_span!(
        "download_task",
        download_id = id,
        mirror_count = mirror_count,
        concurrent = concurrent
    );
    {
        let _guard = span.enter();
        info!(
            collection_input = %request.collection_input,
            target_directory = %request.config.directory,
            skip_existing = request.skip_existing,
            auto_overwrite = request.auto_overwrite,
            "Spawning download task"
        );
    }

    spawn_download_task(id, span, tx, move |shutdown, status| async move {
        run_download(id, request, shutdown, status).await
    })
}

pub fn spawn_selective_download(
    id: DownloadId,
    request: SelectiveDownloadRequest,
    tx: UnboundedSender<DownloadEvent>,
) -> DownloadHandle {
    let mirror_count = request.config.mirrors.len();
    let concurrent = request.config.concurrent;
    let beatmapset_count = request.beatmapset_ids.len();
    let span = info_span!(
        "selective_download_task",
        download_id = id,
        mirror_count = mirror_count,
        concurrent = concurrent,
        beatmapset_count = beatmapset_count
    );
    {
        let _guard = span.enter();
        info!(
            target_directory = %request.config.directory,
            collection_count = request.collection_ids.len(),
            "Spawning selective download task"
        );
    }

    spawn_download_task(id, span, tx, move |shutdown, status| async move {
        run_selective_download(id, request, shutdown, status).await
    })
}

struct RunDownloadCoreParams {
    session: DownloadSession,
    shutdown: ShutdownToken,
    mirrors: Vec<MirrorEndpoint>,
    concurrent: u8,
    skip_existing: bool,
    auto_overwrite: bool,
    verify_zip_eocd: bool,
    flavor: PipelineFlavor,
}

struct BuildContextParams {
    id: DownloadId,
    thread_count: usize,
    skip_existing: bool,
    auto_overwrite: bool,
    verify_zip_eocd: bool,
    client: reqwest::Client,
    shutdown: ShutdownToken,
    mirrors: Vec<MirrorEndpoint>,
    tracker: super::BeatmapTracker,
    output_dir: PathBuf,
    initial_unverified: Arc<DashSet<u32>>,
    status: StatusSink,
}

fn build_download_context(params: BuildContextParams) -> Result<DownloadContext, DownloadError> {
    validate_mirrors(&params.mirrors)?;
    let progress_watchdog = Duration::from_secs(DEFAULT_PROGRESS_WATCHDOG_SECS);

    Ok(DownloadContext::new(DownloadContextConfig {
        id: params.id,
        thread_count: params.thread_count,
        skip_existing: params.skip_existing,
        auto_overwrite: params.auto_overwrite,
        verify_zip_eocd: params.verify_zip_eocd,
        shutdown: params.shutdown,
        client: params.client,
        mirror_pool: MirrorPool::new(params.mirrors.into_iter().map(|m| m.to_mirror()).collect()),
        output_dir: params.output_dir,
        tracker: params.tracker,
        initial_unverified: params.initial_unverified,
        status: params.status,
        progress_watchdog,
    }))
}

async fn run_download_pass(
    ctx: &DownloadContext,
    totals: &mut DownloadSummary,
    log_prefix: &str,
) -> FailureReport {
    let mut final_failures = FailureReport::default();

    if ctx.shutdown.is_cancelled() {
        return final_failures;
    }

    let pending = ctx.tracker.pending_snapshot();
    if pending.is_empty() {
        return final_failures;
    }

    info!(
        download_id = ctx.id,
        remaining = pending.len(),
        thread_count = ctx.thread_count,
        "{} download pass",
        log_prefix
    );

    let pass_outcome = PassCoordinator::new(ctx.clone(), totals).run(pending).await;

    if pass_outcome.aborted {
        warn!("{} aborted during pass", log_prefix);
    } else if let Some(summary) = pass_outcome.failures.describe_top_failure() {
        ctx.emit(DownloadEvent::Log {
            id: ctx.id,
            message: format!("Most common failure: {}", summary),
        });
    }

    for (beatmapset_id, reason) in pass_outcome.failures.beatmaps() {
        final_failures.record_error(*beatmapset_id, reason.clone());
    }

    final_failures
}

async fn run_download_core(params: RunDownloadCoreParams) -> Result<(), DownloadError> {
    let RunDownloadCoreParams {
        session,
        shutdown,
        mirrors,
        concurrent,
        skip_existing,
        auto_overwrite,
        verify_zip_eocd,
        flavor,
    } = params;

    let DownloadSession {
        id,
        status,
        target,
        beatmapset_ids,
        output,
        tracker,
        mut totals,
        initial_unverified,
        _lock_guard,
    } = session;
    let _session_lock = _lock_guard;

    log_status(&status, id, "Fetching collection size from Nekoha...");
    let api_client = http_client::api_client()?;
    let size_result =
        super::size_fetcher::fetch_beatmapset_sizes(&api_client, &beatmapset_ids).await;
    status.emit(DownloadEvent::CollectionSizeResolved {
        id,
        total_bytes: size_result.total_bytes,
    });
    if size_result.missing_count > 0 {
        log_status(
            &status,
            id,
            format!(
                "Size info unavailable for {} beatmapsets",
                size_result.missing_count
            ),
        );
    }

    check_and_warn_low_disk_space(&status, id, &output.output_dir);

    let download_client = http_client::download_client()?;
    let thread_count = concurrent.max(1) as usize;
    let ctx = build_download_context(BuildContextParams {
        id,
        thread_count,
        skip_existing,
        auto_overwrite,
        verify_zip_eocd,
        client: download_client,
        shutdown: shutdown.clone(),
        mirrors,
        tracker,
        output_dir: output.output_dir.clone(),
        initial_unverified,
        status: status.clone(),
    })?;

    let failure_report = run_download_pass(&ctx, &mut totals, flavor.log_prefix).await;
    ctx.tracker.clear_validation_cache();

    if abort_if_shutdown(
        &status,
        id,
        &shutdown,
        &ctx,
        &failure_report,
        flavor.abort_log_message,
    )
    .await
    {
        if let Some(warning) = flavor.abort_warning {
            warn!("{}", warning);
        }
        return Ok(());
    }

    if failure_report.is_empty() {
        let collection = target.collection().clone();
        let output_dir = ctx.output_dir.as_ref().as_path().to_path_buf();
        let db_result = tokio::task::spawn_blocking(move || {
            create_collection_database(&collection, &output_dir)
        })
        .await
        .map_err(|e| {
            AppError::other_dynamic(format!("spawn_blocking panicked: {e}").into_boxed_str())
        })
        .and_then(|r| r);
        match db_result {
            Ok(()) => {
                log_status(&status, id, "collection.db created successfully");
                info!("collection.db created successfully");
            }
            Err(e) => {
                let message = format!("failed to create collection.db: {e}");
                log_status(&status, id, message.clone());
                error!(error = %e, "failed to create collection.db");
                return Err(DownloadError::internal(message));
            }
        }
    }

    summarize_failed_maps(&status, id, &failure_report, flavor.failure_summary);

    finished_status(&status, id, &totals);
    stage_status(&status, id, DownloadStage::Completed);
    info!("{}", flavor.completion_log);
    Ok(())
}

async fn abort_if_shutdown(
    status: &StatusSink,
    id: DownloadId,
    shutdown: &ShutdownToken,
    ctx: &DownloadContext,
    failures: &FailureReport,
    log_message: Option<&str>,
) -> bool {
    if !shutdown.is_cancelled() {
        return false;
    }

    handle_shutdown_cleanup(
        status,
        id,
        failures,
        &ctx.cleanup_tracker,
        ctx.output_dir.as_ref().as_path(),
    )
    .await;
    if let Some(message) = log_message {
        log_status(status, id, message);
    }
    fail_status(status, id, "Download aborted by user");
    true
}

fn summarize_failed_maps(
    status: &StatusSink,
    id: DownloadId,
    failures: &FailureReport,
    summary_message: &str,
) {
    if failures.is_empty() {
        return;
    }

    emit_failed_maps(status, id, failures);
    warn!(count = failures.beatmaps().len(), "{}", summary_message);
}

async fn handle_shutdown_cleanup(
    status: &StatusSink,
    id: DownloadId,
    failures: &FailureReport,
    cleanup_tracker: &CleanupTracker,
    output_dir: &Path,
) {
    emit_failed_maps(status, id, failures);
    let cleanup_outcome = cleanup_tracker.cleanup_incomplete().await;
    if cleanup_outcome.removed > 0 {
        info!(
            removed = cleanup_outcome.removed,
            "Removed incomplete beatmap archives"
        );
        log_status(
            status,
            id,
            format!(
                "Cleaned up {} incomplete beatmap archives",
                cleanup_outcome.removed
            ),
        );
    }
    for (path, message) in &cleanup_outcome.failures {
        warn!(target = %path.display(), error = %message, "Failed to cleanup file");
        log_status(
            status,
            id,
            format!("Cleanup warning for {}: {}", path.display(), message),
        );
    }

    match try_remove_empty_output_dir(output_dir).await {
        Ok(()) => {
            info!(dir = %output_dir.display(), "Removed empty output directory");
            log_status(
                status,
                id,
                format!("Removed empty directory {}", output_dir.display()),
            );
        }
        Err(DownloadError::DirectoryNotEmpty) => {}
        Err(err) => {
            warn!(dir = %output_dir.display(), error = %err, "Failed to remove output directory");
        }
    }
}

async fn try_remove_empty_output_dir(output_dir: &Path) -> Result<(), DownloadError> {
    let mut entries = fs::read_dir(output_dir).await?;

    if entries.next_entry().await?.is_some() {
        return Err(DownloadError::DirectoryNotEmpty);
    }

    fs::remove_dir(output_dir).await?;
    Ok(())
}

fn validate_mirrors(mirrors: &[MirrorEndpoint]) -> Result<(), DownloadError> {
    if mirrors.is_empty() {
        warn!("Download request did not include any mirrors");
        return Err(DownloadError::NoMirrors);
    }
    Ok(())
}

fn check_and_warn_low_disk_space(status: &StatusSink, id: DownloadId, output_dir: &Path) {
    if is_low_disk_space(output_dir)
        && let Some(available) = check_available_space(output_dir)
    {
        warn!(
            available_bytes = available,
            output_dir = %output_dir.display(),
            "Low disk space detected"
        );
        low_disk_space_status(status, id, available);
    }
}

fn emit_failed_maps(status: &StatusSink, id: DownloadId, failures: &FailureReport) {
    if failures.is_empty() {
        return;
    }

    status.emit(DownloadEvent::FailedMaps {
        id,
        failures: failures.beatmaps().to_vec(),
    });
}

fn create_collection_database(collection: &Collection, output_dir: &Path) -> Result<(), AppError> {
    let db_collection_name = format!("{}-{}", collection.name, collection.id);
    create_collection_db(collection, &db_collection_name, output_dir)
}

async fn run_download(
    id: DownloadId,
    request: DownloadRequest,
    shutdown: ShutdownToken,
    status: StatusSink,
) -> Result<(), DownloadError> {
    let DownloadRequest {
        collection_input,
        config,
        skip_existing,
        auto_overwrite,
    } = request;
    let DownloadConfig {
        directory,
        mirrors,
        concurrent,
        verify_zip_eocd,
        ..
    } = config;

    info!(
        collection_input = %collection_input,
        concurrent,
        mirror_count = mirrors.len(),
        skip_existing,
        auto_overwrite,
        "Running download pipeline"
    );

    stage_status(&status, id, DownloadStage::Resolving);
    let flavor = PipelineFlavor::collection();
    let thread_count = concurrent.max(1) as usize;

    let session = DownloadSession::prepare_collection(PrepareCollectionParams {
        id,
        status: status.clone(),
        shutdown: &shutdown,
        directory: &directory,
        collection_input: &collection_input,
        thread_count,
        verify_zip_eocd,
        flavor: &flavor,
        registry: &DOWNLOAD_REGISTRY,
    })
    .await?;

    let Some(session) = session else {
        return Ok(());
    };

    run_download_core(RunDownloadCoreParams {
        session,
        shutdown,
        mirrors,
        concurrent,
        skip_existing,
        auto_overwrite,
        verify_zip_eocd,
        flavor,
    })
    .await
}

async fn run_selective_download(
    id: DownloadId,
    request: SelectiveDownloadRequest,
    shutdown: ShutdownToken,
    status: StatusSink,
) -> Result<(), DownloadError> {
    let SelectiveDownloadRequest {
        collection_ids,
        beatmapset_ids,
        config,
        snapshot_dir,
        snapshots: collection_snapshots,
    } = request;
    let DownloadConfig {
        directory,
        mirrors,
        concurrent,
        verify_zip_eocd,
        ..
    } = config;

    info!(
        collection_count = collection_ids.len(),
        beatmapset_count = beatmapset_ids.len(),
        concurrent,
        mirror_count = mirrors.len(),
        "Running selective download pipeline"
    );

    if beatmapset_ids.is_empty() {
        return Err(DownloadError::NoBeatmapsets);
    }

    stage_status(&status, id, DownloadStage::Resolving);
    let flavor = PipelineFlavor::selective();
    let thread_count = concurrent.max(1) as usize;

    let session = DownloadSession::prepare_selective(PrepareSelectiveParams {
        id,
        status: status.clone(),
        shutdown: &shutdown,
        directory: &directory,
        collection_ids: &collection_ids,
        beatmapset_ids: &beatmapset_ids,
        thread_count,
        verify_zip_eocd,
        flavor: &flavor,
        registry: &DOWNLOAD_REGISTRY,
    })
    .await?;

    let Some(session) = session else {
        return Ok(());
    };

    let tracker = session.tracker.clone();
    run_download_core(RunDownloadCoreParams {
        session,
        shutdown,
        mirrors,
        concurrent,
        skip_existing: true,
        auto_overwrite: false,
        verify_zip_eocd,
        flavor,
    })
    .await?;

    if beatmapset_ids.iter().all(|id| tracker.is_verified(*id))
        && let Some(snapshot_dir) = snapshot_dir
    {
        tokio::task::spawn_blocking(move || {
            for snapshot in collection_snapshots {
                let Ok(collection_id) = snapshot.collection_id.parse() else {
                    continue;
                };
                snapshots::save(
                    &snapshot,
                    &snapshots::snapshot_path(&snapshot_dir, collection_id),
                );
            }
        })
        .await
        .map_err(|err| DownloadError::internal(format!("snapshot save task panicked: {err}")))?;
    }

    Ok(())
}
