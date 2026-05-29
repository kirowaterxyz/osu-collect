pub mod error;
pub mod fs;
pub mod logging;

pub use error::{AppError, Result};
pub use fs::{
    CompletionResult, complete_dir, expand_tilde, format_bytes, is_low_disk_space,
    prepare_directory, pretty_path,
};

pub use logging::init_logging;

pub fn parse_collection_id(input: &str) -> Result<u32> {
    osu_downloader::parse_collection_id(input)
        .map_err(|err| AppError::invalid_url_dynamic(err.to_string().into_boxed_str()))
}

/// Delete the last "word" from `s` in place: trailing separators (any
/// non-alphanumeric run) followed by the trailing alphanumeric run.
///
/// Path/URL friendly: `/home/user/foo` → `/home/user/` → `/home/`. Thin wrapper
/// over [`delete_word_left`] anchored at the end of the string.
pub fn delete_last_word(s: &mut String) {
    delete_word_left(s, s.chars().count());
}

/// Delete the word immediately left of the caret (`caret` is a **char index**).
/// Removes the run of trailing separators then the alphanumeric run to its
/// left, deleting only text in `0..caret` and leaving everything at and after
/// the caret untouched. Returns the new caret char index (the deletion start).
///
/// Drives alt/ctrl+backspace and ctrl+w/h in text inputs. Path/URL friendly:
/// `/home/user/foo` with the caret at the end → `/home/user/`.
pub fn delete_word_left(s: &mut String, caret: usize) -> usize {
    let caret = caret.min(s.chars().count());
    // Split at the caret so the word logic only sees text to its left; chars
    // at/after the caret are preserved verbatim.
    let caret_byte = s
        .char_indices()
        .nth(caret)
        .map(|(byte, _)| byte)
        .unwrap_or(s.len());
    let left = &s[..caret_byte];

    let after_separators = left.trim_end_matches(|c: char| !c.is_alphanumeric());
    let after_word = after_separators.trim_end_matches(|c: char| c.is_alphanumeric());
    let start_byte = after_word.len();

    s.replace_range(start_byte..caret_byte, "");
    s[..start_byte].chars().count()
}

#[cfg(test)]
#[path = "../../tests/unit/utils_word.rs"]
mod word_tests;
