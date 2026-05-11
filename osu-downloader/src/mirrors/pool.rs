use super::{Mirror, MirrorKind};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Mirror pool for managing mirror selection and rate limiting
#[derive(Clone)]
pub struct MirrorPool {
    mirrors: Arc<Vec<Mirror>>,
    penalties: Arc<Mutex<HashMap<MirrorKind, Instant>>>,
}

impl MirrorPool {
    /// Create a new mirror pool
    pub fn new(mirrors: Vec<Mirror>) -> Self {
        Self {
            mirrors: Arc::new(mirrors),
            penalties: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Check if only a single mirror is configured
    pub fn is_single_mirror(&self) -> bool {
        self.mirrors.len() == 1
    }

    /// Get a prioritized list of mirrors for attempting downloads
    ///
    /// Returns mirrors in order: ready mirrors first, then cooling mirrors by soonest availability
    pub fn plan(&self) -> Vec<Mirror> {
        let now = Instant::now();
        let mut penalties = self.penalties.lock().unwrap();
        let mut ready: Vec<Mirror> = Vec::with_capacity(self.mirrors.len());
        let mut cooling: Vec<(Mirror, Instant)> = Vec::new();

        for mirror in self.mirrors.iter() {
            match penalties.get(&mirror.kind()).copied() {
                Some(until) if until > now => cooling.push((mirror.clone(), until)),
                Some(_) => {
                    penalties.remove(&mirror.kind());
                    ready.push(mirror.clone());
                }
                None => ready.push(mirror.clone()),
            }
        }

        drop(penalties);

        if ready.is_empty() {
            cooling.sort_by_key(|(_, until)| *until);
            return cooling.into_iter().map(|(mirror, _)| mirror).collect();
        }

        cooling.sort_by_key(|(_, until)| *until);
        ready.extend(cooling.into_iter().map(|(mirror, _)| mirror));
        ready
    }

    /// Get cooldown info if using a single mirror and it's rate limited
    pub fn single_mirror_cooldown(&self) -> Option<(Mirror, Duration)> {
        if !self.is_single_mirror() {
            return None;
        }

        let mirror = self.mirrors.first()?.clone();
        let now = Instant::now();
        let penalties = self.penalties.lock().unwrap();

        penalties.get(&mirror.kind()).and_then(|&until| {
            if until > now {
                Some((mirror, until - now))
            } else {
                None
            }
        })
    }

    /// Mark a mirror as rate limited
    pub fn mark_rate_limited(&self, kind: MirrorKind) {
        let mut penalties = self.penalties.lock().unwrap();
        penalties.insert(kind, Instant::now() + kind.rate_limit_backoff());
    }

    /// Clear rate limit penalty for a mirror
    pub fn clear_penalty(&self, kind: MirrorKind) {
        let mut penalties = self.penalties.lock().unwrap();
        penalties.remove(&kind);
    }

    /// Get remaining cooldown duration for a mirror
    pub fn penalty_remaining(&self, kind: MirrorKind) -> Option<Duration> {
        let now = Instant::now();
        let penalties = self.penalties.lock().unwrap();
        penalties
            .get(&kind)
            .and_then(|&until| (until > now).then_some(until - now))
    }

    #[cfg(test)]
    pub(crate) fn mirrors(&self) -> &[Mirror] {
        &self.mirrors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mirrors::CatboyRegion;

    #[test]
    fn test_mirror_pool_plan() {
        let mirrors = vec![
            Mirror::nerinyan(),
            Mirror::catboy(CatboyRegion::Us),
            Mirror::osu_direct(),
        ];
        let pool = MirrorPool::new(mirrors);

        let plan = pool.plan();
        assert_eq!(plan.len(), 3);
    }

    #[test]
    fn test_rate_limit() {
        let mirrors = vec![Mirror::nerinyan()];
        let pool = MirrorPool::new(mirrors);

        pool.mark_rate_limited(MirrorKind::Nerinyan);
        assert!(pool.penalty_remaining(MirrorKind::Nerinyan).is_some());

        pool.clear_penalty(MirrorKind::Nerinyan);
        assert!(pool.penalty_remaining(MirrorKind::Nerinyan).is_none());
    }
}
