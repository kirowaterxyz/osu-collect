use crate::config::{Config, ThemeMode, load_config_from, save_config};
use std::fs;

fn write_toml(dir: &std::path::Path, contents: &str) -> std::path::PathBuf {
    let path = dir.join("config.toml");
    fs::write(&path, contents).unwrap();
    path
}

#[test]
fn theme_field_roundtrips_through_save_and_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");

    let mut config = Config::default();
    config.display.theme = ThemeMode::ColorblindSafe;

    // Use the env-var override to point save_config at our temp dir
    unsafe { std::env::set_var("OSU_COLLECT_CONFIG", path.to_str().unwrap()) };
    save_config(&config).expect("save must succeed");
    let loaded = load_config_from(&path).expect("load must succeed");
    unsafe { std::env::remove_var("OSU_COLLECT_CONFIG") };

    assert_eq!(
        loaded.display.theme,
        ThemeMode::ColorblindSafe,
        "theme variant must survive save+load"
    );
}

#[test]
fn theme_defaults_to_auto_when_absent_from_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_toml(dir.path(), "[mirror]\nnerinyan = true\n");
    let loaded = load_config_from(&path).expect("load must succeed");
    assert_eq!(
        loaded.display.theme,
        ThemeMode::Auto,
        "missing display.theme must default to Auto"
    );
}

#[test]
fn all_theme_variants_serialize_and_deserialize() {
    let cases = [
        (ThemeMode::Auto, "auto"),
        (ThemeMode::Default, "default"),
        (ThemeMode::Sixteen, "sixteen"),
        (ThemeMode::ColorblindSafe, "colorblind-safe"),
    ];
    for (variant, toml_str) in cases {
        let toml = format!("[display]\ntheme = \"{toml_str}\"\n");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, &toml).unwrap();
        let loaded = load_config_from(&path).expect("load must succeed");
        assert_eq!(
            loaded.display.theme, variant,
            "{toml_str} must deserialize to correct variant"
        );
    }
}
