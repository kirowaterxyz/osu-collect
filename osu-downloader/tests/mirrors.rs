use crate::{Mirror, MirrorKind};

#[test]
fn mirror_templates() {
    assert_eq!(
        Mirror::nerinyan().url_for_id(123),
        "https://api.nerinyan.moe/d/123"
    );
    assert_eq!(
        Mirror::osu_direct().url_for_id(789),
        "https://osu.direct/d/789"
    );
    assert_eq!(
        Mirror::sayobot().url_for_id(42),
        "https://dl.sayobot.cn/beatmaps/download/full/42"
    );
    assert_eq!(
        Mirror::nekoha().url_for_id(1),
        "https://mirror.nekoha.moe/api4/download/1"
    );
}

#[test]
fn url_for_edge_ids() {
    // id=0
    assert_eq!(
        Mirror::nerinyan().url_for_id(0),
        "https://api.nerinyan.moe/d/0"
    );
    assert_eq!(Mirror::osu_direct().url_for_id(0), "https://osu.direct/d/0");
    // id=u32::MAX
    assert_eq!(
        Mirror::nerinyan().url_for_id(u32::MAX),
        format!("https://api.nerinyan.moe/d/{}", u32::MAX)
    );
    assert_eq!(
        Mirror::osu_direct().url_for_id(u32::MAX),
        format!("https://osu.direct/d/{}", u32::MAX)
    );
    // all builtin kinds via Mirror::builtin
    for kind in [
        MirrorKind::Nerinyan,
        MirrorKind::OsuDirect,
        MirrorKind::Sayobot,
        MirrorKind::Nekoha,
    ] {
        let mirror = Mirror::builtin(kind).expect("builtin mirror exists");
        let url = mirror.url_for_id(1);
        assert!(
            url.contains('1'),
            "url_for_id(1) must contain '1' for {kind:?}: {url}"
        );
        assert!(
            !url.contains("{id}"),
            "url must not contain literal '{{id}}' for {kind:?}"
        );
    }
}

#[test]
fn no_video_switches_template_when_supported() {
    assert_eq!(
        Mirror::nerinyan().no_video().url_for_id(42),
        "https://api.nerinyan.moe/d/42?nv=1"
    );
}

#[test]
fn no_video_is_noop_for_custom_mirrors() {
    let mirror = Mirror::custom("https://example.com/dl/{id}")
        .unwrap()
        .no_video();
    assert_eq!(mirror.url_for_id(123), "https://example.com/dl/123");
}

#[test]
fn custom_mirror() {
    let mirror = Mirror::custom("https://example.com/dl/{id}").unwrap();
    assert_eq!(mirror.url_for_id(123), "https://example.com/dl/123");
}

#[test]
fn invalid_custom_mirror() {
    assert!(Mirror::custom("https://example.com/dl/").is_err());
    assert!(Mirror::custom("ftp://example.com/{id}").is_err());
}
