use super::{Mirror, MirrorKind};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Mirror penalty pool shared across concurrent workers.
#[doc(hidden)]
#[derive(Clone)]
pub struct MirrorPool {
    mirrors: Arc<Vec<Mirror>>,
    penalties: Arc<Mutex<HashMap<MirrorKind, Instant>>>,
}

impl MirrorPool {
    /// Create a new pool from a list of mirrors.
    #[doc(hidden)]
    pub fn new(mirrors: Vec<Mirror>) -> Self {
        Self {
            mirrors: Arc::new(mirrors),
            penalties: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn mirrors_len(&self) -> usize {
        self.mirrors.len()
    }

    /// Record a rate-limit penalty for the given mirror kind.
    #[doc(hidden)]
    pub fn mark_rate_limited(&self, kind: MirrorKind) {
        let mut penalties = self.penalties.lock().unwrap();
        penalties.insert(kind, Instant::now() + kind.rate_limit_backoff());
    }

    /// Return the remaining penalty duration for the given mirror kind.
    #[doc(hidden)]
    pub fn penalty_remaining(&self, kind: MirrorKind) -> Option<Duration> {
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
