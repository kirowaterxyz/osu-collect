# osu-downloader

A lightweight, reusable and vibecoded Rust library for downloading osu! beatmaps from multiple mirrors.

## Features

- 🚀 **High Performance** - Concurrent downloads with configurable thread pool
- 🔄 **Mirror Fallback** - Automatic failover across multiple beatmap mirrors
- ⏸️ **Rate Limiting** - Smart rate limit handling with automatic backoff
- 📊 **Progress Tracking** - Real-time progress via channel-based events
- ✅ **Validation** - MD5 hash computation and ZIP archive validation
- 🎵 **Collection Support** - Download entire collections from osucollector.com (feature: `collection`)
- 📦 **Minimal Dependencies** - Only 9 core dependencies
- 🔌 **Feature Flags** - Optional functionality via cargo features

## Quick Start

```toml
[dependencies]
osu-downloader = "0.1"
```

### Download a Single Beatmapset

```rust
use osu_downloader::{Downloader, Mirror};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let downloader = Downloader::builder()
        .mirror(Mirror::nerinyan())
        .build()?;

    let result = downloader
        .download_one(123456, "./downloads")
        .await?;

    println!("Downloaded: {:?}", result);
    Ok(())
}
```

### Batch Downloads with Progress

```rust
use osu_downloader::{Downloader, Mirror, CatboyRegion, DownloadEvent};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let downloader = Downloader::builder()
        .mirror(Mirror::nerinyan())
        .mirror(Mirror::catboy(CatboyRegion::Us))
        .concurrent_downloads(8)
        .max_retries(3)
        .build()?;

    let ids = vec![123456, 789012, 345678];
    let mut session = downloader
        .download_many(ids, "./downloads")
        .await;

    while let Some(event) = session.next_event().await {
        match event {
            DownloadEvent::Progress { beatmapset_id, downloaded_bytes, total_bytes, .. } => {
                println!("#{}: {} / {:?} bytes", beatmapset_id, downloaded_bytes, total_bytes);
            }
            DownloadEvent::BeatmapsetCompleted { beatmapset_id, filename, .. } => {
                println!("✓ #{}: {}", beatmapset_id, filename);
            }
            DownloadEvent::BeatmapsetFailed { beatmapset_id, error, .. } => {
                eprintln!("✗ #{}: {}", beatmapset_id, error);
            }
            _ => {}
        }
    }

    let summary = session.wait().await?;
    println!("Downloaded {} / {} beatmapsets",
             summary.downloaded.len(),
             summary.total);
    Ok(())
}
```

## Supported Mirrors

- **Nerinyan** - https://api.nerinyan.moe
- **Catboy** - https://catboy.best (Central, US, Asia regions)
- **osu.direct** - https://osu.direct
- **Sayobot** - https://dl.sayobot.cn
- **Nekoha** - https://mirror.nekoha.moe
- **Custom** - Your own mirror with URL template

## Feature Flags

```toml
[dependencies]
osu-downloader = { version = "0.1", features = ["full"] }
```

- `collection` (default) - Collection API and `collection.db` writer
- `size-fetch` - Beatmapset size fetching from Nekoha API
- `full` - Enable all features

## Architecture

This library is designed to be:

- **Pure** - No file I/O except downloads, no config file reading
- **Explicit** - All configuration via builder pattern
- **Async-first** - Built on tokio for maximum performance
- **Channel-based** - Events via `tokio::sync::mpsc` for natural async integration
- **Reusable** - No TUI/app-specific dependencies

## Comparison with osu-collect

`osu-downloader` is the core library extracted from [osu-collect](https://github.com/uwuclxdy/osu-collect):

| Feature | osu-downloader | osu-collect |
|---|---|---|
| Download beatmaps | ✅ Core library | ✅ Uses library |
| Multiple mirrors | ✅ | ✅ |
| Progress tracking | ✅ Channels | ✅ TUI display |
| Collection support | ✅ (optional) | ✅ |
| Terminal UI | ❌ | ✅ TUI |
| Config files | ❌ | ✅ TOML |
| osu! database reading | ❌ | ✅ Stable + Lazer |
| Auto-updater | ❌ | ✅ |

## License

MIT License - see [LICENSE](LICENSE) for details

## Acknowledgments

- Inspired by [BBD (beatmap batch downloader)](https://github.com/nzbasic/batch-beatmap-downloader)
- Written by Claude

---

**Note**: This library is in active development. API may change before 1.0 release.
