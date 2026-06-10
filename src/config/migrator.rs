use std::{fs, path::Path};
use tracing::{info, warn};

const OBSOLETE_DOWNLOAD_FIELDS: &[&str] = &["verify_zip_eocd"];

/// Old theme values → new values.
/// `default` → `full`, `sixteen` → `compatible`, `colorblind-safe` → `auto`.
const THEME_RENAMES: &[(&str, &str)] = &[
    ("default", "full"),
    ("sixteen", "compatible"),
    ("colorblind-safe", "auto"),
];

pub fn migrate_in_place(path: &Path) {
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };
    let Ok(mut table) = contents.parse::<toml::Table>() else {
        return;
    };

    let mut dirty = false;
    dirty |= strip_obsolete_fields(&mut table);
    dirty |= migrate_theme_mode(&mut table);

    if !dirty {
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

/// Remap old `display.theme` string values to the new palette names.
fn migrate_theme_mode(table: &mut toml::Table) -> bool {
    let Some(toml::Value::Table(display)) = table.get_mut("display") else {
        return false;
    };
    let Some(toml::Value::String(current)) = display.get_mut("theme") else {
        return false;
    };
    for (old, new) in THEME_RENAMES {
        if current.as_str() == *old {
            info!(old = old, new = new, "migrated display.theme value");
            *current = (*new).to_owned();
            return true;
        }
    }
    false
}

#[cfg(test)]
#[path = "../../tests/unit/config_migrator.rs"]
mod tests;
