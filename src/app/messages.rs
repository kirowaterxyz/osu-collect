use std::time::{Duration, Instant};

const MESSAGE_TTL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Error,
    Loading,
}

pub struct AppMessage {
    pub kind: MessageKind,
    pub text: String,
    created_at: Instant,
}

impl AppMessage {
    pub fn info(text: impl Into<String>) -> Self {
        Self::new(MessageKind::Info, text)
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self::new(MessageKind::Error, text)
    }

    pub fn loading(text: impl Into<String>) -> Self {
        Self::new(MessageKind::Loading, text)
    }

    pub fn is_expired(&self) -> bool {
        self.kind != MessageKind::Loading && self.created_at.elapsed() >= MESSAGE_TTL
    }

    fn new(kind: MessageKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            created_at: Instant::now(),
        }
    }
}

pub(crate) fn set_error_message(slot: &mut Option<AppMessage>, message: impl Into<String>) {
    *slot = Some(AppMessage::error(message));
}

pub(crate) fn set_info_message(slot: &mut Option<AppMessage>, message: impl Into<String>) {
    *slot = Some(AppMessage::info(message));
}

pub(crate) fn set_loading_message(slot: &mut Option<AppMessage>, message: impl Into<String>) {
    *slot = Some(AppMessage::loading(message));
}

pub(crate) fn clear_app_message(slot: &mut Option<AppMessage>) {
    *slot = None;
}

pub(crate) fn clear_expired_app_message(slot: &mut Option<AppMessage>) {
    if slot.as_ref().is_some_and(AppMessage::is_expired) {
        *slot = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
