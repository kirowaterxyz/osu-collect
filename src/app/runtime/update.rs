//! Self-update lifecycle surfaced to the UI as toasts.
//!
//! The check runs once at startup as a detached task. When a newer release is
//! found it raises a sticky "downloading update" toast; on a successful install
//! that toast is swapped for a "restart to finish" notice that stays until the
//! user dismisses it. Failures are best-effort — they replace the in-flight
//! toast (or push a fresh one) and need no action.

use super::super::{App, Toast, ToastTag};
use crate::auto_update::check_and_apply;
use tokio::sync::mpsc;
use tracing::warn;

#[derive(Debug)]
pub(super) enum UpdateEvent {
    /// A newer release was found; the download has started.
    Downloading,
    /// The new binary was installed; the app must be restarted to apply it.
    Installed,
    /// The check or download failed. Best-effort; carries the reason.
    Failed(String),
}

/// Spawn the one-shot background update check. Detached: it outlives nothing the
/// runtime awaits, so a mid-run quit simply drops the receiver.
pub(super) fn spawn_update_check(tx: mpsc::UnboundedSender<UpdateEvent>) {
    tokio::spawn(async move {
        let found_tx = tx.clone();
        let result = check_and_apply(move || {
            let _ = found_tx.send(UpdateEvent::Downloading);
        })
        .await;
        let outcome = match result {
            Ok(Some(_message)) => UpdateEvent::Installed,
            Ok(None) => return, // up to date — stay silent
            Err(err) => UpdateEvent::Failed(err.to_string()),
        };
        let _ = tx.send(outcome);
    });
}

pub(super) fn handle_update_event(event: UpdateEvent, app: &mut App) {
    match event {
        UpdateEvent::Downloading => {
            app.push_toast(
                Toast::info("downloading update")
                    .until_resolved()
                    .tagged(ToastTag::Update),
            );
        }
        UpdateEvent::Installed => {
            app.toasts.replace_tagged(
                ToastTag::Update,
                Toast::success("update installed")
                    .with_detail("restart osu!collect to finish update")
                    .until_dismissed(),
            );
        }
        UpdateEvent::Failed(err) => {
            warn!(error = %err, "Auto-update failed; a new version may be available");
            app.toasts.replace_tagged(
                ToastTag::Update,
                Toast::danger("update failed")
                    .with_detail(err)
                    .tagged(ToastTag::Update),
            );
        }
    }
}
