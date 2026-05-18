# osu-downloader

A vibecoded Rust library for downloading osu! beatmapsets in bulk from multiple mirrors, with failover, rate-limit backoff, MD5 + ZIP validation, and a streaming event API. Build a `Downloader`, hand it beatmapset IDs and an output directory, then consume `Event`s off the session until it finishes.

```toml
[dependencies]
osu-downloader = "0.7"
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
use osu_downloader::{Downloader, Event, Mirror};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let downloader = Downloader::builder()
        .mirror(Mirror::nerinyan())
        .mirror(Mirror::osu_direct())
        .concurrent_downloads(8)
        .network_retry_attempts(2)
        .build()?;

    let mut session = downloader.download_many([123456u32, 789012, 345678], "./downloads");
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

`download_many` accepts anything iterable over `u32`. `session.cancel()` aborts running downloads at the next checkpoint. `.default_mirrors()` adds every built-in in one call. `.on_existing(FileExistsPolicy::Overwrite)` controls what happens when a beatmapset's archive is already on disk (default: skip).

## Mirrors

- **Nerinyan** — https://api.nerinyan.moe
- **osu.direct** — https://osu.direct
- **Sayobot** — https://dl.sayobot.cn
- **Nekoha** — https://mirror.nekoha.moe
- **Custom** — `Mirror::custom("https://your.mirror/d/{id}")?`

`Mirror::all_builtins()` returns every built-in. `Mirror::from_kind(MirrorKind::Sayobot)` constructs a single built-in by tag (returns `None` for `Custom`). `Mirror::nerinyan().no_video()` switches to the no-video template for mirrors that have one (no-op for custom mirrors).

## Collections

```rust
use osu_downloader::{collection::CollectionClient, Downloader};

let collection = CollectionClient::new().fetch_with_retries(12345, 3).await?;
let downloader = Downloader::builder().default_mirrors().build()?;
let mut session = downloader.download_many(collection.beatmapset_ids(), "./downloads");
// consume events, then:
collection.write_db("./downloads/collection.db".as_ref())?;
```

- `CollectionClient::fetch(id)` does a single request and surfaces errors verbatim.
- `CollectionClient::fetch_with_retries(id, attempts)` adds the library's built-in retry policy (rate-limit-aware sleeps + exponential backoff for transient network errors).
- `CollectionClient::fetch_input(input)` accepts either a numeric ID or a `https://osucollector.com/collections/<id>` URL.
- For multi-collection bundles use `collection::write_collections_db(&[CollectionDbEntry { … }], path)`.

## Errors

Everything funnels into one [`Error`](https://docs.rs/osu-downloader/latest/osu_downloader/enum.Error.html) enum so callers can match exhaustively without juggling layered error types:

```rust
match err {
    osu_downloader::Error::NotFound => …,
    osu_downloader::Error::RateLimited { retry_after } => …,
    osu_downloader::Error::Network(msg) => …,
    osu_downloader::Error::Timeout => …,
    osu_downloader::Error::Validation(msg) => …,
    _ => …,
}
```

`Error::is_transient()` is the shortcut for "should I retry?".

## Validation

```rust
use osu_downloader::{validate_archive, validate_and_remove, ArchiveValidation};

match validate_archive(path, ArchiveValidation::Eocd).await? {
    ArchiveValidationResult::Valid => …,
    ArchiveValidationResult::NotFound => …,
    ArchiveValidationResult::Invalid(reason) => …,
}

// or, to delete the file on validation failure:
validate_and_remove(path, ArchiveValidation::Eocd).await?;
```

## Output directory scanning

When walking the directory the downloader writes into, `classify_output_entry(name)` tells you which entries belong to the library:

```rust
use osu_downloader::{classify_output_entry, OutputEntry};

match classify_output_entry(&entry.file_name()) {
    OutputEntry::Archive { beatmapset_id } => …,
    OutputEntry::OrphanTemp => /* leftover from a cancelled download — safe to delete */,
    OutputEntry::Other => /* foreign file */,
}
```

## Feature flags

- `collection` (default) — `CollectionClient`, `Collection`, `write_collections_db`
- `size-fetch` (default) — `SizeFetcher` for beatmapset size estimates and mirror availability probes

## License

MIT — see [LICENSE](LICENSE).

## Acknowledgments

- Inspired by [BBD (beatmap batch downloader)](https://github.com/nzbasic/batch-beatmap-downloader)
- Written by Claude
