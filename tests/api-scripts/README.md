# API test scripts

Shell scripts that exercise every HTTP endpoint the program calls, validating response shapes against the Rust models.

## dependencies

- `bash`
- `curl`
- `jq`
- `xxd`

## run

```bash
# run all tests
bash tests/api-scripts/run_all.sh

# run individual test
bash tests/api-scripts/test_osucollector.sh
```

## scripts

| script | endpoint | rust source | model validated |
|--------|----------|-------------|-----------------|
| `test_osucollector.sh` | `GET https://osucollector.com/api/collections/{id}` | `src/core/collection/api_client.rs` | `Collection { id, name, uploader: { id, username }, beatmapsets: [{ id, beatmaps: [{ id, checksum }] }] }` |
| `test_github_releases.sh` | `GET https://api.github.com/repos/uwuclxdy/osu-collect/releases/latest` | `src/auto_update.rs` | `ReleaseResponse { name, tag_name, assets: [{ name, browser_download_url }] }` |
| `test_nekoha_size.sh` | `GET https://mirror.nekoha.moe/api4/beatmapset/{id}` | `src/download/size_fetcher.rs` | `BeatmapsetResponse { file_size: Option<u64> }` (handles string or number) |
| `test_mirrors_download.sh` | all mirror download URLs | `osu-downloader/src/mirrors/mod.rs`, `src/config/constants.rs` | ZIP magic bytes (PK\x03\x04) in first 4 bytes |

## mirror endpoints covered

| mirror | download template | check URL (`MIRROR_CHECK_URLS`) |
|--------|-------------------|----------------------------------|
| nerinyan | `https://api.nerinyan.moe/d/{id}` | same |
| osu.direct | `https://osu.direct/d/{id}` | `https://osu.direct/api/d/{id}` |
| sayobot | `https://dl.sayobot.cn/beatmaps/download/full/{id}` | same |
| nekoha | `https://mirror.nekoha.moe/api4/download/{id}` | same |

## fixture data

- beatmapset: `705655` (THE ORAL CIGARETTES - https://osu.ppy.sh/beatmapsets/705655)
- collection: `50` (used for osucollector test; 705655 is a beatmapset ID, not a collection ID)

## notes

- mirrors that return `429` (rate limited) or `502/503/504` (server error) are skipped, not failed — these are transient upstream conditions
- sayobot intermittently returns 504; this is a known upstream issue
