use super::{Mirror, MirrorKind};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(crate) struct MirrorPool {
    mirrors: Arc<Vec<Mirror>>,
    penalties: Arc<Mutex<HashMap<MirrorKind, Instant>>>,
}

impl MirrorPool {
    pub(crate) fn new(mirrors: Vec<Mirror>) -> Self {
        Self {
            mirrors: Arc::new(mirrors),
            penalties: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn mark_rate_limited(&self, kind: MirrorKind) {
        let mut penalties = self.penalties.lock().unwrap();
        penalties.insert(kind, Instant::now() + kind.rate_limit_backoff());
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
