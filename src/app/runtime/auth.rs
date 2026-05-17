use super::super::{
    App, AuthLoginState,
    messages::{set_error_message, set_info_message},
};
use crate::auth;
use tokio::sync::mpsc;
use tracing::debug;

#[derive(Debug)]
pub(super) enum AuthEvent {
    LoginComplete(Result<(), String>),
    LogoutComplete(Result<(), String>),
}

pub(super) fn handle_auth_event(event: AuthEvent, app: &mut App) {
    match event {
        AuthEvent::LoginComplete(_)
            if !matches!(app.config.login_state, AuthLoginState::InProgress(_)) =>
        {
            debug!("Discarding stale login event (user cancelled or already settled)");
        }
        AuthEvent::LoginComplete(Ok(())) => {
            app.config.set_login_complete();
            set_info_message(&mut app.config.message, "login successful");
        }
        AuthEvent::LoginComplete(Err(err)) => {
            app.config.set_login_failed();
            set_error_message(&mut app.config.message, format!("login failed: {err}"));
        }
        AuthEvent::LogoutComplete(Ok(())) => {
            app.config.set_logged_out();
            set_info_message(&mut app.config.message, "logged out");
        }
        AuthEvent::LogoutComplete(Err(err)) => {
            app.config.set_login_failed();
            set_error_message(&mut app.config.message, format!("logout failed: {err}"));
        }
    }
}

pub(super) fn spawn_login_task(
    client_id: String,
    client_secret: String,
    tx: mpsc::UnboundedSender<AuthEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let result = auth::run_login_flow(&client, &client_id, &client_secret)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string());
        let _ = tx.send(AuthEvent::LoginComplete(result));
    })
}

pub(super) fn spawn_logout_task(tx: mpsc::UnboundedSender<AuthEvent>) {
    tokio::task::spawn_blocking(move || {
        let result = auth::delete().map_err(|err| err.to_string());
        let _ = tx.send(AuthEvent::LogoutComplete(result));
    });
}
