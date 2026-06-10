![osu!collect banner](media/osu-collect.png)

# osu!collect — free osu!collector collection downloader

[![Release](https://github.com/uwuclxdy/osu-collect/actions/workflows/release.yml/badge.svg)](https://github.com/uwuclxdy/osu-collect/actions/workflows/release.yml)
[![Latest release](https://img.shields.io/github/v/release/uwuclxdy/osu-collect)](https://github.com/uwuclxdy/osu-collect/releases/latest)

A terminal app (TUI) to **download osu! beatmap collections from [osu!collector](https://osucollector.com) for free** — batch downloads across multiple mirrors, `collection.db` generation for one-click import, and a collection updater that fetches only the maps you're missing. Windows, Linux and macOS :3

![usage example](media/image.png)
![usage example](media/image1.png)

## Features

- **Batch beatmap downloads** from osu!collector collections — paste a URL or ID, press enter
- **Multiple mirrors with failover** — Nerinyan, osu.direct, Sayobot, Nekoha, your own custom mirror, plus the official osu! servers after logging in with your osu! account
- **Rate-limit aware** — throttled mirrors sit out while the rest keep downloading; per-map cooldown countdowns in the UI
- **Collection updater** — re-check a collection later and download only missing or newly added maps
- **`collection.db` writer** — downloaded maps arrive as a proper osu! collection, not a loose folder
- **Integrity verification** — MD5 + archive validation on every download; existing files are verified and skipped
- **Retry failed maps** — failures persist between runs; retry them with one key or on the next download
- **Parallel downloads** in per-collection tabs — queue several collections at once
- **Self-updating** — checks GitHub releases on start and updates itself in place
- **Theming** — truecolor and 256/16-color palettes, auto-detected
- Disk-space warnings, download speed + ETA, config file with sane defaults

## Installation

Grab a binary from the [releases page](https://github.com/uwuclxdy/osu-collect/releases/latest), or on Linux x64 / macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/uwuclxdy/osu-collect/main/install.sh | bash
```

Or build from source (Rust 1.85+):

```bash
git clone https://github.com/uwuclxdy/osu-collect
cd osu-collect
cargo install --path .
```

> [!NOTE]
> This is a terminal program — run `osu-collect` in a terminal. On Windows, Windows Terminal or PowerShell 7+ are recommended.

## Usage

Run `osu-collect`, paste a collection link, pick a directory, press `enter`.

- **Collection URL or ID** — accepts `https://osucollector.com/collections/{id}` links or a bare ID; resolves automatically as you type and remembers your recent collections. *Required*
- **Download directory** — defaults to the last used directory; `tab` completes filesystem paths.
- **Threads** — parallel downloads; defaults to your CPU core count (20 or less avoids rate limiting).
- **Custom mirror URL** — must include the `{id}` placeholder; combines with the built-in mirror toggles.
- **Skip existing** — verifies and skips maps already on disk.
- **Auto-overwrite** — forces a redownload of every map.
- **No video** — downloads without video where the mirror supports it.

### Controls

| Key | Action |
|---|---|
| `↑` `↓` | move between rows |
| `←` `→` | switch tabs |
| `↵` | activate / toggle / start download / edit a field |
| `space` | toggle the focused checkbox or switch |
| `tab` | path-complete the directory field |
| `+` `-` | adjust thread count |
| `r` | retry all failed maps on a download tab |
| `x` | dismiss an error message |
| `?` | help overlay with every key |
| `q` | back / quit (press twice to confirm; aborts a running download the same way) |
| `ctrl+c` | quit immediately from anywhere |

Text fields support full caret editing: `home`/`end`, `delete`, `ctrl+w` to delete the previous word.

### Download tabs

Every queued collection gets its own tab with live per-map progress, speed and ETA, rate-limit countdowns, and a failure summary with reasons. Failed maps persist — retry them with `r` or get prompted on your next download of that collection (configurable).

### Updates tab

Tracks every collection you've downloaded. Re-checks them against osu!collector, shows what's missing or was removed, and lets you select exactly which maps to fetch — so keeping a collection current doesn't mean redownloading it.

### Logging in with your osu! account (optional)

The config tab has a login chip that opens the osu! OAuth flow in your browser. Once logged in, the official osu! servers become an additional download source. Credentials are stored locally in `auth.json`.

## Importing into osu!

### osu! lazer

1. Import all downloaded maps into lazer
2. Click `Run first time setup` and `Next` until the **Import screen**
3. Set `previous osu! install` to the **directory of the collection** you downloaded
4. Click `Import content from previous version`
5. Done — the maps and the collection are imported

### osu! stable

Drag the downloaded `.osz` files into osu!, then merge the generated `collection.db` with a tool like [Collection Manager](https://github.com/Piotrekol/CollectionManager) (or back up your own `collection.db` and swap it in if you have no existing collections).

## Configuration

Optional config file with defaults for mirrors, threads, archive validation, retry policy, theme and logging:

- **Linux/macOS**: `~/.config/osu-collect/config.toml`
- **Windows**: `%APPDATA%\osu-collect\config.toml`

Every key is documented in [config.toml.example](config.toml.example). Most settings are also editable live on the config tab — changes apply and save immediately.

## Alternatives

| Tool | Difference |
|---|---|
| [osu!Collector desktop client](https://osucollector.com/app) | the official app; bulk download requires a paid subscription — osu!collect is free |
| [BatchBeatmapDownloader](https://github.com/nzbasic/batch-beatmap-downloader) | downloads by filters/criteria rather than osu!collector collections; the original inspiration for this project |
| [osu-collector-dl](https://github.com/roogue/osu-collector-dl) | CLI script; no TUI, no collection.db generation, no updater |
| [OsuCollectionDownloader](https://github.com/waylaa/OsuCollectionDownloader) | .osdb generator; requires the .NET runtime |
| [Collection Manager](https://github.com/Piotrekol/CollectionManager) | manages/merges existing collections; pairs well with osu!collect for stable imports |

## FAQ

**How do I download an osu!collector collection for free?**
Run osu!collect, paste the collection URL, press `↵`. Downloads come from public beatmap mirrors (or the official servers if you log in) — no subscription needed.

**Does it work with osu! lazer?**
Yes — see [Importing into osu!](#importing-into-osu). The generated `collection.db` imports through lazer's first-time-setup flow.

**Do I need an osu! account?**
No. Logging in is optional and only adds the official osu! servers as an extra download source.

**Can it update a collection I downloaded earlier?**
Yes — the updates tab diffs your downloaded collections against osu!collector and fetches only what's missing.

**A download failed / got rate limited — what now?**
Failures are saved per collection. Press `r` on the download tab to retry them all, or accept the retry prompt next time you download that collection. Rate-limited mirrors cool down automatically while others continue.

## Building from source

```bash
cargo build --release
```

Requires Rust 1.85+ (edition 2024). For Windows cross-builds, `build.sh` produces Linux + Windows binaries in `build/`.

## Roadmap

- [ ] action menu (`a`) with batch operations
- [ ] toast notifications + scrollbars (cloudy-tui conformance round 2)
- [ ] full BatchBeatmapDownloader-style filter downloads

## Acknowledgments

Powered by [osu-downloader](osu-downloader/) (the bundled Rust library — mirrors, failover, validation, events), [osu-db](https://crates.io/crates/osu-db) and [ratatui](https://ratatui.rs). Inspired by [BatchBeatmapDownloader](https://github.com/nzbasic/batch-beatmap-downloader).
