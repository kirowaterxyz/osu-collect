use super::{Mirror, OSU_API_MIN_REQUEST_INTERVAL};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
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
    /// Timestamp of the last osu! official ([`MirrorKind::OsuApi`]) request,
    /// shared across every concurrent worker so the proactive interval cannot be
    /// bypassed by concurrency. `None` until the first such request. Held under
    /// an async mutex because the gate sleeps while holding it (serializing the
    /// stamp); other mirrors never touch it.
    osu_api_last_request: Arc<AsyncMutex<Option<Instant>>>,
}

impl MirrorPool {
    pub(crate) fn new(mirrors: Vec<Mirror>) -> Self {
        Self {
            mirrors: Arc::new(mirrors),
            penalties: Arc::new(Mutex::new(HashMap::new())),
            osu_api_last_request: Arc::new(AsyncMutex::new(None)),
        }
    }

    /// Proactively space out osu! official requests to at most one per
    /// [`OSU_API_MIN_REQUEST_INTERVAL`]. Call once per [`MirrorKind::OsuApi`]
    /// attempt before issuing any HTTP request; the caller gates on the kind so
    /// other mirrors never reach it. Sleeps while holding the timestamp lock so
    /// concurrent workers queue behind it rather than all firing at once, then
    /// stamps the release time as the new "last request".
    pub(crate) async fn throttle_osu_api(&self) {
        let mut last = self.osu_api_last_request.lock().await;
        if let Some(prev) = *last {
            let elapsed = prev.elapsed();
            if elapsed < OSU_API_MIN_REQUEST_INTERVAL {
                sleep(OSU_API_MIN_REQUEST_INTERVAL - elapsed).await;
            }
        }
        *last = Some(Instant::now());
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
