use osu_collect::{app::home::HomeTab, config::Config, mirrors::MirrorKind};

fn home_all_off(config: &Config) -> HomeTab {
    let mut home = HomeTab::new(config);
    home.nerinyan = false;
    home.osu_direct = false;
    home.sayobot = false;
    home.nekoha = false;
    home.custom_mirror.value = String::new();
    home
}
#[test]
fn home_defaults_to_every_builtin_mirror() {
    let config = Config::default();
    let home = HomeTab::new(&config);

    let mirror_kinds: Vec<_> = home
        .build_mirror_list()
        .iter()
        .map(|mirror| mirror.kind())
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
fn build_mirror_list_returns_selected_mirrors() {
    let config = Config::default();
    let mut home = home_all_off(&config);
    home.nerinyan = true;

    let mirrors = home.build_mirror_list();
    assert_eq!(mirrors.len(), 1);
    assert_eq!(mirrors[0].kind(), MirrorKind::Nerinyan);
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
    assert_eq!(mirrors[0].kind(), MirrorKind::Custom);
}

#[test]
fn build_request_uses_same_mirrors_as_build_mirrors() {
    let config = Config::default();
    let mut home = home_all_off(&config);
    home.nerinyan = true;
    home.osu_direct = true;
    home.collection.value = "12345".to_string();

    let standalone = home.build_mirrors();
    let request = home.build_request(false).unwrap();
    let request_kinds: Vec<_> = request.config.mirrors.iter().map(|m| m.kind()).collect();
    let standalone_kinds: Vec<_> = standalone.iter().map(|m| m.kind()).collect();
    assert_eq!(request_kinds, standalone_kinds);
}

#[test]
fn build_request_passes_verify_zip_eocd_argument() {
    let config = Config::default();
    let mut home = HomeTab::new(&config);
    home.collection.value = "12345".to_string();

    assert!(!home.build_request(false).unwrap().config.verify_zip_eocd);
    assert!(home.build_request(true).unwrap().config.verify_zip_eocd);
}
