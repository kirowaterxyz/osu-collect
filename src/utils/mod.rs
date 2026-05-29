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
/// Drives alt/ctrl+backspace and ctrl+w in text inputs. Operating on the end of
/// the string matches the append-only editing model (the caret is always at the
/// end). Path/URL friendly: `/home/user/foo` → `/home/user/` → `/home/`.
pub fn delete_last_word(s: &mut String) {
    let after_separators = s.trim_end_matches(|c: char| !c.is_alphanumeric());
    let after_word = after_separators.trim_end_matches(|c: char| c.is_alphanumeric());
    s.truncate(after_word.len());
}

#[cfg(test)]
#[path = "../../tests/unit/utils_word.rs"]
mod word_tests;
