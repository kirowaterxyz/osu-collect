use super::{Mirror, MirrorKind, OSU_API_MIN_REQUEST_INTERVAL};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::sleep;

pub(crate) struct MirrorPool {
    mirrors: Arc<Vec<Mirror>>,
    penalties: Arc<Mutex<HashMap<MirrorKind, Instant>>>,
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

    pub(crate) fn mark_rate_limited(&self, kind: MirrorKind) {
        let now = Instant::now();
        let mut penalties = self.penalties.lock().unwrap();
        if penalties.get(&kind).is_some_and(|&until| until > now) {
            return;
        }
        penalties.insert(kind, now + kind.rate_limit_backoff());
    }

    pub(crate) fn penalty_remaining(&self, kind: MirrorKind) -> Option<Duration> {
        let now = Instant::now();
        let penalties = self.penalties.lock().unwrap();
        penalties
            .get(&kind)
            .and_then(|&until| (until > now).then_some(until - now))
    }

    pub(crate) fn mirrors(&self) -> &[Mirror] {
        &self.mirrors
    }
}

#[cfg(test)]
#[path = "../../tests/pool.rs"]
mod tests;
