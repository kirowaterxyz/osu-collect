use osu_downloader::__test_exports::MirrorPool;
use osu_downloader::{Mirror, MirrorKind};

#[test]
fn rate_limit_records_penalty() {
    let pool = MirrorPool::new(vec![Mirror::nerinyan()]);
    pool.mark_rate_limited(MirrorKind::Nerinyan);
    assert!(pool.penalty_remaining(MirrorKind::Nerinyan).is_some());
}
