use super::Mirror;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::sleep;

pub(crate) struct MirrorPool {
    mirrors: Arc<Vec<Mirror>>,
    /// Rate-limit cooldowns keyed by **mirror index** (position in `mirrors`),
    /// not [`MirrorKind`](super::MirrorKind): two custom mirrors share the
    /// `Custom` kind but must back off independently, so a per-slot key is
    /// required.
    penalties: Arc<Mutex<HashMap<usize, Instant>>>,
    /// Per-slot last-request timestamps gating the proactive
    /// [`MirrorKind::min_request_interval`](super::MirrorKind::min_request_interval)
    /// spacing, keyed by **mirror index** so each mirror (including two customs
    /// sharing `Custom`) is spaced independently. Each slot carries its own
    /// async mutex: the gate sleeps while holding it so concurrent workers
    /// hitting the *same* mirror queue behind one another, while workers on
    /// *different* mirrors never block each other.
    request_gates: Vec<AsyncMutex<Option<Instant>>>,
    /// Monotonic counter handing each beatmapset download a distinct starting
    /// mirror slot, round-robining the initial mirror across maps so load
    /// spreads instead of every map hammering slot 0 first.
    round_robin: AtomicUsize,
}

impl MirrorPool {
    pub(crate) fn new(mirrors: Vec<Mirror>) -> Self {
        let request_gates = (0..mirrors.len()).map(|_| AsyncMutex::new(None)).collect();
        Self {
            mirrors: Arc::new(mirrors),
            penalties: Arc::new(Mutex::new(HashMap::new())),
            request_gates,
            round_robin: AtomicUsize::new(0),
        }
    }

    /// Proactively space requests to the mirror at `idx` to at most one per its
    /// kind's
    /// [`min_request_interval`](super::MirrorKind::min_request_interval) (100 ms
    /// for most mirrors, 1 s for the osu! official API). Call once per attempt
    /// before issuing the HTTP request. Sleeps while holding the slot's
    /// timestamp lock so concurrent workers on the same mirror queue rather than
    /// burst, then stamps the release time as the new "last request". Mirrors on
    /// different slots use different locks and never block each other. A no-op
    /// for an out-of-range `idx`.
    pub(crate) async fn throttle(&self, idx: usize) {
        let (Some(mirror), Some(gate)) = (self.mirrors.get(idx), self.request_gates.get(idx))
        else {
            return;
        };
        let interval = mirror.kind().min_request_interval();
        let mut last = gate.lock().await;
        if let Some(prev) = *last {
            let elapsed = prev.elapsed();
            if elapsed < interval {
                sleep(interval - elapsed).await;
            }
        }
        *last = Some(Instant::now());
    }

    /// Next round-robin starting slot for a beatmapset download. Monotonic; the
    /// caller takes it modulo the mirror count. Spreads the initial mirror
    /// across concurrent maps so they don't all start on slot 0.
    pub(crate) fn next_round_robin_start(&self) -> usize {
        self.round_robin.fetch_add(1, Ordering::Relaxed)
    }

    /// Mark the mirror at `idx` rate-limited, starting a cooldown derived from
    /// its kind's [`MirrorKind::rate_limit_backoff`]. Keyed per slot so custom
    /// mirrors back off independently.
    pub(crate) fn mark_rate_limited(&self, idx: usize) {
        let Some(mirror) = self.mirrors.get(idx) else {
            return;
        };
        let now = Instant::now();
        let mut penalties = self.penalties.lock().unwrap();
        if penalties.get(&idx).is_some_and(|&until| until > now) {
            return;
        }
        penalties.insert(idx, now + mirror.kind().rate_limit_backoff());
    }

    pub(crate) fn penalty_remaining(&self, idx: usize) -> Option<Duration> {
        let now = Instant::now();
        let penalties = self.penalties.lock().unwrap();
        penalties
            .get(&idx)
            .and_then(|&until| (until > now).then_some(until - now))
    }

    pub(crate) fn mirrors(&self) -> &[Mirror] {
        &self.mirrors
    }
}

#[cfg(test)]
#[path = "../../tests/pool.rs"]
mod tests;
