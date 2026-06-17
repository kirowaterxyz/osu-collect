use super::MirrorPool;
use crate::mirrors::OSU_API_MIN_REQUEST_INTERVAL;
use crate::{Mirror, MirrorKind};
use std::thread::sleep;
use std::time::Duration;

#[test]
fn rate_limit_records_penalty() {
    let pool = MirrorPool::new(vec![Mirror::nerinyan()]);
    pool.mark_rate_limited(0);
    assert!(pool.penalty_remaining(0).is_some());
}

#[test]
fn penalty_self_clears_after_deadline() {
    let pool = MirrorPool::new(vec![Mirror::nerinyan()]);
    let backoff = MirrorKind::Nerinyan.rate_limit_backoff();
    pool.mark_rate_limited(0);
    sleep(backoff * 3);
    assert!(pool.penalty_remaining(0).is_none());
}

#[test]
fn second_mark_does_not_extend_active_penalty() {
    let pool = MirrorPool::new(vec![Mirror::nerinyan()]);
    let backoff = MirrorKind::Nerinyan.rate_limit_backoff();
    pool.mark_rate_limited(0);
    sleep(backoff / 2);
    pool.mark_rate_limited(0);
    let after = pool
        .penalty_remaining(0)
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
    pool.mark_rate_limited(0);
    assert!(pool.penalty_remaining(0).is_some());
    assert!(pool.penalty_remaining(1).is_none());
}

#[test]
fn penalties_are_independent_across_custom_mirrors() {
    // Two custom mirrors share `MirrorKind::Custom`; the per-slot key must keep
    // their cooldowns separate so a 429 on one does not sideline the other.
    let pool = MirrorPool::new(vec![
        Mirror::custom("https://a.example/d/{id}").unwrap(),
        Mirror::custom("https://b.example/d/{id}").unwrap(),
    ]);
    pool.mark_rate_limited(0);
    assert!(pool.penalty_remaining(0).is_some());
    assert!(pool.penalty_remaining(1).is_none());
}

#[test]
fn penalty_remaining_for_out_of_range_index_is_none() {
    let pool = MirrorPool::new(vec![Mirror::nerinyan()]);
    pool.mark_rate_limited(5);
    assert!(pool.penalty_remaining(5).is_none());
}

#[test]
fn osu_api_throttle_interval_is_one_second() {
    // The proactive osu! official limiter targets ~60 req/min.
    assert_eq!(OSU_API_MIN_REQUEST_INTERVAL, Duration::from_secs(1));
}

#[tokio::test]
async fn first_osu_api_throttle_does_not_block() {
    // With no prior request stamped, the gate must return immediately — only
    // the *second* call within the interval waits.
    let pool = MirrorPool::new(vec![Mirror::osu_api()]);
    let start = std::time::Instant::now();
    pool.throttle_osu_api().await;
    assert!(
        start.elapsed() < OSU_API_MIN_REQUEST_INTERVAL,
        "the first osu! API request must not be delayed"
    );
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
