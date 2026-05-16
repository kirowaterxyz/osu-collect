use super::{Mirror, MirrorKind};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone)]
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

    pub(crate) fn plan(&self) -> Vec<Mirror> {
        let now = Instant::now();
        let mut penalties = self.penalties.lock().unwrap();
        let mut ready: Vec<Mirror> = Vec::with_capacity(self.mirrors.len());

        for mirror in self.mirrors.iter() {
            match penalties.get(&mirror.kind()).copied() {
                Some(until) if until > now => {}
                Some(_) => {
                    penalties.remove(&mirror.kind());
                    ready.push(mirror.clone());
                }
                None => ready.push(mirror.clone()),
            }
        }

        ready
    }

    pub(crate) fn mirrors_len(&self) -> usize {
        self.mirrors.len()
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
        assert!(pool.plan().is_empty());
        assert!(pool.penalty_remaining(MirrorKind::Nerinyan).is_some());
    }

    #[test]
    fn test_plan_excludes_cooling_mirrors_when_ready_exists() {
        let mirrors = vec![Mirror::nerinyan(), Mirror::osu_direct()];
        let pool = MirrorPool::new(mirrors);

        pool.mark_rate_limited(MirrorKind::Nerinyan);

        let plan = pool.plan();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].kind(), MirrorKind::OsuDirect);
    }

    #[test]
    fn test_plan_returns_no_mirrors_when_all_cooling() {
        let mirrors = vec![Mirror::nerinyan()];
        let pool = MirrorPool::new(mirrors);

        pool.mark_rate_limited(MirrorKind::Nerinyan);

        let plan = pool.plan();
        assert!(plan.is_empty());
        assert!(pool.penalty_remaining(MirrorKind::Nerinyan).is_some());
    }
}
