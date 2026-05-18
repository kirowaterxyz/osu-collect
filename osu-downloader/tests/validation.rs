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
async fn off_mode_skips_all_checks() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(b"not a zip at all").unwrap();
    assert!(
        ensure_valid_archive(tmp.path(), ArchiveValidation::Off)
            .await
            .is_ok()
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
async fn zip_with_odd_eocd_offset_still_passes() {
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
