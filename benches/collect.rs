use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

// ── panel_block title formatting ─────────────────────────────────────────────
//
// src/tui/widgets.rs:panel_block — called on every TUI render frame.
// Three panels in the download view (overview, active, results) each call
// panel_block with a &'static str title. At ~30 fps that is ≥90 calls/sec.
// The current implementation does:
//   format!(" {} ", title.to_uppercase())
// which allocates a String for `to_uppercase()` and a second String for the
// format!. Because title is &'static str the uppercased result is identical on
// every call — it can be precomputed or written via write! into a fixed buffer.

fn bench_panel_block_title_format(c: &mut Criterion) {
    let titles: &[&str] = &["overview", "active", "results", "config", "updates"];

    let mut group = c.benchmark_group("panel_block_title_format");

    // Baseline: exact current production pattern.
    for &title in titles {
        group.bench_with_input(
            BenchmarkId::new("format_to_uppercase", title),
            title,
            |b, title| {
                b.iter(|| {
                    let s: String = format!(" {} ", black_box(title).to_uppercase());
                    black_box(s)
                })
            },
        );
    }

    group.finish();
}

// ── detect_changed_beatmapsets ────────────────────────────────────────────────
//
// src/download/precheck.rs:detect_changed_beatmapsets — called at the end of
// every precheck pass to detect files mutated while validation was running.
// Current implementation builds two HashMap<&str, &Entry> from already-sorted
// Vec<Entry>, then cross-references them: two heap allocations of N entries
// each for N = number of archives in the output dir (easily hundreds).
// Because both slices are already sorted (snapshot.sort() before return),
// a merge-walk or binary_search on sorted slices needs zero heap allocation.

// Mirror the production struct — private, so we inline it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OszSnapshotEntry {
    name: Box<str>,
    beatmapset_id: u32,
    size: u64,
    modified_micros: Option<u128>,
}

fn make_snapshot(n: usize, mutate_fraction: usize) -> Vec<OszSnapshotEntry> {
    let mut v: Vec<OszSnapshotEntry> = (0..n)
        .map(|i| OszSnapshotEntry {
            name: format!("{i:07} Artist - Title [Diff].osz").into_boxed_str(),
            beatmapset_id: i as u32,
            size: 1_000_000 + i as u64 * 17,
            modified_micros: Some(1_700_000_000_000_000 + i as u128 * 1000),
        })
        .collect();
    v.sort();
    // Mutate a fraction to simulate real mid-precheck changes.
    for i in (0..n).step_by(mutate_fraction.max(1) + 1).take(n / 4) {
        v[i].size += 1;
    }
    v
}

fn bench_detect_changed_beatmapsets(c: &mut Criterion) {
    let sizes: &[usize] = &[50, 200, 500];

    let mut group = c.benchmark_group("detect_changed_beatmapsets");

    for &n in sizes {
        let initial = make_snapshot(n, 0);
        // Final snapshot: same entries but ~10% have a mutated size.
        let mut final_snap = initial.clone();
        for entry in final_snap.iter_mut().step_by(10) {
            entry.size += 1;
        }
        final_snap.sort();

        group.bench_with_input(BenchmarkId::new("hashmap_cross_ref", n), &n, |b, _| {
            b.iter(|| {
                // Inline the exact current production implementation.
                use std::collections::{HashMap, HashSet};
                let initial_map: HashMap<&str, &OszSnapshotEntry> = black_box(&initial)
                    .iter()
                    .map(|e| (e.name.as_ref(), e))
                    .collect();
                let final_map: HashMap<&str, &OszSnapshotEntry> = black_box(&final_snap)
                    .iter()
                    .map(|e| (e.name.as_ref(), e))
                    .collect();

                let mut changes = HashSet::new();
                for (name, previous) in &initial_map {
                    match final_map.get(name) {
                        Some(current) => {
                            if previous.size != current.size
                                || previous.modified_micros != current.modified_micros
                            {
                                changes.insert(previous.beatmapset_id);
                            }
                        }
                        None => {
                            changes.insert(previous.beatmapset_id);
                        }
                    }
                }
                for (name, current) in &final_map {
                    if !initial_map.contains_key(name) {
                        changes.insert(current.beatmapset_id);
                    }
                }
                black_box(changes)
            })
        });
    }

    group.finish();
}

// ── message_style to_lowercase ────────────────────────────────────────────────
//
// src/tui/widgets.rs:message_style — called once per active download slot per
// render frame (concurrent × ~30 fps). Current implementation:
//   let lower = message.to_lowercase();
//   if lower.contains("error") || lower.starts_with("failed") { ... }
// This allocates a new String on every call. Since all matched prefixes/patterns
// are pure-ASCII, `eq_ignore_ascii_case`, `.to_ascii_lowercase()` on a byte,
// or a manual byte prefix check are zero-alloc alternatives.

fn bench_message_style_classify(c: &mut Criterion) {
    // Representative messages from active_download_item in production.
    let messages: &[(&str, &str)] = &[
        ("downloading from nerinyan", "downloading"),
        ("checking nerinyan", "checking"),
        ("skipped: already exists", "skipped"),
        ("done — downloaded from nerinyan", "done"),
        ("failed: checksum mismatch", "failed"),
        ("network error: connection reset", "error"),
        ("verifying from osu!direct", "verifying"),
        (
            "retrying nerinyan after connection reset (attempt 2/3)",
            "retrying",
        ),
    ];

    let mut group = c.benchmark_group("message_style_classify");

    for &(message, label) in messages {
        // Baseline: exact current production pattern (to_lowercase allocates).
        group.bench_with_input(
            BenchmarkId::new("to_lowercase", label),
            message,
            |b, message| {
                b.iter(|| {
                    let lower = black_box(message).to_lowercase();
                    let result = if lower.contains("error") || lower.starts_with("failed") {
                        1u8
                    } else if lower.starts_with("done") {
                        2u8
                    } else if lower.starts_with("skipped") {
                        3u8
                    } else {
                        0u8
                    };
                    black_box(result)
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_panel_block_title_format,
    bench_detect_changed_beatmapsets,
    bench_message_style_classify,
);
criterion_main!(benches);
