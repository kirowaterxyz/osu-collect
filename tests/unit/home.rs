use crate::{
    app::home::{HomeField, HomeTab},
    config::Config,
    download::ArchiveValidation,
    mirrors::MirrorKind,
};

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
fn build_request_uses_same_mirrors_as_build_mirror_list() {
    let config = Config::default();
    let mut home = home_all_off(&config);
    home.nerinyan = true;
    home.osu_direct = true;
    home.collection.value = "12345".to_string();

    let standalone = home.build_mirror_list();
    let request = home.build_request(ArchiveValidation::Magic).unwrap();
    let request_kinds: Vec<_> = request.config.mirrors.iter().map(|m| m.kind()).collect();
    let standalone_kinds: Vec<_> = standalone.iter().map(|m| m.kind()).collect();
    assert_eq!(request_kinds, standalone_kinds);
}

#[test]
fn build_request_passes_archive_validation_argument() {
    let config = Config::default();
    let mut home = HomeTab::new(&config);
    home.collection.value = "12345".to_string();

    let magic = home.build_request(ArchiveValidation::Magic).unwrap();
    assert_eq!(magic.config.archive_validation, ArchiveValidation::Magic);

    let eocd = home.build_request(ArchiveValidation::Eocd).unwrap();
    assert_eq!(eocd.config.archive_validation, ArchiveValidation::Eocd);
}

#[test]
fn threads_stepper_increments_by_one() {
    let config = Config::default();
    let mut home = HomeTab::new(&config);
    // Start from a known value below the max.
    home.threads.value = "2".to_string();
    home.focus = HomeField::Threads;

    home.step_up();

    assert_eq!(home.resolved_threads(), 3);
}

#[test]
fn threads_stepper_decrements_by_one() {
    let config = Config::default();
    let mut home = HomeTab::new(&config);
    home.threads.value = "4".to_string();
    home.focus = HomeField::Threads;

    home.step_down();

    assert_eq!(home.resolved_threads(), 3);
}

#[test]
fn threads_stepper_does_not_go_below_one() {
    let config = Config::default();
    let mut home = HomeTab::new(&config);
    home.threads.value = "1".to_string();
    home.focus = HomeField::Threads;

    home.step_down();

    assert_eq!(home.resolved_threads(), 1);
}

#[test]
fn threads_stepper_does_not_exceed_default_threads() {
    let config = Config::default();
    let mut home = HomeTab::new(&config);
    let max = home.default_threads;
    home.threads.value = max.to_string();

    home.step_up();

    assert_eq!(home.resolved_threads(), max);
}

#[test]
fn threads_digit_key_does_not_mutate_value() {
    let config = Config::default();
    let mut home = HomeTab::new(&config);
    home.focus = HomeField::Threads;
    home.threads.value = "3".to_string();

    home.handle_char('5');

    // Value must remain "3" — digit keys are ignored on the stepper.
    assert_eq!(home.threads.value, "3");
}

#[test]
fn threads_field_is_not_text_input() {
    assert!(!HomeField::Threads.is_text_input());
    assert!(HomeField::Threads.is_stepper());
}

#[test]
fn r_key_is_not_suppressed_when_threads_focused() {
    use crate::app::AppCommand;
    use crate::app::state::App;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let mut app = App::new(Config::default());
    app.home.focus = HomeField::Threads;

    let key = KeyEvent {
        code: KeyCode::Char('r'),
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    };
    let cmd = app.handle_key(key);
    assert!(
        matches!(cmd, Some(AppCommand::ProbeMirrors)),
        "'r' with threads focused must trigger mirror probe, got {cmd:?}"
    );
}
