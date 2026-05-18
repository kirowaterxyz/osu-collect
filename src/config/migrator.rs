use std::{fs, path::Path};
use tracing::{info, warn};

const OBSOLETE_DOWNLOAD_FIELDS: &[&str] = &["verify_zip_eocd"];

pub fn migrate_in_place(path: &Path) {
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };
    let Ok(mut table) = contents.parse::<toml::Table>() else {
        return;
    };

    if !strip_obsolete_fields(&mut table) {
        return;
    }

    match toml::to_string_pretty(&table) {
        Ok(rewritten) => {
            if let Err(err) = fs::write(path, rewritten) {
                warn!(path = %path.display(), error = %err, "failed to write migrated config");
            }
        }
        Err(err) => warn!(error = %err, "failed to serialize migrated config"),
    }
}

fn strip_obsolete_fields(table: &mut toml::Table) -> bool {
    let Some(toml::Value::Table(download)) = table.get_mut("download") else {
        return false;
    };

    let mut dirty = false;
    for field in OBSOLETE_DOWNLOAD_FIELDS {
        if download.remove(*field).is_some() {
            info!(field = field, "removed obsolete config field");
            dirty = true;
        }
    }
    dirty
}

#[cfg(test)]
#[path = "../../tests/unit/config_migrator.rs"]
mod tests;
