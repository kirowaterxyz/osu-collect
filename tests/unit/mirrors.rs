use super::{Mirror, MirrorKind};
use crate::config::Config;

#[test]
fn config_defaults_to_every_builtin_mirror() {
    let config: Config = toml::from_str("[mirror]\n[download]\n").unwrap();

    assert!(config.mirror.nerinyan);
    assert!(config.mirror.osu_direct);
    assert!(config.mirror.sayobot);
    assert!(config.mirror.nekoha);
    assert!(config.mirror.beatconnect);
    assert!(config.mirror.osudl);
    // hinamizawa (redundant cascade) and osu! official (needs login) are off by default.
    assert!(!config.mirror.hinamizawa);
    assert!(!config.mirror.osu_official);
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
