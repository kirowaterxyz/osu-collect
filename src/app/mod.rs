pub mod collection;
pub mod collection_state;
pub mod config;
pub mod failed_maps;
pub mod home;
pub mod messages;
pub mod runtime;
pub mod snapshots;
pub mod state;
pub mod updates;

pub use collection::{CollectionPage, ThreadStatusLine};
pub use config::{AuthLoginState, ConfigField, ConfigTab};
pub use home::{HomeField, HomeTab, InputField};
pub use messages::MessageKind;
pub use runtime::run as run_app;
pub use state::{App, AppCommand};
pub use updates::{UpdatesField, UpdatesTab};

pub(crate) fn next_field<T: Copy + PartialEq>(fields: &[T], current: T) -> T {
    adjacent_field(fields, current, 1)
}

pub(crate) fn prev_field<T: Copy + PartialEq>(fields: &[T], current: T) -> T {
    adjacent_field(fields, current, fields.len().saturating_sub(1))
}

fn adjacent_field<T: Copy + PartialEq>(fields: &[T], current: T, offset: usize) -> T {
    let idx = fields
        .iter()
        .position(|&field| field == current)
        .unwrap_or_default();
    fields[(idx + offset) % fields.len()]
}
