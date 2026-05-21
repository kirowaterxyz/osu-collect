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

    /// Errors stick until the user dismisses them; loading toasts stick until
    /// the operation resolves. Info ages out after [`MESSAGE_TTL`].
    pub fn is_expired(&self) -> bool {
        matches!(self.kind, MessageKind::Info) && self.created_at.elapsed() >= MESSAGE_TTL
    }

    pub fn is_dismissible_error(&self) -> bool {
        self.kind == MessageKind::Error
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

pub(crate) fn clear_expired_message(slot: &mut Option<AppMessage>) {
    if slot.as_ref().is_some_and(AppMessage::is_expired) {
        *slot = None;
    }
}

/// Clears the slot if it holds a sticky error toast. Returns `true` when an
/// error was actually dismissed, so callers can short-circuit other handlers
/// for the same keypress.
pub(crate) fn dismiss_error_message(slot: &mut Option<AppMessage>) -> bool {
    if slot.as_ref().is_some_and(AppMessage::is_dismissible_error) {
        *slot = None;
        return true;
    }
    false
}

#[cfg(test)]
#[path = "../../tests/unit/app_messages.rs"]
mod tests;
