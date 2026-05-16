//! Archive validation and hash computation

use crate::{DownloadError, Result};
use std::{io::SeekFrom, path::Path};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt},
    task,
};

/// ZIP local file header signature (at offset 0 for non-empty ZIPs)
const LOCAL_HEADER_SIGNATURE: &[u8] = &[0x50, 0x4B, 0x03, 0x04];

/// ZIP End of Central Directory signature
const EOCD_SIGNATURE: &[u8] = &[0x50, 0x4B, 0x05, 0x06];

/// Offset within EOCD record where "offset of start of central directory" lives (relative to EOCD start)
const EOCD_CD_OFFSET_FIELD: usize = 16;

/// Maximum bytes to search for EOCD signature
const MAX_EOCD_SEARCH_BYTES: u64 = 65536;

pub(crate) async fn validate_zip_archive(path: &Path) -> Result<()> {
    let mut file = fs::File::open(path).await?;
    let file_size = file.metadata().await?.len();

    // Minimum valid ZIP: local header (30) + EOCD (22) = 52 bytes
    if file_size < 22 {
        return Err(
            DownloadError::validation_failed("file too small to be a valid ZIP archive").into(),
        );
    }

    // Read the first 4 bytes to check local file header magic
    let mut header_buf = [0u8; 4];
    file.read_exact(&mut header_buf).await?;
    if &header_buf != LOCAL_HEADER_SIGNATURE {
        return Err(DownloadError::validation_failed(
            "ZIP local file header signature not found at offset 0",
        )
        .into());
    }

    // Read the tail to find the EOCD record
    let search_size = file_size.min(MAX_EOCD_SEARCH_BYTES);
    let search_start = file_size.saturating_sub(search_size);

    file.seek(SeekFrom::Start(search_start)).await?;

    let mut buffer = vec![0u8; search_size as usize];
    file.read_exact(&mut buffer).await?;

    task::spawn_blocking(move || {
        let eocd_pos = find_eocd_position(&buffer)
            .ok_or_else(|| DownloadError::validation_failed("ZIP EOCD signature not found"))?;

        // Central directory offset is a u32 at EOCD+16; it must be < file_size.
        let abs_eocd = search_start + eocd_pos as u64;
        let cd_offset_start = eocd_pos + EOCD_CD_OFFSET_FIELD;
        if cd_offset_start + 4 <= buffer.len() {
            let cd_offset = u32::from_le_bytes(
                buffer[cd_offset_start..cd_offset_start + 4]
                    .try_into()
                    .unwrap(),
            ) as u64;
            if cd_offset >= abs_eocd {
                return Err(DownloadError::validation_failed(
                    "ZIP central directory offset exceeds file size",
                )
                .into());
            }
        }

        Ok(())
    })
    .await
    .map_err(|err| DownloadError::worker_error(format!("validation task failed: {err}")))?
}

/// Find the position of the EOCD signature within the tail buffer.
fn find_eocd_position(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(EOCD_SIGNATURE.len())
        .rposition(|window| window == EOCD_SIGNATURE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn minimal_zip_bytes() -> Vec<u8> {
        // Minimal valid ZIP: one empty stored file + EOCD.
        // Local file header for empty file named "a"
        let local_header: &[u8] = &[
            0x50, 0x4B, 0x03, 0x04, // local file header sig
            0x14, 0x00, // version needed
            0x00, 0x00, // flags
            0x00, 0x00, // compression (stored)
            0x00, 0x00, 0x00, 0x00, // mod time/date
            0x00, 0x00, 0x00, 0x00, // crc32
            0x00, 0x00, 0x00, 0x00, // compressed size
            0x00, 0x00, 0x00, 0x00, // uncompressed size
            0x01, 0x00, // filename length = 1
            0x00, 0x00, // extra field length
            b'a', // filename
        ];
        // Central directory header
        let cd_header: &[u8] = &[
            0x50, 0x4B, 0x01, 0x02, // central dir sig
            0x14, 0x00, // version made by
            0x14, 0x00, // version needed
            0x00, 0x00, // flags
            0x00, 0x00, // compression
            0x00, 0x00, 0x00, 0x00, // mod time/date
            0x00, 0x00, 0x00, 0x00, // crc32
            0x00, 0x00, 0x00, 0x00, // compressed size
            0x00, 0x00, 0x00, 0x00, // uncompressed size
            0x01, 0x00, // filename length
            0x00, 0x00, // extra field length
            0x00, 0x00, // comment length
            0x00, 0x00, // disk start
            0x00, 0x00, // internal attrs
            0x00, 0x00, 0x00, 0x00, // external attrs
            0x00, 0x00, 0x00, 0x00, // local header offset
            b'a', // filename
        ];
        let local_len = local_header.len() as u32;
        let cd_len = cd_header.len() as u32;
        // EOCD
        let eocd: Vec<u8> = {
            let mut v = vec![
                0x50, 0x4B, 0x05, 0x06, // EOCD sig
                0x00, 0x00, // disk number
                0x00, 0x00, // disk with CD
                0x01, 0x00, // entries on disk
                0x01, 0x00, // total entries
            ];
            v.extend_from_slice(&cd_len.to_le_bytes()); // CD size
            v.extend_from_slice(&local_len.to_le_bytes()); // CD offset = after local header
            v.extend_from_slice(&[0x00, 0x00]); // comment length
            v
        };
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
        assert!(validate_zip_archive(tmp.path()).await.is_ok());
    }

    #[tokio::test]
    async fn zeros_with_eocd_tail_fails_local_header_check() {
        // File is all zeros except for the EOCD magic at the end — no local header at offset 0.
        let mut data = vec![0u8; 64];
        let eocd_magic = &[0x50u8, 0x4B, 0x05, 0x06];
        let tail = data.len() - 4;
        data[tail..].copy_from_slice(eocd_magic);

        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(&data).unwrap();
        assert!(validate_zip_archive(tmp.path()).await.is_err());
    }

    #[test]
    fn find_eocd_returns_last_occurrence() {
        let mut buf = vec![0u8; 64];
        // Two EOCD signatures; rposition should return the last one.
        buf[10..14].copy_from_slice(EOCD_SIGNATURE);
        buf[40..44].copy_from_slice(EOCD_SIGNATURE);
        assert_eq!(find_eocd_position(&buf), Some(40));
    }
}
