use osu_downloader::MirrorKind;
use reqwest::Client;
use std::time::{Duration, Instant};
use tokio::{sync::mpsc, sync::watch};

const PROBE_TIMEOUT_MS: u64 = 1500;

/// Result of probing a single mirror's latency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeResult {
    /// Round-trip completed; latency in milliseconds.
    Ms(u32),
    /// Request timed out.
    Timeout,
    /// Connection or other network error.
    Error,
}

/// Events emitted by the mirror probe task.
#[derive(Debug)]
pub enum MirrorProbeEvent {
    /// Probe started for all mirrors; show `…` for each.
    Started,
    /// Result for a single mirror.
    Result {
        kind: MirrorKind,
        result: ProbeResult,
    },
}

/// Base URL used for HEAD probing each built-in mirror.
///
/// These are the scheme+host roots — safe to HEAD without authentication and
/// without substituting a beatmapset ID. Each is the root path, not a
/// download endpoint, so no ID substitution is needed.
pub fn probe_url(kind: MirrorKind) -> Option<&'static str> {
    match kind {
        // api.nerinyan.moe root returns a JSON status page
        MirrorKind::Nerinyan => Some("https://api.nerinyan.moe/"),
        // osu.direct root responds quickly
        MirrorKind::OsuDirect => Some("https://osu.direct/"),
        // Sayobot serves the download tree from dl.sayobot.cn
        MirrorKind::Sayobot => Some("https://dl.sayobot.cn/"),
        // Nekoha root at mirror.nekoha.moe
        MirrorKind::Nekoha => Some("https://mirror.nekoha.moe/"),
        MirrorKind::Custom => None,
    }
}

/// Abort any in-flight probe task and start a new one.
///
/// Does nothing if a probe is already in flight (i.e. `handle` is `Some`).
/// Returns `true` if a new probe was spawned, `false` if one was skipped.
pub fn schedule_probe(
    handle: &mut Option<tokio::task::JoinHandle<()>>,
    cancel_tx: &mut Option<watch::Sender<bool>>,
    tx: &mpsc::UnboundedSender<MirrorProbeEvent>,
) -> bool {
    // Skip if a probe is already running.
    if handle.as_ref().is_some_and(|h| !h.is_finished()) {
        return false;
    }

    // Cancel any stale finished handle.
    if let Some(prev) = handle.take() {
        prev.abort();
    }
    if let Some(prev_tx) = cancel_tx.take() {
        let _ = prev_tx.send(true);
    }

    let (new_cancel_tx, cancel_rx) = watch::channel(false);
    *cancel_tx = Some(new_cancel_tx);

    let tx = tx.clone();
    let new_handle = tokio::spawn(async move {
        run_probe_all(cancel_rx, tx).await;
    });
    *handle = Some(new_handle);
    true
}

async fn run_probe_all(
    cancel_rx: watch::Receiver<bool>,
    tx: mpsc::UnboundedSender<MirrorProbeEvent>,
) {
    let _ = tx.send(MirrorProbeEvent::Started);

    let client = Client::builder()
        .timeout(Duration::from_millis(PROBE_TIMEOUT_MS))
        .build()
        .unwrap_or_default();

    // Probe all four built-in mirrors in parallel.
    let (r_osu_direct, r_nerinyan, r_sayobot, r_nekoha) = tokio::join!(
        probe_one(&client, MirrorKind::OsuDirect),
        probe_one(&client, MirrorKind::Nerinyan),
        probe_one(&client, MirrorKind::Sayobot),
        probe_one(&client, MirrorKind::Nekoha),
    );

    // Bail if cancelled while probes were running.
    if *cancel_rx.borrow() {
        return;
    }

    for (kind, result) in [
        (MirrorKind::OsuDirect, r_osu_direct),
        (MirrorKind::Nerinyan, r_nerinyan),
        (MirrorKind::Sayobot, r_sayobot),
        (MirrorKind::Nekoha, r_nekoha),
    ] {
        let _ = tx.send(MirrorProbeEvent::Result { kind, result });
    }
}

async fn probe_one(client: &Client, kind: MirrorKind) -> ProbeResult {
    let Some(url) = probe_url(kind) else {
        return ProbeResult::Error;
    };

    let start = Instant::now();
    match client.head(url).send().await {
        Ok(_) => {
            let ms = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
            ProbeResult::Ms(ms)
        }
        Err(err) if err.is_timeout() => ProbeResult::Timeout,
        Err(_) => ProbeResult::Error,
    }
}

/// Apply a `MirrorProbeEvent` to the home tab's latency map.
pub fn handle_mirror_probe_event(event: MirrorProbeEvent, home: &mut crate::app::HomeTab) {
    match event {
        MirrorProbeEvent::Started => {
            home.mirror_probe_started();
        }
        MirrorProbeEvent::Result { kind, result } => {
            home.set_mirror_latency(kind, result);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/mirror_probe.rs"]
mod tests;
