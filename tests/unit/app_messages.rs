use super::{AppMessage, MESSAGE_TTL, clear_expired_message, dismiss_error_message};
use std::time::{Duration, Instant};

#[test]
fn info_message_expires_after_ttl() {
    let mut message = AppMessage::info("saved");
    message.created_at = Instant::now() - MESSAGE_TTL - Duration::from_millis(1);

    assert!(message.is_expired());
}

#[test]
fn loading_message_does_not_expire() {
    let mut message = AppMessage::loading("loading");
    message.created_at = Instant::now() - MESSAGE_TTL - Duration::from_millis(1);

    assert!(!message.is_expired());
}

#[test]
fn error_message_does_not_expire_even_after_ttl() {
    let mut message = AppMessage::error("network unreachable");
    message.created_at = Instant::now() - MESSAGE_TTL - Duration::from_secs(60);

    assert!(!message.is_expired(), "errors must persist until dismissed");
}

#[test]
fn clear_expired_keeps_aged_error_but_drops_aged_info() {
    let mut info_slot = Some(AppMessage::info("saved"));
    if let Some(msg) = info_slot.as_mut() {
        msg.created_at = Instant::now() - MESSAGE_TTL - Duration::from_millis(1);
    }
    let mut error_slot = Some(AppMessage::error("boom"));
    if let Some(msg) = error_slot.as_mut() {
        msg.created_at = Instant::now() - MESSAGE_TTL - Duration::from_secs(60);
    }

    clear_expired_message(&mut info_slot);
    clear_expired_message(&mut error_slot);

    assert!(info_slot.is_none(), "info must be cleared after ttl");
    assert!(
        error_slot.is_some(),
        "error must survive the same expiry sweep"
    );
}

#[test]
fn dismiss_error_clears_only_error_slots() {
    let mut error_slot = Some(AppMessage::error("boom"));
    assert!(dismiss_error_message(&mut error_slot));
    assert!(error_slot.is_none());

    let mut info_slot = Some(AppMessage::info("saved"));
    assert!(!dismiss_error_message(&mut info_slot));
    assert!(info_slot.is_some(), "info must not be dismissed by x");

    let mut empty: Option<AppMessage> = None;
    assert!(!dismiss_error_message(&mut empty));
}
