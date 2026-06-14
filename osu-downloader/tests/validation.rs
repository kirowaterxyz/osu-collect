use super::ensure_valid_archive;
use super::{
    EOCD_SIGNATURE, LOCAL_HEADER_SIGNATURE, find_eocd_position, minimal_zip_bytes_for_test,
};
use crate::{ArchiveValidation, ArchiveValidationResult, validate_and_remove};
use std::io::Write;
use tempfile::NamedTempFile;

#[tokio::test]
async fn valid_zip_passes() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(&minimal_zip_bytes_for_test()).unwrap();
    assert!(
        ensure_valid_archive(tmp.path(), ArchiveValidation::Eocd)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn lenient_validation_allows_header_only_archive() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(LOCAL_HEADER_SIGNATURE).unwrap();
    assert!(
        ensure_valid_archive(tmp.path(), ArchiveValidation::Magic)
            .await
            .is_ok()
    );
    assert!(
        ensure_valid_archive(tmp.path(), ArchiveValidation::Eocd)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn off_mode_skips_zip_shape_check() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(b"not a zip at all").unwrap();
    assert!(
        ensure_valid_archive(tmp.path(), ArchiveValidation::Off)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn off_mode_still_rejects_empty_files() {
    let tmp = NamedTempFile::new().unwrap();
    assert!(
        ensure_valid_archive(tmp.path(), ArchiveValidation::Off)
            .await
            .is_err(),
        "Off must still reject 0-byte files"
    );
}

#[tokio::test]
async fn validate_and_remove_deletes_invalid_files() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(b"not a zip").unwrap();
    let path = tmp.path().to_path_buf();
    let result = validate_and_remove(&path, ArchiveValidation::Magic)
        .await
        .unwrap();
    assert!(matches!(result, ArchiveValidationResult::Invalid(_)));
    assert!(!path.exists());
}

#[tokio::test]
async fn eocd_zip64_sentinel_offset_passes() {
    // 0xFFFFFFFF in the CD-offset field marks a ZIP64 archive whose real offsets
    // live in a separate ZIP64 record; strict accepts rather than false-rejecting.
    let mut data = minimal_zip_bytes_for_test();
    let eocd_pos = find_eocd_position(&data).unwrap();
    data[eocd_pos + 16..eocd_pos + 20].copy_from_slice(&(u32::MAX).to_le_bytes());

    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(&data).unwrap();
    assert!(
        ensure_valid_archive(tmp.path(), ArchiveValidation::Eocd)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn eocd_with_inconsistent_cd_bounds_fails() {
    // A wrong, non-sentinel CD offset means the directory no longer abuts the
    // footer: strict must reject it while basic (magic-only) still accepts.
    let mut data = minimal_zip_bytes_for_test();
    let eocd_pos = find_eocd_position(&data).unwrap();
    data[eocd_pos + 16..eocd_pos + 20].copy_from_slice(&5u32.to_le_bytes());

    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(&data).unwrap();
    assert!(
        ensure_valid_archive(tmp.path(), ArchiveValidation::Magic)
            .await
            .is_ok()
    );
    assert!(
        ensure_valid_archive(tmp.path(), ArchiveValidation::Eocd)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn zeros_with_eocd_tail_fails_local_header_check() {
    let mut data = vec![0u8; 64];
    let tail = data.len() - EOCD_SIGNATURE.len();
    data[tail..].copy_from_slice(EOCD_SIGNATURE);

    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(&data).unwrap();
    assert!(
        ensure_valid_archive(tmp.path(), ArchiveValidation::Eocd)
            .await
            .is_err()
    );
}

#[test]
fn find_eocd_returns_last_occurrence() {
    let mut buf = vec![0u8; 64];
    buf[10..14].copy_from_slice(EOCD_SIGNATURE);
    buf[40..44].copy_from_slice(EOCD_SIGNATURE);
    assert_eq!(find_eocd_position(&buf), Some(40));
}

#[test]
fn find_eocd_empty_returns_none() {
    assert_eq!(find_eocd_position(&[]), None);
}

#[test]
fn find_eocd_shorter_than_signature_returns_none() {
    assert_eq!(find_eocd_position(&[0x50, 0x4B, 0x05]), None);
}

#[test]
fn find_eocd_exactly_four_bytes_matches() {
    assert_eq!(find_eocd_position(EOCD_SIGNATURE), Some(0));
}

#[test]
fn find_eocd_truncated_at_end_returns_none() {
    // Signature split across the very last bytes — should not match.
    let mut buf = vec![0u8; 8];
    buf[6..8].copy_from_slice(&EOCD_SIGNATURE[..2]);
    assert_eq!(find_eocd_position(&buf), None);
}

#[test]
fn find_eocd_signature_at_very_end() {
    let mut buf = vec![0u8; 32];
    let end = buf.len() - 4;
    buf[end..].copy_from_slice(EOCD_SIGNATURE);
    assert_eq!(find_eocd_position(&buf), Some(end));
}

#[test]
fn find_eocd_false_positive_0x50_before_real_signature() {
    // Buffer contains a lone 0x50 before the real EOCD signature.
    let mut buf = vec![0u8; 32];
    buf[5] = 0x50;
    buf[20..24].copy_from_slice(EOCD_SIGNATURE);
    assert_eq!(find_eocd_position(&buf), Some(20));
}
