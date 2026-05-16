use osu_collect::mirrors::{CatboyRegion, Mirror, MirrorKind};

#[test]
fn builtin_nerinyan_is_constructible() {
    let mirror = Mirror::builtin(MirrorKind::Nerinyan, false).unwrap();
    assert_eq!(mirror.kind(), MirrorKind::Nerinyan);
}

#[test]
fn builtin_nerinyan_no_video_is_constructible() {
    let mirror = Mirror::builtin(MirrorKind::Nerinyan, true).unwrap();
    assert_eq!(mirror.kind(), MirrorKind::Nerinyan);
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
}

#[test]
fn catboy_regions_are_constructible() {
    let central = Mirror::builtin(MirrorKind::Catboy(CatboyRegion::Central), false).unwrap();
    let us = Mirror::builtin(MirrorKind::Catboy(CatboyRegion::Us), false).unwrap();
    let asia = Mirror::builtin(MirrorKind::Catboy(CatboyRegion::Asia), false).unwrap();
    assert_eq!(central.kind(), MirrorKind::Catboy(CatboyRegion::Central));
    assert_eq!(us.kind(), MirrorKind::Catboy(CatboyRegion::Us));
    assert_eq!(asia.kind(), MirrorKind::Catboy(CatboyRegion::Asia));
}

#[test]
fn display_name_returns_static_str() {
    let mirror = Mirror::builtin(MirrorKind::Nerinyan, false).unwrap();
    assert!(!mirror.display_name().is_empty());
}
