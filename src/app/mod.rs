use std::{fs, io::Write, path::Path};

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

pub use collection::{ActiveDownloadLine, CollectionPage};
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

fn write_atomic(path: &Path, tmp_extension: &str, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp = path.with_extension(tmp_extension);
    let write_result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.flush()?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&tmp);
    }

    write_result
}
