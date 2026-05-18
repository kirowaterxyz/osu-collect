use super::{AppMessage, MESSAGE_TTL};
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
