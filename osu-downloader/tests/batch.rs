use super::{BatchConfig, download_batch};
use crate::mirrors::pool::MirrorPool;
use crate::{ArchiveValidation, Mirror, OnExists};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

#[tokio::test]
async fn cancel_mid_batch_does_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let client = reqwest::Client::new();
    let mirror_pool = Arc::new(MirrorPool::new(vec![Mirror::nerinyan()]));
    let config = BatchConfig {
        concurrent_downloads: 2,
        archive_validation: ArchiveValidation::Off,
        progress_timeout: Duration::from_secs(1),
        network_retry_attempts: 0,
        sanitize_filenames: true,
        on_exists: OnExists::Skip,
        rate_limit_skip_after: None,
    };

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        let _ = cancel_tx.send(true);
    });

    let ids: Vec<u32> = (1u32..=5).collect();
    let summary = download_batch(
        ids,
        dir.path(),
        client,
        mirror_pool,
        config,
        event_tx,
        cancel_rx,
        std::sync::Arc::new(tokio::sync::Notify::new()),
    )
    .await;

    assert!(summary.downloaded.len() + summary.skipped.len() + summary.failed.len() <= 5);
}
