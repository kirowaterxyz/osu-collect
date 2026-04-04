//! Archive validation and hash computation

use crate::{DownloadError, Result};
use bytes::Bytes;
use md5::{Digest, Md5};
use std::{io::SeekFrom, path::Path, sync::mpsc};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt},
    task,
};

/// ZIP End of Central Directory signature
const EOCD_SIGNATURE: &[u8] = &[0x50, 0x4B, 0x05, 0x06];

/// Maximum bytes to search for EOCD signature
const MAX_EOCD_SEARCH_BYTES: u64 = 65536;

/// Hash worker for background MD5 computation
pub(crate) struct HashWorker {
    sender: Option<mpsc::Sender<Bytes>>,
    handle: task::JoinHandle<String>,
}

impl HashWorker {
    /// Create a new hash worker
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<Bytes>();
        let handle = task::spawn_blocking(move || {
            let mut hasher = Md5::new();
            while let Ok(chunk) = receiver.recv() {
                hasher.update(&chunk);
            }
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect()
        });
        Self {
            sender: Some(sender),
            handle,
        }
    }

    /// Update hash with new data
    pub fn update(&self, data: Bytes) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(data);
        }
    }

    /// Finalize hash computation and get result
    pub async fn finalize(mut self) -> Result<String> {
        self.sender.take();
        self.handle
            .await
            .map_err(|err| DownloadError::worker_error(format!("Hash worker failed: {err}")))
            .map_err(Into::into)
    }

    /// Abort hash computation
    pub fn abort(mut self) {
        self.sender.take();
        self.handle.abort();
    }
}

/// Validate that a file is a valid ZIP archive
///
/// Checks for the ZIP End of Central Directory (EOCD) signature
pub async fn validate_zip_archive(path: &Path) -> Result<()> {
    let mut file = fs::File::open(path).await?;
    let file_size = file.metadata().await?.len();

    if file_size < 22 {
        return Err(
            DownloadError::validation_failed("File too small to be a valid ZIP archive").into(),
        );
    }

    let search_size = file_size.min(MAX_EOCD_SEARCH_BYTES);
    let search_start = file_size.saturating_sub(search_size);

    file.seek(SeekFrom::Start(search_start)).await?;

    let mut buffer = vec![0u8; search_size as usize];
    file.read_exact(&mut buffer).await?;

    if has_eocd_signature(&buffer) {
        Ok(())
    } else {
        Err(DownloadError::validation_failed("ZIP EOCD signature not found").into())
    }
}

/// Check if buffer contains ZIP EOCD signature
fn has_eocd_signature(buffer: &[u8]) -> bool {
    buffer
        .windows(EOCD_SIGNATURE.len())
        .any(|window| window == EOCD_SIGNATURE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_eocd_signature() {
        let valid = vec![0x00, 0x50, 0x4B, 0x05, 0x06, 0x00];
        assert!(has_eocd_signature(&valid));

        let invalid = vec![0x00, 0x01, 0x02, 0x03, 0x04];
        assert!(!has_eocd_signature(&invalid));
    }

    #[tokio::test]
    async fn test_hash_worker() {
        let worker = HashWorker::new();
        worker.update(Bytes::from("test"));
        worker.update(Bytes::from("data"));

        // Note: actual hash computation would be tested with real data
        let result = worker.finalize().await;
        assert!(result.is_ok());
    }
}
