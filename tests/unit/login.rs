use super::{LoginField, LoginPhase, LoginTab};

#[test]
fn new_logged_out_starts_on_credentials() {
    let tab = LoginTab::new(false);
    assert_eq!(tab.phase, LoginPhase::Credentials);
    assert_eq!(tab.focus, LoginField::Username);
    assert_eq!(
        tab.fields(),
        [
            LoginField::Username,
            LoginField::Password,
            LoginField::Submit
        ]
        .as_slice()
    );
}

#[test]
fn new_logged_in_starts_on_account_view() {
    let tab = LoginTab::new(true);
    assert_eq!(tab.phase, LoginPhase::LoggedIn);
    assert_eq!(tab.focus, LoginField::Submit);
    assert_eq!(tab.fields(), [LoginField::Submit].as_slice());
}

#[test]
fn next_field_cycles_credentials() {
    let mut tab = LoginTab::new(false);
    assert_eq!(tab.focus, LoginField::Username);
    tab.next_field();
    assert_eq!(tab.focus, LoginField::Password);
    tab.next_field();
    assert_eq!(tab.focus, LoginField::Submit);
    tab.next_field();
    assert_eq!(tab.focus, LoginField::Username, "field nav must wrap");
}

#[test]
fn enter_verification_reveals_code_field() {
    let mut tab = LoginTab::new(false);
    tab.username.set_value("user");
    tab.password.set_value("pass");
    tab.enter_verification();
    assert_eq!(tab.phase, LoginPhase::NeedsVerification);
    assert_eq!(tab.focus, LoginField::Code);
    assert_eq!(
        tab.fields(),
        [LoginField::Code, LoginField::Submit, LoginField::Resend].as_slice()
    );
}

#[test]
fn enter_logged_in_clears_every_secret() {
    let mut tab = LoginTab::new(false);
    tab.username.set_value("user");
    tab.password.set_value("secret");
    tab.code.set_value("1234");
    tab.enter_logged_in();
    assert_eq!(tab.phase, LoginPhase::LoggedIn);
    assert!(tab.username.value.is_empty());
    assert!(tab.password.value.is_empty());
    assert!(tab.code.value.is_empty());
}

#[test]
fn reset_credentials_keeps_username_clears_password() {
    let mut tab = LoginTab::new(false);
    tab.username.set_value("keepme");
    tab.password.set_value("secret");
    tab.phase = LoginPhase::NeedsVerification;
    tab.reset_credentials();
    assert_eq!(tab.phase, LoginPhase::Credentials);
    assert_eq!(tab.username.value, "keepme");
    assert!(tab.password.value.is_empty());
    assert_eq!(tab.focus, LoginField::Username);
}

#[test]
fn clear_password_wipes_only_password() {
    let mut tab = LoginTab::new(false);
    tab.username.set_value("user");
    tab.password.set_value("secret");
    tab.clear_password();
    assert_eq!(tab.username.value, "user");
    assert!(tab.password.value.is_empty());
}

#[test]
fn field_classification() {
    assert!(LoginField::Password.is_secret());
    assert!(!LoginField::Username.is_secret());
    assert!(LoginField::Username.is_text_input());
    assert!(LoginField::Code.is_text_input());
    assert!(!LoginField::Submit.is_text_input());
    assert!(!LoginField::Resend.is_text_input());
}

#[test]
fn handle_char_types_into_focused_input_only() {
    let mut tab = LoginTab::new(false);
    tab.focus = LoginField::Username;
    tab.handle_char('a');
    tab.handle_char('b');
    assert_eq!(tab.username.value, "ab");

    // A chip row has no input — typing is a no-op.
    tab.focus = LoginField::Submit;
    tab.handle_char('x');
    assert_eq!(tab.username.value, "ab");
}
