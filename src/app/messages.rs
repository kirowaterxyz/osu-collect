//! Footer status line for in-progress ("loading") work.
//!
//! Results and errors no longer live here — they surface as toasts
//! ([`crate::app::Toast`]). A tab holds at most one loading message at a time;
//! it is replaced or cleared by [`clear_app_message`] when the operation
//! resolves. The footer renders it with a spinner.

/// An in-progress status shown in the footer with a spinner.
pub struct AppMessage {
    pub text: String,
}

impl AppMessage {
    fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

pub(crate) fn set_loading_message(slot: &mut Option<AppMessage>, message: impl Into<String>) {
    *slot = Some(AppMessage::new(message));
}

pub(crate) fn clear_app_message(slot: &mut Option<AppMessage>) {
    *slot = None;
}

#[cfg(test)]
#[path = "../../tests/unit/app_messages.rs"]
mod tests;
