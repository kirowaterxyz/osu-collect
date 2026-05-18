//! Archive validation.

use crate::{Error, Result};
use std::{io::ErrorKind, io::SeekFrom, path::Path};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt},
};

const LOCAL_HEADER_SIGNATURE: &[u8] = &[0x50, 0x4B, 0x03, 0x04];
const EOCD_SIGNATURE: &[u8] = &[0x50, 0x4B, 0x05, 0x06];
const MAX_EOCD_SEARCH_BYTES: u64 = 65_536;

/// Archive validation strictness.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveValidation {
    /// Skip validation entirely.
    Off,
    /// Require the ZIP local-file-header magic bytes only (default).
    #[default]
    Magic,
    /// Also require the ZIP end-of-central-directory footer.
    Eocd,
}

/// Archive validation outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchiveValidationResult {
    /// The archive passed validation.
    Valid,
    /// The archive path does not exist.
    NotFound,
    /// The archive failed validation. The reason is the underlying error message.
    /// If the call site was [`validate_and_remove`], the file has also been deleted.
    Invalid(String),
}

/// Validate that a file looks like an osu! beatmap archive (internal helper used by the downloader pipeline).
pub(crate) async fn ensure_valid_archive(path: &Path, mode: ArchiveValidation) -> Result<()> {
    if mode == ArchiveValidation::Off {
        return Ok(());
    }

    let metadata = fs::metadata(path).await?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(Error::validation("downloaded file is empty or invalid"));
    }

    let mut file = fs::File::open(path).await?;
    let mut header = [0u8; 64];
    let bytes_read = file.read(&mut header).await?;

    if bytes_read < 4 {
        return Err(Error::validation("file too small to be a valid archive"));
    }

    if &header[..LOCAL_HEADER_SIGNATURE.len()] == LOCAL_HEADER_SIGNATURE {
        if mode == ArchiveValidation::Eocd {
            verify_zip_eocd_footer(&mut file, metadata.len()).await?;
        }
        return Ok(());
    }

    let trimmed = trim_leading_whitespace(&header[..bytes_read]);
    if trimmed.starts_with(b"<!DOCTYPE")
        || trimmed.starts_with(b"<!doctype")
        || trimmed.starts_with(b"<html")
        || trimmed.starts_with(b"<HTML")
    {
        return Err(Error::validation(
            "received HTML error page instead of beatmap archive",
        ));
    }

    Err(Error::validation("invalid archive: missing ZIP signature"))
}

/// Validate an archive at `path` without modifying the file.
///
/// Returns:
/// - [`ArchiveValidationResult::Valid`] — the file looks like a real archive
/// - [`ArchiveValidationResult::NotFound`] — no file at this path
/// - [`ArchiveValidationResult::Invalid`] — the file exists but failed validation; reason describes why
pub async fn validate_archive(
    path: &Path,
    mode: ArchiveValidation,
) -> Result<ArchiveValidationResult> {
    inspect_archive(path, mode, false).await
}

/// Like [`validate_archive`], but deletes any file that fails validation before returning.
///
/// A successful removal still returns [`ArchiveValidationResult::Invalid`] — the variant
/// records that the file failed; the deletion is a side effect.
pub async fn validate_and_remove(
    path: &Path,
    mode: ArchiveValidation,
) -> Result<ArchiveValidationResult> {
    inspect_archive(path, mode, true).await
}

async fn inspect_archive(
    path: &Path,
    mode: ArchiveValidation,
    remove_on_invalid: bool,
) -> Result<ArchiveValidationResult> {
    let metadata = match fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Ok(ArchiveValidationResult::NotFound);
        }
        Err(err) => return Err(err.into()),
    };

    if !metadata.is_file() {
        return handle_invalid(path, "not a regular file", remove_on_invalid).await;
    }

    if metadata.len() == 0 {
        return handle_invalid(path, "file is empty", remove_on_invalid).await;
    }

    if let Err(err) = ensure_valid_archive(path, mode).await {
        return handle_invalid(path, &err.to_string(), remove_on_invalid).await;
    }

    Ok(ArchiveValidationResult::Valid)
}

async fn verify_zip_eocd_footer(file: &mut fs::File, file_size: u64) -> Result<()> {
    if file_size < 22 {
        return Err(Error::validation(
            "invalid archive: missing central directory footer",
        ));
    }

    let search_len = MAX_EOCD_SEARCH_BYTES.min(file_size);
    file.seek(SeekFrom::End(-(search_len as i64))).await?;
    let mut buffer = vec![0u8; search_len as usize];
    file.read_exact(&mut buffer).await?;

    if find_eocd_position(&buffer).is_some() {
        Ok(())
    } else {
        Err(Error::validation(
            "invalid archive: missing central directory footer",
        ))
    }
}

async fn handle_invalid(
    path: &Path,
    reason: &str,
    remove: bool,
) -> Result<ArchiveValidationResult> {
    if remove {
        match fs::remove_file(path).await {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }

    Ok(ArchiveValidationResult::Invalid(reason.to_string()))
}

fn trim_leading_whitespace(data: &[u8]) -> &[u8] {
    let start = data
        .iter()
        .position(|&byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        .unwrap_or(data.len());
    &data[start..]
}

fn find_eocd_position(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(EOCD_SIGNATURE.len())
        .rposition(|window| window == EOCD_SIGNATURE)
}

#[cfg(test)]
pub(crate) fn minimal_zip_bytes_for_test() -> Vec<u8> {
    let local_header: &[u8] = &[
        0x50, 0x4B, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, b'a',
    ];
    let cd_header: &[u8] = &[
        0x50, 0x4B, 0x01, 0x02, 0x14, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'a',
    ];
    let local_len = local_header.len() as u32;
    let cd_len = cd_header.len() as u32;
    let mut eocd = vec![
        0x50, 0x4B, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
    ];
    eocd.extend_from_slice(&cd_len.to_le_bytes());
    eocd.extend_from_slice(&local_len.to_le_bytes());
    eocd.extend_from_slice(&[0x00, 0x00]);

    let mut zip = Vec::new();
    zip.extend_from_slice(local_header);
    zip.extend_from_slice(cd_header);
    zip.extend_from_slice(&eocd);
    zip
}

#[cfg(test)]
#[path = "../tests/validation.rs"]
mod tests;
