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
