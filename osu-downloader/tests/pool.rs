use super::MirrorPool;
use crate::{Mirror, MirrorKind};
use std::thread::sleep;

#[test]
fn rate_limit_records_penalty() {
    let pool = MirrorPool::new(vec![Mirror::nerinyan()]);
    pool.mark_rate_limited(MirrorKind::Nerinyan);
    assert!(pool.penalty_remaining(MirrorKind::Nerinyan).is_some());
}

#[test]
fn penalty_self_clears_after_deadline() {
    let pool = MirrorPool::new(vec![Mirror::nerinyan()]);
    let backoff = MirrorKind::Nerinyan.rate_limit_backoff();
    pool.mark_rate_limited(MirrorKind::Nerinyan);
    sleep(backoff * 3);
    assert!(pool.penalty_remaining(MirrorKind::Nerinyan).is_none());
}

#[test]
fn second_mark_does_not_extend_active_penalty() {
    let pool = MirrorPool::new(vec![Mirror::nerinyan()]);
    let backoff = MirrorKind::Nerinyan.rate_limit_backoff();
    pool.mark_rate_limited(MirrorKind::Nerinyan);
    sleep(backoff / 2);
    pool.mark_rate_limited(MirrorKind::Nerinyan);
    let after = pool
        .penalty_remaining(MirrorKind::Nerinyan)
        .expect("penalty still active half a backoff in");
    // Remaining must reflect the *original* deadline (≈ backoff/2 left), not a fresh
    // one (≈ backoff left). The 3/4 boundary is safely between those two outcomes.
    assert!(
        after < backoff * 3 / 4,
        "second mark within the active window must not reset the deadline (after={after:?}, backoff={backoff:?})"
    );
}

#[test]
fn penalties_are_independent_across_mirrors() {
    let pool = MirrorPool::new(vec![Mirror::nerinyan(), Mirror::osu_direct()]);
    pool.mark_rate_limited(MirrorKind::Nerinyan);
    assert!(pool.penalty_remaining(MirrorKind::Nerinyan).is_some());
    assert!(pool.penalty_remaining(MirrorKind::OsuDirect).is_none());
}

#[test]
fn mirrors_preserves_order_and_duplicates() {
    let pool = MirrorPool::new(vec![
        Mirror::nerinyan(),
        Mirror::osu_direct(),
        Mirror::nerinyan(),
    ]);
    let kinds: Vec<MirrorKind> = pool.mirrors().iter().map(Mirror::kind).collect();
    assert_eq!(
        kinds,
        vec![
            MirrorKind::Nerinyan,
            MirrorKind::OsuDirect,
            MirrorKind::Nerinyan,
        ]
    );
}
