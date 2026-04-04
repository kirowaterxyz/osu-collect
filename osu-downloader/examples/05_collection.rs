//! Example: Download a collection from osucollector.com
//!
//! Usage: cargo run --example 05_collection --features collection

use osu_downloader::{collection::CollectionClient, DownloadEvent, Downloader};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Collection ID 1 (a popular collection on osucollector.com)
    let collection_id = 1;

    println!("Fetching collection {}...", collection_id);

    // Create collection client
    let collection_client = CollectionClient::new()?;

    // Fetch collection metadata
    let collection = collection_client.fetch(collection_id).await?;

    println!();
    println!("╔═══════════════════════════════════════════════╗");
    println!("  Collection: {}", collection.name);
    println!("  Uploader: {}", collection.uploader.username);
    println!("  Beatmapsets: {}", collection.beatmapset_ids().len());
    println!("  Favourites: {}", collection.favourites);
    println!("╚═══════════════════════════════════════════════╝");
    println!();

    if let Some(desc) = &collection.description {
        println!("Description: {}", desc);
        println!();
    }

    // Create downloader
    let downloader = Downloader::builder()
        .default_mirrors()
        .concurrent_downloads(6)
        .build()?;

    println!("Starting download...");
    println!();

    // Download collection
    let mut session = collection.download(&downloader, "./downloads").await;

    while let Some(event) = session.next_event().await {
        match event {
            DownloadEvent::BeatmapsetStarted {
                beatmapset_id,
                mirror,
            } => {
                print!("⬇️  Downloading {} from {:?}... ", beatmapset_id, mirror);
            }

            DownloadEvent::BeatmapsetCompleted {
                filename,
                size_bytes,
                ..
            } => {
                println!("✓ ({:.2} MB)", size_bytes as f64 / 1_048_576.0);
                println!("   {}", filename);
            }

            DownloadEvent::BeatmapsetFailed {
                beatmapset_id,
                error,
                ..
            } => {
                println!("✗");
                println!("   Failed to download {}: {}", beatmapset_id, error);
            }

            DownloadEvent::BeatmapsetSkipped {
                beatmapset_id,
                reason,
            } => {
                println!("⊘  Skipped {}: {:?}", beatmapset_id, reason);
            }

            DownloadEvent::Progress {
                downloaded_bytes,
                total_bytes,
                speed_bps,
                ..
            } => {
                let progress_pct = if let Some(total) = total_bytes {
                    (downloaded_bytes as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                let speed_mbps = speed_bps as f64 / 1_048_576.0;

                print!(
                    "\r   Progress: {:.1}% | Speed: {:.2} MB/s    ",
                    progress_pct, speed_mbps
                );
            }

            DownloadEvent::SessionCompleted { summary } => {
                println!();
                println!();
                println!("═══════════════════════════════════════════════");
                println!("             Download Complete                 ");
                println!("═══════════════════════════════════════════════");
                println!("✅ Downloaded: {}", summary.downloaded.len());
                println!("⊘  Skipped: {}", summary.skipped.len());
                println!("❌ Failed: {}", summary.failed.len());
                println!(
                    "📦 Total size: {:.2} MB",
                    summary.total_bytes as f64 / 1_048_576.0
                );
                println!("⏱️  Duration: {:.1}s", summary.duration.as_secs_f64());
                if summary.duration.as_secs() > 0 {
                    println!(
                        "📈 Average speed: {:.2} MB/s",
                        (summary.total_bytes as f64 / 1_048_576.0) / summary.duration.as_secs_f64()
                    );
                }
                println!("═══════════════════════════════════════════════");
            }

            _ => {}
        }
    }

    let summary = session.wait().await?;

    // Write collection.db file
    if !summary.downloaded.is_empty() {
        println!();
        println!("Writing collection.db file...");
        collection.write_db(Path::new("./collection.db"))?;
        println!("✓ Collection database written to ./collection.db");
        println!();
        println!("To use this collection:");
        println!("1. Copy collection.db to your osu! folder");
        println!("2. Restart osu!");
        println!(
            "3. The collection '{}' will appear in your collections",
            collection.name
        );
    }

    Ok(())
}
