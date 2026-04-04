//! Example: Advanced progress tracking with statistics
//!
//! Usage: cargo run --example 04_progress_tracking

use osu_downloader::{DownloadEvent, Downloader};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let downloader = Downloader::builder()
        .default_mirrors()
        .concurrent_downloads(6)
        .build()?;

    let beatmapset_ids = vec![41823, 774965, 320118, 658127, 123593];

    println!("Downloading {} beatmapsets...", beatmapset_ids.len());
    println!();

    let start = Instant::now();
    let mut session = downloader
        .download_many(beatmapset_ids, "./downloads")
        .await;

    let mut total_bytes = 0u64;
    let mut active_downloads: std::collections::HashMap<u32, (u64, u64)> =
        std::collections::HashMap::new();

    while let Some(event) = session.next_event().await {
        match event {
            DownloadEvent::BeatmapsetStarted { beatmapset_id, .. } => {
                active_downloads.insert(beatmapset_id, (0, 0));
            }

            DownloadEvent::Progress {
                beatmapset_id,
                downloaded_bytes,
                total_bytes: size,
                ..
            } => {
                if let Some(entry) = active_downloads.get_mut(&beatmapset_id) {
                    *entry = (downloaded_bytes, size.unwrap_or(0));
                }

                // Display overall progress
                let total_downloaded: u64 = active_downloads.values().map(|(d, _)| d).sum();
                let elapsed = start.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    (total_downloaded as f64 / elapsed) / 1_048_576.0
                } else {
                    0.0
                };

                print!(
                    "\r📊 Overall: {} MB | {:.2} MB/s | {} active    ",
                    total_downloaded / 1_048_576,
                    speed,
                    active_downloads.len()
                );
            }

            DownloadEvent::BeatmapsetCompleted {
                beatmapset_id,
                size_bytes,
                filename,
                ..
            } => {
                active_downloads.remove(&beatmapset_id);
                total_bytes += size_bytes;
                println!();
                println!("✓ {}", filename);
            }

            DownloadEvent::BeatmapsetFailed { beatmapset_id, .. } => {
                active_downloads.remove(&beatmapset_id);
            }

            DownloadEvent::BeatmapsetSkipped { beatmapset_id, .. } => {
                active_downloads.remove(&beatmapset_id);
            }

            DownloadEvent::SessionCompleted { summary } => {
                println!();
                println!();
                println!("═══════════════════════════════════════");
                println!("           Download Summary            ");
                println!("═══════════════════════════════════════");
                println!("Total: {} beatmapsets", summary.total);
                println!("✅ Downloaded: {}", summary.downloaded.len());
                println!("⊘  Skipped: {}", summary.skipped.len());
                println!("❌ Failed: {}", summary.failed.len());
                println!("📦 Total size: {:.2} MB", total_bytes as f64 / 1_048_576.0);
                println!("⏱️  Duration: {:.1}s", summary.duration.as_secs_f64());
                println!(
                    "📈 Average speed: {:.2} MB/s",
                    (total_bytes as f64 / 1_048_576.0) / summary.duration.as_secs_f64()
                );
                println!("✨ Success rate: {:.1}%", summary.success_rate() * 100.0);
                println!("═══════════════════════════════════════");
            }

            _ => {}
        }
    }

    let _ = session.wait().await?;
    Ok(())
}
