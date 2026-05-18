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
osu-downloader = "0.6"
```

### Batch Downloads with Progress

```rust
use osu_downloader::{Downloader, DownloadItem, Event, Mirror};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let downloader = Downloader::builder()
        .mirror(Mirror::nerinyan())
        .mirror(Mirror::osu_direct())
        .concurrent_downloads(8)
        .network_retry_attempts(2)
        .build()?;

    let items = [123456u32, 789012, 345678].map(DownloadItem::skip_if_present);
    let mut session = downloader.download_many(items, "./downloads");
    let mut events = session.events();

    while let Some(event) = events.next().await {
        match event {
            Event::Progress { beatmapset_id, downloaded_bytes, total_bytes, .. } => {
                println!("#{beatmapset_id}: {downloaded_bytes} / {total_bytes:?} bytes");
            }
            Event::BeatmapsetCompleted { beatmapset_id, filename, .. } => {
                println!("ok  #{beatmapset_id}: {filename}");
            }
            Event::BeatmapsetFailed { beatmapset_id, error, .. } => {
                eprintln!("err #{beatmapset_id}: {error}");
            }
            _ => {}
        }
    }

    let summary = session.wait().await?;
    println!(
        "downloaded {} / {} beatmapsets",
        summary.downloaded.len(),
        summary.total
    );
    Ok(())
}
```

### Strip Video Per Mirror

```rust
let downloader = Downloader::builder()
    .mirror(Mirror::nerinyan().no_video())
    .mirror(Mirror::sayobot().no_video())
    .build()?;
```

`no_video()` is a no-op for mirrors that don't have a no-video variant (including custom mirrors).

### Download an osucollector Collection

```rust
use osu_downloader::{collection::CollectionClient, Downloader, DownloadItem, Mirror};

let client = CollectionClient::new();
let collection = client.fetch(12345).await?;

let downloader = Downloader::builder().default_mirrors().build()?;
let items = collection
    .beatmapset_ids()
    .into_iter()
    .map(DownloadItem::skip_if_present);
let mut session = downloader.download_many(items, "./downloads");
// consume events as above
let _summary = session.wait().await?;

// persist the collection metadata to osu!'s collection.db format
collection.write_db("./downloads/collection.db".as_ref())?;
```

For multi-collection bundles, use [`collection::write_collections_db`] with an explicit `[CollectionDbEntry]` list.

`CollectionClient::fetch` returns a typed `CollectionError` so callers can recognise `NotFound`, `RateLimited { retry_after }`, transport errors, etc. and retry on their own terms.

### Cancellation

```rust
let mut session = downloader.download_many(ids, "./downloads");
let events = session.events();

// from anywhere with a handle:
session.cancel(); // running attempts abort at the next checkpoint
```

## Supported Mirrors

- **Nerinyan** - https://api.nerinyan.moe
- **osu.direct** - https://osu.direct
- **Sayobot** - https://dl.sayobot.cn
- **Nekoha** - https://mirror.nekoha.moe
- **Custom** - your own mirror with a URL template containing `{id}`

## Feature Flags

```toml
[dependencies]
osu-downloader = { version = "0.6", features = ["collection", "size-fetch"] }
```

- `collection` (default) - osucollector.com client and `collection.db` writer
- `size-fetch` (default) - Nekoha-backed beatmapset size and availability probes

## Public API At a Glance

Top level:

- `Downloader`, `DownloaderBuilder`, `DownloadItem`, `DownloadSession`, `FileExistsPolicy`
- `Event`, `StatusEvent`, `Summary`, `SkipReason`
- `Mirror`, `MirrorKind`
- `ArchiveValidation`, `ArchiveValidationOptions`, `ArchiveValidationResult`, `validate_archive`
- `Error`, `DownloadError`, `Result`

Optional modules:

- `osu_downloader::collection` (feature `collection`) — `CollectionClient`, `Collection`, `CollectionDbEntry`, `CollectionError`, `write_collections_db`
- `osu_downloader::size` (feature `size-fetch`) — `SizeFetcher`, `SizeFetchResult`, `MirrorAvailabilityResult`

## Architecture

- No file I/O except downloads, no config file reading
- All configuration via the builder pattern
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
