//! Archive validation and hash computation

use crate::{DownloadError, Result};
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

/// Archive validation options.
#[derive(Clone, Copy, Debug, Default)]
pub struct ArchiveValidationOptions {
    /// Validation strictness.
    pub mode: ArchiveValidation,
    /// Whether invalid archives should be removed.
    pub remove_on_invalid: bool,
}

/// Archive validation outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchiveValidationResult {
    /// The archive passed validation.
    Valid,
    /// The archive path does not exist.
    NotFound,
    /// The archive failed validation and remains on disk.
    Invalid(String),
    /// The archive failed validation and was removed.
    Removed(String),
}

/// Validate that a file looks like an osu! beatmap archive.
pub async fn ensure_valid_archive(path: &Path, mode: ArchiveValidation) -> Result<()> {
    if mode == ArchiveValidation::Off {
        return Ok(());
    }

    let metadata = fs::metadata(path).await?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(DownloadError::validation_failed("downloaded file is empty or invalid").into());
    }

    let mut file = fs::File::open(path).await?;
    let mut header = [0u8; 64];
    let bytes_read = file.read(&mut header).await?;

    if bytes_read < 4 {
        return Err(
            DownloadError::validation_failed("file too small to be a valid archive").into(),
        );
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
        return Err(DownloadError::validation_failed(
            "received HTML error page instead of beatmap archive",
        )
        .into());
    }

    Err(DownloadError::validation_failed("invalid archive: missing ZIP signature").into())
}

/// Validate an archive path and optionally remove invalid files.
pub async fn validate_archive(
    path: &Path,
    options: ArchiveValidationOptions,
) -> Result<ArchiveValidationResult> {
    let metadata = match fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Ok(ArchiveValidationResult::NotFound);
        }
        Err(err) => return Err(err.into()),
    };

    if !metadata.is_file() {
        return handle_invalid(path, "not a regular file", options.remove_on_invalid).await;
    }

    if metadata.len() == 0 {
        return handle_invalid(path, "file is empty", options.remove_on_invalid).await;
    }

    if let Err(err) = ensure_valid_archive(path, options.mode).await {
        return handle_invalid(path, &err.to_string(), options.remove_on_invalid).await;
    }

    Ok(ArchiveValidationResult::Valid)
}

async fn verify_zip_eocd_footer(file: &mut fs::File, file_size: u64) -> Result<()> {
    if file_size < 22 {
        return Err(DownloadError::validation_failed(
            "invalid archive: missing central directory footer",
        )
        .into());
    }

    let search_len = MAX_EOCD_SEARCH_BYTES.min(file_size);
    file.seek(SeekFrom::End(-(search_len as i64))).await?;
    let mut buffer = vec![0u8; search_len as usize];
    file.read_exact(&mut buffer).await?;

    if find_eocd_position(&buffer).is_some() {
        Ok(())
    } else {
        Err(
            DownloadError::validation_failed("invalid archive: missing central directory footer")
                .into(),
        )
    }
}

async fn handle_invalid(
    path: &Path,
    reason: &str,
    remove: bool,
) -> Result<ArchiveValidationResult> {
    if remove {
        match fs::remove_file(path).await {
            Ok(()) => return Ok(ArchiveValidationResult::Removed(reason.to_string())),
            Err(err) if err.kind() == ErrorKind::NotFound => {
                return Ok(ArchiveValidationResult::Removed(reason.to_string()));
            }
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
pub(crate) mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    pub(crate) fn minimal_zip_bytes_for_test() -> Vec<u8> {
        minimal_zip_bytes()
    }

    fn minimal_zip_bytes() -> Vec<u8> {
        let local_header: &[u8] = &[
            0x50, 0x4B, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, b'a',
        ];
        let cd_header: &[u8] = &[
            0x50, 0x4B, 0x01, 0x02, 0x14, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'a',
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

    #[tokio::test]
    async fn valid_zip_passes() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&minimal_zip_bytes()).unwrap();
        assert!(ensure_valid_archive(tmp.path(), ArchiveValidation::Eocd)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn lenient_validation_allows_header_only_archive() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(LOCAL_HEADER_SIGNATURE).unwrap();
        assert!(ensure_valid_archive(tmp.path(), ArchiveValidation::Magic)
            .await
            .is_ok());
        assert!(ensure_valid_archive(tmp.path(), ArchiveValidation::Eocd)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn off_mode_skips_all_checks() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"not a zip at all").unwrap();
        assert!(ensure_valid_archive(tmp.path(), ArchiveValidation::Off)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn validate_archive_removes_invalid_files() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"not a zip").unwrap();
        let path = tmp.path().to_path_buf();
        let result = validate_archive(
            &path,
            ArchiveValidationOptions {
                mode: ArchiveValidation::Magic,
                remove_on_invalid: true,
            },
        )
        .await
        .unwrap();
        assert!(matches!(result, ArchiveValidationResult::Removed(_)));
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn zip_with_odd_eocd_offset_still_passes() {
        let mut data = minimal_zip_bytes();
        let eocd_pos = find_eocd_position(&data).unwrap();
        data[eocd_pos + 16..eocd_pos + 20].copy_from_slice(&(u32::MAX).to_le_bytes());

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&data).unwrap();
        assert!(ensure_valid_archive(tmp.path(), ArchiveValidation::Eocd)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn zeros_with_eocd_tail_fails_local_header_check() {
        let mut data = vec![0u8; 64];
        let tail = data.len() - EOCD_SIGNATURE.len();
        data[tail..].copy_from_slice(EOCD_SIGNATURE);

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&data).unwrap();
        assert!(ensure_valid_archive(tmp.path(), ArchiveValidation::Eocd)
            .await
            .is_err());
    }

    #[test]
    fn find_eocd_returns_last_occurrence() {
        let mut buf = vec![0u8; 64];
        buf[10..14].copy_from_slice(EOCD_SIGNATURE);
        buf[40..44].copy_from_slice(EOCD_SIGNATURE);
        assert_eq!(find_eocd_position(&buf), Some(40));
    }
}
