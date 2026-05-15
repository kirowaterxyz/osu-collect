pub mod error;
pub mod fs;
pub mod logging;

pub use error::{AppError, Result};
pub use fs::{
    FileExistsAction, check_available_space, determine_file_exists_action, format_bytes,
    is_low_disk_space, validate_and_prepare_directory,
};
pub use logging::init_logging;

use url::Url;

pub fn parse_collection_id(input: &str) -> Result<u32> {
    let trimmed = input.trim();

    if trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return trimmed.parse::<u32>().map_err(|_| {
            AppError::invalid_url_dynamic(
                format!("Invalid collection ID: {trimmed}").into_boxed_str(),
            )
        });
    }

    if trimmed.is_empty() {
        return Err(AppError::invalid_url(
            "Collection ID or URL cannot be empty",
        ));
    }

    let url = Url::parse(trimmed).map_err(|_| {
        AppError::invalid_url_dynamic(
            format!("Invalid URL or collection ID: {trimmed}").into_boxed_str(),
        )
    })?;

    if url.host_str() != Some("osucollector.com") {
        return Err(AppError::invalid_url("URL must be from osucollector.com"));
    }

    if url.scheme() != "https" {
        return Err(AppError::invalid_url("URL must use HTTPS protocol"));
    }

    let path_segments: Vec<&str> = url
        .path_segments()
        .ok_or(AppError::invalid_url("Invalid URL path"))?
        .collect();

    if path_segments.len() < 2 || path_segments[0] != "collections" {
        return Err(AppError::invalid_url(
            "URL must be in format: https://osucollector.com/collections/{id}",
        ));
    }

    let id = path_segments[1];

    id.parse::<u32>().map_err(|_| {
        AppError::invalid_url_dynamic(
            format!("Collection ID must be numeric, got: {id}").into_boxed_str(),
        )
    })
}

pub fn sanitize_filename(filename: &str) -> String {
    let mut sanitized = String::with_capacity(filename.len());

    for c in filename.chars() {
        sanitized.push(match c {
            '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            other => other,
        });
    }

    let mut start = sanitized.len();
    let mut end = 0;

    for (idx, ch) in sanitized.char_indices() {
        if !ch.is_whitespace() {
            start = idx;
            break;
        }
    }

    for (idx, ch) in sanitized.char_indices().rev() {
        if !ch.is_whitespace() {
            end = idx + ch.len_utf8();
            break;
        }
    }

    if start >= end {
        sanitized.clear();
    } else {
        sanitized.drain(..start);
        sanitized.truncate(end - start);
    }

    sanitized
}

pub fn sanitize_filename_safe(filename: &str, beatmapset_id: u32) -> String {
    use std::path::Path;

    let sanitized = sanitize_filename(filename);

    match Path::new(&sanitized).file_name() {
        Some(name) if name != "." && name != ".." => name.to_string_lossy().into_owned(),
        _ => format!("{beatmapset_id}.osz"),
    }
}
