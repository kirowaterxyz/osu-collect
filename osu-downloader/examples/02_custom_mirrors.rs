//! Example: Using custom mirrors
//!
//! Usage: cargo run --example 02_custom_mirrors

use osu_downloader::{Downloader, Mirror};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a downloader with specific mirrors
    let downloader = Downloader::builder()
        .mirror(Mirror::nerinyan())
        .mirror(Mirror::sayobot())
        .mirror(Mirror::osu_direct())
        .concurrent_downloads(8)
        .max_retries(5)
        .no_video(true) // Skip videos for smaller downloads
        .build()?;

    println!("Downloader configured with:");
    println!("  - Nerinyan");
    println!("  - Sayobot");
    println!("  - osu.direct");
    println!("  - No video mode enabled");
    println!();

    let beatmapset_id = 41823;
    println!("Downloading beatmapset #{}...", beatmapset_id);

    match downloader
        .download_one(beatmapset_id, "./downloads")
        .await?
    {
        osu_downloader::DownloadResult::Success {
            filename,
            size_bytes,
            ..
        } => {
            println!("✓ Success! Downloaded {}", filename);
            println!("  Size: {} MB", size_bytes / 1_048_576);
        }
        osu_downloader::DownloadResult::Skipped { reason } => {
            println!("⊘ Skipped: {:?}", reason);
        }
    }

    Ok(())
}
