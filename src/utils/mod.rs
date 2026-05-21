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
