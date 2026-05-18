use crate::Mirror;

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
