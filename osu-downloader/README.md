# osu-downloader

A vibecoded Rust library for downloading osu! beatmaps from multiple mirrors.

## Features

- Concurrent downloads with configurable thread pool
- Automatic failover across multiple beatmap mirrors
- Rate limit handling with automatic backoff
- Real-time progress via channel-based events
- MD5 hash computation and ZIP archive validation
- Download entire collections from osucollector.com (feature: `collection`)
- 9 core dependencies
- Optional functionality via cargo features

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
use osu_downloader::{Downloader, Mirror, DownloadEvent};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let downloader = Downloader::builder()
        .mirror(Mirror::nerinyan())
        .mirror(Mirror::osu_direct())
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

- No file I/O except downloads, no config file reading
- All configuration via builder pattern
- Built on tokio, async throughout
- Events via `tokio::sync::mpsc`
- No TUI or app-specific dependencies

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

API may change before 1.0.
