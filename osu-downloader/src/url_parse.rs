//! Collection ID / URL parsing.

use crate::error::{Error, Result};

const COLLECTOR_HOST: &str = "osucollector.com";
const COLLECTOR_PATH_PREFIX: &str = "/collections/";

/// Parse a numeric collection ID or an
/// `https://osucollector.com/collections/<id>` URL.
///
/// Accepted forms:
/// - bare numeric ID (`"12345"`)
/// - `https://osucollector.com/collections/12345`
/// - `https://osucollector.com/collections/12345/`
///
/// Any other host, scheme, or path shape is rejected.
pub fn parse_collection_id(input: &str) -> Result<u32> {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return Err(Error::invalid_url("collection ID or URL cannot be empty"));
    }

    if trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return trimmed
            .parse::<u32>()
            .map_err(|_| Error::invalid_url(format!("invalid collection ID: {trimmed}")));
    }

    parse_collection_url(trimmed)
}

fn parse_collection_url(url: &str) -> Result<u32> {
    let after_scheme = url
        .strip_prefix("https://")
        .ok_or_else(|| Error::invalid_url(format!("collection URL must be HTTPS: {url}")))?;

    let (host, rest) = after_scheme
        .split_once('/')
        .ok_or_else(|| Error::invalid_url(format!("invalid collection URL: {url}")))?;

    if !host.eq_ignore_ascii_case(COLLECTOR_HOST) {
        return Err(Error::invalid_url(format!(
            "collection URL must be on {COLLECTOR_HOST}: {url}"
        )));
    }

    let path = format!("/{rest}");
    let tail = path
        .strip_prefix(COLLECTOR_PATH_PREFIX)
        .ok_or_else(|| Error::invalid_url(format!("invalid collection URL: {url}")))?;

    let id_str = tail.trim_end_matches('/');
    if id_str.is_empty() || id_str.contains('/') {
        return Err(Error::invalid_url(format!("invalid collection URL: {url}")));
    }

    id_str
        .parse::<u32>()
        .map_err(|_| Error::invalid_url(format!("collection ID must be numeric: {id_str}")))
}

#[cfg(test)]
#[path = "../tests/url_parse.rs"]
mod tests;
