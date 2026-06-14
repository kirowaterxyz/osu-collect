use std::{fs, path::Path};
use tracing::{info, warn};

const OBSOLETE_DOWNLOAD_FIELDS: &[&str] = &["verify_zip_eocd"];

/// Old theme values → new values. `default` → `full`, `sixteen` → `compatible`.
const THEME_RENAMES: &[(&str, &str)] = &[("default", "full"), ("sixteen", "compatible")];

/// Obsolete theme values. These are removed so the key becomes absent → the
/// full palette is used (rather than pinning them to a stale palette).
const THEME_REMOVALS: &[&str] = &["auto", "colorblind-safe"];

pub fn migrate_in_place(path: &Path) {
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };
    let Ok(mut table) = contents.parse::<toml::Table>() else {
        return;
    };

    let mut dirty = false;
    dirty |= strip_obsolete_fields(&mut table);
    dirty |= migrate_no_video(&mut table);
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

/// Migrate the inverted `download.no_video` key to `download.video`.
///
/// `video = !no_video` (true = videos included), then the old key is removed.
/// A pre-existing `video` key is left untouched and `no_video` is still
/// dropped, so the explicit value wins over the legacy one.
fn migrate_no_video(table: &mut toml::Table) -> bool {
    let Some(toml::Value::Table(download)) = table.get_mut("download") else {
        return false;
    };
    let Some(toml::Value::Boolean(no_video)) = download.remove("no_video") else {
        return false;
    };
    if !download.contains_key("video") {
        download.insert("video".to_owned(), toml::Value::Boolean(!no_video));
    }
    info!(no_video, "migrated download.no_video to download.video");
    true
}

/// Remap or strip old `display.theme` string values.
///
/// Renamed values (`default`/`sixteen`) are rewritten in place; the obsolete
/// values (`auto`/`colorblind-safe`) have the key removed so the default full
/// palette takes over.
fn migrate_theme_mode(table: &mut toml::Table) -> bool {
    let Some(toml::Value::Table(display)) = table.get_mut("display") else {
        return false;
    };
    let Some(toml::Value::String(current)) = display.get("theme") else {
        return false;
    };
    let current = current.clone();

    if THEME_REMOVALS.contains(&current.as_str()) {
        display.remove("theme");
        info!(old = %current, "removed obsolete display.theme value");
        return true;
    }
    for (old, new) in THEME_RENAMES {
        if current == *old {
            info!(old = old, new = new, "migrated display.theme value");
            display.insert("theme".to_owned(), toml::Value::String((*new).to_owned()));
            return true;
        }
    }
    false
}

#[cfg(test)]
#[path = "../../tests/unit/config_migrator.rs"]
mod tests;
