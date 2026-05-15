use osu_collect::mirrors::{CatboyRegion, Mirror, MirrorKind};

#[test]
fn builtin_nerinyan_generates_valid_url() {
    let mirror = Mirror::builtin(MirrorKind::Nerinyan, false).unwrap();
    let url = mirror.url_for(12345);
    assert!(url.contains("12345"));
    assert!(url.starts_with("https://"));
}

#[test]
fn builtin_nerinyan_no_video() {
    let with_video = Mirror::builtin(MirrorKind::Nerinyan, false).unwrap();
    let no_video = Mirror::builtin(MirrorKind::Nerinyan, true).unwrap();
    assert_ne!(with_video.url_for(100), no_video.url_for(100));
}

#[test]
fn custom_mirror_requires_id_placeholder() {
    assert!(Mirror::custom("https://example.com/download").is_err());
}

#[test]
fn custom_mirror_requires_http() {
    assert!(Mirror::custom("ftp://example.com/{id}").is_err());
}

#[test]
fn custom_mirror_valid() {
    let mirror = Mirror::custom("https://example.com/d/{id}").unwrap();
    assert_eq!(mirror.kind(), MirrorKind::Custom);
    assert_eq!(mirror.url_for(99999), "https://example.com/d/99999");
}

#[test]
fn catboy_regions_produce_different_urls() {
    let central = Mirror::builtin(MirrorKind::Catboy(CatboyRegion::Central), false).unwrap();
    let us = Mirror::builtin(MirrorKind::Catboy(CatboyRegion::Us), false).unwrap();
    let asia = Mirror::builtin(MirrorKind::Catboy(CatboyRegion::Asia), false).unwrap();
    assert!(central.url_for(1) != us.url_for(1) || us.url_for(1) != asia.url_for(1));
}

#[test]
fn display_name_returns_static_str() {
    let mirror = Mirror::builtin(MirrorKind::Nerinyan, false).unwrap();
    assert!(!mirror.display_name().is_empty());
}
