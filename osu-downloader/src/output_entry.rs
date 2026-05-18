//! Classification helpers for entries in a downloader output directory.

use std::ffi::OsStr;

/// Classification of a single filename inside an output directory used by the downloader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputEntry {
    /// A `<id>.osz` (or `<id> name.osz`) archive belonging to `beatmapset_id`.
    Archive {
        /// Beatmapset id parsed from the filename.
        beatmapset_id: u32,
    },
    /// A leftover temp file from a cancelled or crashed download
    /// (`<name>.download-<pid>-<counter>.tmp`).
    OrphanTemp,
    /// Anything else (foreign files, partial writes from other tools, etc.).
    Other,
}

/// Classify a single directory entry by filename only. No filesystem access.
///
/// This is the single helper consumers should use to walk a downloader output
/// directory — it owns the knowledge of the on-disk filename conventions
/// (archive naming and temp-file format) so callers don't have to.
pub fn classify_output_entry(name: &OsStr) -> OutputEntry {
    let Some(name_str) = name.to_str() else {
        return OutputEntry::Other;
    };
    if let Some(id) = parse_beatmapset_filename(name_str) {
        return OutputEntry::Archive { beatmapset_id: id };
    }
    if is_temp_download_name(name_str) {
        return OutputEntry::OrphanTemp;
    }
    OutputEntry::Other
}

/// Parse a `<id>.osz` or `<id> name.osz` filename into its beatmapset id.
/// Case-insensitive on the `.osz` extension.
pub(crate) fn parse_beatmapset_filename(name: &str) -> Option<u32> {
    let digits_len = name.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits_len == 0 {
        return None;
    }
    let id: u32 = name[..digits_len].parse().ok()?;
    let rest = &name[digits_len..];
    let lower = rest.to_ascii_lowercase();
    if lower == ".osz" {
        return Some(id);
    }
    if rest.starts_with(' ') && lower.ends_with(".osz") {
        return Some(id);
    }
    None
}

fn is_temp_download_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".tmp") else {
        return false;
    };
    let Some(idx) = stem.find(".download-") else {
        return false;
    };
    let tail = &stem[idx + ".download-".len()..];
    let mut parts = tail.splitn(2, '-');
    let pid = parts.next().unwrap_or("");
    let counter = parts.next().unwrap_or("");
    !pid.is_empty()
        && !counter.is_empty()
        && pid.bytes().all(|b| b.is_ascii_digit())
        && counter.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn classify(s: &str) -> OutputEntry {
        classify_output_entry(&OsString::from(s))
    }

    #[test]
    fn parses_archive_with_only_id() {
        assert_eq!(
            classify("123.osz"),
            OutputEntry::Archive { beatmapset_id: 123 }
        );
    }

    #[test]
    fn parses_archive_with_name_suffix() {
        assert_eq!(
            classify("456 some title.osz"),
            OutputEntry::Archive { beatmapset_id: 456 }
        );
    }

    #[test]
    fn accepts_uppercase_extension() {
        assert_eq!(
            classify("789.OSZ"),
            OutputEntry::Archive { beatmapset_id: 789 }
        );
        assert_eq!(
            classify("789 foo.OsZ"),
            OutputEntry::Archive { beatmapset_id: 789 }
        );
    }

    #[test]
    fn rejects_non_archive() {
        assert_eq!(classify("foo.osz"), OutputEntry::Other);
        assert_eq!(classify("123name.osz"), OutputEntry::Other);
        assert_eq!(classify("123.txt"), OutputEntry::Other);
        assert_eq!(classify("collection.db"), OutputEntry::Other);
    }

    #[test]
    fn detects_orphan_temp_file() {
        assert_eq!(
            classify("123.osz.download-987-3.tmp"),
            OutputEntry::OrphanTemp
        );
        assert_eq!(
            classify("download.download-1-0.tmp"),
            OutputEntry::OrphanTemp
        );
    }

    #[test]
    fn rejects_non_temp() {
        assert_eq!(classify("123.osz.tmp"), OutputEntry::Other);
        assert_eq!(classify("foo.download-.tmp"), OutputEntry::Other);
        assert_eq!(classify("foo.download-abc-1.tmp"), OutputEntry::Other);
    }
}
