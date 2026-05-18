use osu_downloader::{Error, Mirror, MirrorKind};

#[test]
fn builder_requires_at_least_one_mirror() {
    assert!(osu_downloader::Downloader::builder().build().is_err());
}

#[test]
fn builder_rejects_zero_concurrency() {
    let result = osu_downloader::Downloader::builder()
        .mirror(Mirror::nerinyan())
        .concurrent_downloads(0)
        .build();
    assert!(matches!(result, Err(Error::Config(_))));
}

#[test]
fn default_mirrors_include_every_builtin_mirror() {
    let downloader = osu_downloader::Downloader::builder()
        .default_mirrors()
        .build()
        .unwrap();

    let mirror_kinds: Vec<_> = downloader
        .mirror_pool_mirrors()
        .iter()
        .map(Mirror::kind)
        .collect();
    assert_eq!(
        mirror_kinds,
        vec![
            MirrorKind::Nerinyan,
            MirrorKind::OsuDirect,
            MirrorKind::Sayobot,
            MirrorKind::Nekoha,
        ]
    );
}

#[test]
fn builder_applies_no_video_to_builtin_mirrors() {
    let downloader = osu_downloader::Downloader::builder()
        .mirror(Mirror::nerinyan())
        .mirror(Mirror::custom("https://example.com/d/{id}").unwrap())
        .no_video(true)
        .build()
        .unwrap();

    let mirrors = downloader.mirror_pool_mirrors();
    assert_eq!(
        mirrors[0].url_for_id(123),
        "https://api.nerinyan.moe/d/123?nv=1"
    );
    assert_eq!(mirrors[1].url_for_id(123), "https://example.com/d/123");
}
