use osu_collect::{
    app::{
        config::{AuthLoginState, ConfigField, ConfigTab},
        messages::{MessageKind, set_info_message},
    },
    config::Config,
    download::ArchiveValidation,
};

fn tab_logged_out() -> ConfigTab {
    let mut tab = ConfigTab::new(&Config::default());
    tab.auth_loaded = false;
    tab.login_state = AuthLoginState::LoggedOut;
    tab
}

fn tab_logged_in() -> ConfigTab {
    let mut tab = ConfigTab::new(&Config::default());
    tab.auth_loaded = true;
    tab.login_state = AuthLoginState::LoggedIn;
    tab
}

#[test]
fn login_state_initial_logged_out() {
    let tab = tab_logged_out();
    assert_eq!(tab.login_state, AuthLoginState::LoggedOut);
    assert!(!tab.auth_loaded);
}

#[test]
fn login_state_initial_logged_in() {
    let tab = tab_logged_in();
    assert_eq!(tab.login_state, AuthLoginState::LoggedIn);
    assert!(tab.auth_loaded);
}

#[test]
fn login_flow_marks_in_progress_without_message() {
    let mut tab = tab_logged_out();
    tab.set_login_in_progress();
    assert_eq!(tab.login_state, AuthLoginState::InProgress(String::new()));
    assert!(tab.message.is_none());
    assert!(!tab.auth_loaded);
}

#[test]
fn login_flow_success() {
    let mut tab = tab_logged_out();
    tab.set_login_in_progress();
    tab.set_login_complete();
    assert_eq!(tab.login_state, AuthLoginState::LoggedIn);
    assert!(tab.auth_loaded);
}

#[test]
fn login_flow_error_returns_to_logged_out() {
    let mut tab = tab_logged_out();
    tab.set_login_in_progress();
    tab.set_login_failed();
    assert_eq!(tab.login_state, AuthLoginState::LoggedOut);
    assert!(!tab.auth_loaded);
}

#[test]
fn cancel_login_returns_to_logged_out_with_info_message() {
    let mut tab = tab_logged_out();
    tab.set_login_in_progress();
    assert!(matches!(tab.login_state, AuthLoginState::InProgress(_)));

    tab.set_login_failed();
    set_info_message(&mut tab.message, "login cancelled");

    assert_eq!(tab.login_state, AuthLoginState::LoggedOut);
    let msg = tab.message.as_ref().expect("info message preserved");
    assert_eq!(msg.kind, MessageKind::Info);
    assert_eq!(msg.text, "login cancelled");
}

#[test]
fn logout_clears_state() {
    let mut tab = tab_logged_in();
    tab.set_logged_out();
    assert_eq!(tab.login_state, AuthLoginState::LoggedOut);
    assert!(!tab.auth_loaded);
}

#[test]
fn logout_loading_message_does_not_expire() {
    let mut tab = tab_logged_in();
    tab.set_loading("logging out...");
    let msg = tab.message.as_ref().unwrap();
    assert!(!msg.is_expired());
}

#[test]
fn next_field_cycles_through_login_entries() {
    let mut tab = tab_logged_in();
    tab.focus = ConfigField::LoggingDirectory;
    tab.next_field();
    assert_eq!(tab.focus, ConfigField::LoginEntry);
    tab.next_field();
    assert_eq!(tab.focus, ConfigField::LogoutEntry);
    tab.next_field();
    assert_eq!(tab.focus, ConfigField::DownloadThreads);
}

#[test]
fn prev_field_cycles_through_login_entries() {
    let mut tab = tab_logged_in();
    tab.focus = ConfigField::DownloadThreads;
    tab.prev_field();
    assert_eq!(tab.focus, ConfigField::LogoutEntry);
    tab.prev_field();
    assert_eq!(tab.focus, ConfigField::LoginEntry);
    tab.prev_field();
    assert_eq!(tab.focus, ConfigField::LoggingDirectory);
}

#[test]
fn next_field_skips_logout_when_logged_out() {
    let mut tab = tab_logged_out();
    tab.focus = ConfigField::LoginEntry;
    tab.next_field();
    assert_eq!(tab.focus, ConfigField::DownloadThreads);
}

#[test]
fn prev_field_skips_logout_when_logged_out() {
    let mut tab = tab_logged_out();
    tab.focus = ConfigField::DownloadThreads;
    tab.prev_field();
    assert_eq!(tab.focus, ConfigField::LoginEntry);
}

#[test]
fn logout_evacuates_focus_when_logging_out() {
    let mut tab = tab_logged_in();
    tab.focus = ConfigField::LogoutEntry;
    tab.set_logged_out();
    assert_eq!(tab.focus, ConfigField::LoginEntry);
}

#[test]
fn all_fields_form_complete_cycle() {
    let mut tab = tab_logged_in();
    let start = tab.focus;
    let total = 14;
    for _ in 0..total {
        tab.next_field();
    }
    assert_eq!(tab.focus, start, "next_field must complete a full cycle");
}

#[test]
fn cycle_archive_validation_wraps_through_all_variants() {
    let mut tab = tab_logged_in();
    tab.archive_validation = ArchiveValidation::Off;
    tab.cycle_archive_validation();
    assert_eq!(tab.archive_validation, ArchiveValidation::Magic);
    tab.cycle_archive_validation();
    assert_eq!(tab.archive_validation, ArchiveValidation::Eocd);
    tab.cycle_archive_validation();
    assert_eq!(tab.archive_validation, ArchiveValidation::Off);
}
