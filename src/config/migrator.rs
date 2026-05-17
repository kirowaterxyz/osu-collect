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
mod tests {
    use super::*;

    fn parse(contents: &str) -> toml::Table {
        contents.parse::<toml::Table>().unwrap()
    }

    #[test]
    fn strips_verify_zip_eocd_from_download_section() {
        let mut table = parse(
            r#"
[download]
concurrent = 8
no_video = true
verify_zip_eocd = true
"#,
        );

        assert!(strip_obsolete_fields(&mut table));
        let download = table["download"].as_table().unwrap();
        assert!(!download.contains_key("verify_zip_eocd"));
        assert_eq!(download["concurrent"].as_integer(), Some(8));
        assert_eq!(download["no_video"].as_bool(), Some(true));
    }

    #[test]
    fn is_a_noop_when_no_obsolete_fields_present() {
        let mut table = parse(
            r#"
[download]
concurrent = 4
"#,
        );

        assert!(!strip_obsolete_fields(&mut table));
        assert_eq!(table["download"]["concurrent"].as_integer(), Some(4));
    }

    #[test]
    fn is_a_noop_when_download_section_missing() {
        let mut table = parse(
            r#"
[mirror]
nerinyan = true
"#,
        );

        assert!(!strip_obsolete_fields(&mut table));
    }

    #[test]
    fn migrate_in_place_rewrites_only_when_dirty() {
        let dir = tempfile::tempdir().unwrap();

        let stale = dir.path().join("stale.toml");
        fs::write(
            &stale,
            "[download]\nconcurrent = 4\nverify_zip_eocd = true\n",
        )
        .unwrap();
        let before = fs::metadata(&stale).unwrap().modified().ok();
        std::thread::sleep(std::time::Duration::from_millis(10));
        migrate_in_place(&stale);
        let after = fs::read_to_string(&stale).unwrap();
        assert!(!after.contains("verify_zip_eocd"));
        let after_mtime = fs::metadata(&stale).unwrap().modified().ok();
        assert_ne!(before, after_mtime, "stale config must be rewritten");

        let clean = dir.path().join("clean.toml");
        fs::write(&clean, "[download]\nconcurrent = 4\n").unwrap();
        let before = fs::metadata(&clean).unwrap().modified().ok();
        std::thread::sleep(std::time::Duration::from_millis(10));
        migrate_in_place(&clean);
        let after_mtime = fs::metadata(&clean).unwrap().modified().ok();
        assert_eq!(before, after_mtime, "clean config must not be rewritten");
    }

    #[test]
    fn migrate_in_place_tolerates_missing_and_malformed_files() {
        let dir = tempfile::tempdir().unwrap();

        migrate_in_place(&dir.path().join("does-not-exist.toml"));

        let bad = dir.path().join("bad.toml");
        fs::write(&bad, "this is = not = valid toml ===").unwrap();
        let before = fs::read_to_string(&bad).unwrap();
        migrate_in_place(&bad);
        assert_eq!(fs::read_to_string(&bad).unwrap(), before);
    }
}
