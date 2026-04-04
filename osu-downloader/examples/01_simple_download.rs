//! Simple example: Download a single beatmapset
//!
//! Usage: cargo run --example 01_simple_download

use osu_downloader::{DownloadResult, Downloader};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a downloader with default mirrors
    let downloader = Downloader::builder()
        .default_mirrors()
        .verify_archives(true)
        .build()?;

    println!("Downloading beatmapset #41823 (The Big Black)...");

    // Download a single beatmapset
    let result = downloader.download_one(41823, "./downloads").await?;

    match result {
        DownloadResult::Success {
            filename,
            size_bytes,
            md5_hash,
        } => {
            println!("✓ Downloaded: {}", filename);
            println!("  Size: {} bytes", size_bytes);
            if let Some(hash) = md5_hash {
                println!("  MD5: {}", hash);
            }
        }
        DownloadResult::Skipped { reason } => {
            println!("⊘ Skipped: {:?}", reason);
        }
    }

    Ok(())
}
