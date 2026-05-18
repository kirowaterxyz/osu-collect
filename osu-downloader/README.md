# osu-downloader

A vibecoded Rust library for downloading osu! beatmapsets in bulk from multiple mirrors, with failover, rate-limit backoff, MD5 + ZIP validation, and a streaming event API. Build a `Downloader`, hand it a list of beatmapset IDs and an output directory, then consume `Event`s off the session until it finishes.

```toml
[dependencies]
osu-downloader = "0.6"
```

## Features

- Concurrent downloads across as many mirrors as you configure, with automatic failover when one returns 404, 429, or transient errors
- Per-mirror rate-limit backoff with a shared penalty pool — a throttled mirror sits out while the others keep working
- Real-time progress, status, and completion events over a `Stream`, plus a one-shot summary on `.wait()`
- Streaming downloads with MD5 hashing and ZIP/EOCD validation, written through a temp file and hard-linked into place
- Optional osucollector.com client + `collection.db` writer (`collection` feature)
- Optional Nekoha-backed size and availability probes (`size-fetch` feature)

## Quick start

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
    println!("{} of {} downloaded", summary.downloaded.len(), summary.total);
    Ok(())
}
```

`session.cancel()` aborts running downloads at the next checkpoint. `Mirror::nerinyan().no_video()` switches to the no-video template for mirrors that have one (no-op for custom mirrors).

## Mirrors

- **Nerinyan** — https://api.nerinyan.moe
- **osu.direct** — https://osu.direct
- **Sayobot** — https://dl.sayobot.cn
- **Nekoha** — https://mirror.nekoha.moe
- **Custom** — `Mirror::custom("https://your.mirror/d/{id}")?`

## Collections

```rust
use osu_downloader::{collection::CollectionClient, DownloadItem, Downloader};

let collection = CollectionClient::new().fetch(12345).await?;
let downloader = Downloader::builder().default_mirrors().build()?;
let mut session = downloader.download_many(
    collection.beatmapset_ids().into_iter().map(DownloadItem::skip_if_present),
    "./downloads",
);
// consume events, then:
collection.write_db("./downloads/collection.db".as_ref())?;
```

`CollectionClient::fetch` returns a typed `CollectionError` (`NotFound`, `RateLimited { retry_after }`, `Network`, `Status`, `Parse`, `InvalidUrl`) so callers can build their own retry policy. For multi-collection bundles use `collection::write_collections_db(&[CollectionDbEntry { … }], path)`.

## Feature flags

- `collection` (default) — `CollectionClient`, `Collection`, `write_collections_db`
- `size-fetch` (default) — `SizeFetcher` for beatmapset size estimates and mirror availability probes

## License

MIT — see [LICENSE](LICENSE).

## Acknowledgments

- Inspired by [BBD (beatmap batch downloader)](https://github.com/nzbasic/batch-beatmap-downloader)
- Written by Claude
