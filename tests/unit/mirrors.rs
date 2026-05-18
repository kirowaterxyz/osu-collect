use super::{Mirror, MirrorKind, from_kind};
use crate::config::Config;

#[test]
fn config_defaults_to_every_builtin_mirror() {
    let config: Config = toml::from_str("[mirror]\n[download]\n").unwrap();

    assert!(config.mirror.nerinyan);
    assert!(config.mirror.osu_direct);
    assert!(config.mirror.sayobot);
    assert!(config.mirror.nekoha);
}

#[test]
fn builtin_nerinyan_is_constructible() {
    let mirror = from_kind(MirrorKind::Nerinyan).unwrap();
    assert_eq!(mirror.kind(), MirrorKind::Nerinyan);
}

#[test]
fn nerinyan_no_video_switches_template() {
    let mirror = from_kind(MirrorKind::Nerinyan).unwrap().no_video();
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
fn kind_label_returns_static_str() {
    let mirror = from_kind(MirrorKind::Nerinyan).unwrap();
    assert!(!mirror.kind().label().is_empty());
}
