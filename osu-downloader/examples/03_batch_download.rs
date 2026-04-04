//! Example: Batch download with progress tracking
//!
//! Usage: cargo run --example 03_batch_download

use osu_downloader::{CatboyRegion, DownloadEvent, Downloader, Mirror};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create downloader with multiple mirrors
    let downloader = Downloader::builder()
        .mirror(Mirror::nerinyan())
        .mirror(Mirror::catboy(CatboyRegion::Central))
        .mirror(Mirror::osu_direct())
        .concurrent_downloads(4)
        .verify_archives(true)
        .build()?;

    println!("Starting batch download...");
    println!();

    // Example beatmapset IDs
    let beatmapset_ids = vec![
        41823,  // The Big Black
        774965, // Harumachi Clover
        320118, // Toumei Elegy
    ];

    let mut session = downloader
        .download_many(beatmapset_ids.clone(), "./downloads")
        .await;

    // Track progress
    let mut downloaded = 0;
    let mut skipped = 0;
    let mut failed = 0;

    // Process events
    while let Some(event) = session.next_event().await {
        match event {
            DownloadEvent::SessionStarted { total_beatmapsets } => {
                println!("📦 Starting download of {} beatmapsets", total_beatmapsets);
                println!();
            }

            DownloadEvent::BeatmapsetStarted {
                beatmapset_id,
                mirror,
            } => {
                println!("⬇️  Downloading #{} from {:?}", beatmapset_id, mirror);
            }

            DownloadEvent::Progress {
                beatmapset_id,
                downloaded_bytes,
                total_bytes,
                ..
            } => {
                if let Some(total) = total_bytes {
                    let progress = (downloaded_bytes as f64 / total as f64) * 100.0;
                    print!(
                        "\r   #{}: {:.1}% ({} / {} MB)    ",
                        beatmapset_id,
                        progress,
                        downloaded_bytes / 1_048_576,
                        total / 1_048_576
                    );
                } else {
                    print!(
                        "\r   #{}: {} MB downloaded    ",
                        beatmapset_id,
                        downloaded_bytes / 1_048_576
                    );
                }
            }

            DownloadEvent::BeatmapsetCompleted {
                beatmapset_id,
                filename,
                size_bytes,
                ..
            } => {
                downloaded += 1;
                println!();
                println!(
                    "✅ Completed #{}: {} ({} MB)",
                    beatmapset_id,
                    filename,
                    size_bytes / 1_048_576
                );
            }

            DownloadEvent::BeatmapsetFailed {
                beatmapset_id,
                error,
                ..
            } => {
                failed += 1;
                println!();
                println!("❌ Failed #{}: {}", beatmapset_id, error);
            }

            DownloadEvent::BeatmapsetSkipped {
                beatmapset_id,
                reason,
            } => {
                skipped += 1;
                println!();
                println!("⊘  Skipped #{}: {:?}", beatmapset_id, reason);
            }

            DownloadEvent::SessionCompleted { summary } => {
                println!();
                println!("🎉 Batch download complete!");
                println!("   Downloaded: {}", summary.downloaded.len());
                println!("   Skipped: {}", summary.skipped.len());
                println!("   Failed: {}", summary.failed.len());
                println!("   Total size: {} MB", summary.total_bytes / 1_048_576);
                println!("   Duration: {:.1}s", summary.duration.as_secs_f64());
                println!("   Success rate: {:.1}%", summary.success_rate() * 100.0);
            }
        }
    }

    // Wait for completion
    let summary = session.wait().await?;

    println!();
    println!("Final statistics:");
    println!("  Total: {}", summary.total);
    println!("  Downloaded: {}", downloaded);
    println!("  Skipped: {}", skipped);
    println!("  Failed: {}", failed);

    Ok(())
}
