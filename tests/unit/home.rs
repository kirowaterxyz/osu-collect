use osu_collect::{app::home::HomeTab, config::Config, mirrors::MirrorKind};

fn home_all_off(config: &Config) -> HomeTab {
    let mut home = HomeTab::new(config);
    home.nerinyan = false;
    home.osu_direct = false;
    home.sayobot = false;
    home.nekoha = false;
    home.catboy_central = false;
    home.catboy_us = false;
    home.catboy_asia = false;
    home.custom_mirror.value = String::new();
    home
}

#[test]
fn build_mirror_list_returns_selected_mirrors() {
    let config = Config::default();
    let mut home = home_all_off(&config);
    home.nerinyan = true;

    let mirrors = home.build_mirror_list();
    assert_eq!(mirrors.len(), 1);
    assert_eq!(mirrors[0].kind, MirrorKind::Nerinyan);
}

#[test]
fn build_mirror_list_empty_when_none_selected() {
    let config = Config::default();
    let home = home_all_off(&config);

    let mirrors = home.build_mirror_list();
    assert!(mirrors.is_empty());
}

#[test]
fn build_mirror_list_includes_custom_mirror() {
    let config = Config::default();
    let mut home = home_all_off(&config);
    home.custom_mirror.value = "https://example.com/d/{id}".to_string();

    let mirrors = home.build_mirror_list();
    assert_eq!(mirrors.len(), 1);
    assert_eq!(mirrors[0].kind, MirrorKind::Custom);
}

#[test]
fn build_request_uses_same_mirrors_as_build_mirrors() {
    let config = Config::default();
    let mut home = home_all_off(&config);
    home.nerinyan = true;
    home.catboy_central = true;
    home.collection.value = "12345".to_string();

    let standalone = home.build_mirrors();
    let request = home.build_request().unwrap();
    let request_kinds: Vec<_> = request.config.mirrors.iter().map(|m| m.kind).collect();
    let standalone_kinds: Vec<_> = standalone.iter().map(|m| m.kind).collect();
    assert_eq!(request_kinds, standalone_kinds);
}

#[test]
fn build_mirror_list_includes_official_with_config_credentials() {
    let mut config = Config::default();
    config.mirror.official = true;
    config.official.client_id = Some("42".to_string());
    config.official.client_secret = Some("secret".to_string());
    let mut home = home_all_off(&config);
    home.official = true;

    let mirrors = home.build_mirror_list();

    assert_eq!(mirrors.len(), 1);
    assert_eq!(mirrors[0].kind, MirrorKind::Official);
    assert!(mirrors[0].headers.is_none());
    assert!(mirrors[0].official.is_some());
}
