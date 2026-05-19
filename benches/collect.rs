use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

// ── panel_block title formatting ─────────────────────────────────────────────
//
// src/tui/widgets.rs:panel_block — called on every TUI render frame.
// Three panels in the download view (overview, active, results) each call
// panel_block with a &'static str title.  At ~30 fps that is ≥90 calls/sec.
//
// Old shape (baseline, no longer in production):
//   format!(" {} ", title.to_uppercase())   — 2 allocations per call
//
// New shape: callers pass pre-uppercased, space-padded &'static str constants.
//   panel_block(" OVERVIEW ")               — zero allocations, pointer pass

fn bench_panel_block_title_format(c: &mut Criterion) {
    // Old pattern (baseline) — kept for before/after delta measurement.
    let lowercase_titles: &[&str] = &["overview", "active", "results", "config", "updates"];
    // New pattern — static constants as callers now supply them.
    let static_titles: &[&'static str] = &[
        " OVERVIEW ",
        " ACTIVE ",
        " RESULTS ",
        " CONFIG ",
        " UPDATES ",
    ];

    let mut group = c.benchmark_group("panel_block_title_format");

    // Baseline: old production pattern (2 allocations per call).
    for &title in lowercase_titles {
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

    // New shape: static constant pass-through — zero allocations, pointer return.
    for &title in static_titles {
        let key = title.trim();
        group.bench_with_input(
            BenchmarkId::new("static_constant", key),
            &title,
            |b, title| {
                b.iter(|| black_box(title));
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

        group.bench_with_input(BenchmarkId::new("merge_walk", n), &n, |b, _| {
            b.iter(|| {
                use std::collections::HashSet;
                let initial = black_box(&initial);
                let fin = black_box(&final_snap);
                let mut changes = HashSet::new();
                let mut i = 0;
                let mut f = 0;
                while i < initial.len() && f < fin.len() {
                    let a = &initial[i];
                    let b = &fin[f];
                    match a.name.cmp(&b.name) {
                        std::cmp::Ordering::Equal => {
                            if a.size != b.size || a.modified_micros != b.modified_micros {
                                changes.insert(a.beatmapset_id);
                            }
                            i += 1;
                            f += 1;
                        }
                        std::cmp::Ordering::Less => {
                            changes.insert(a.beatmapset_id);
                            i += 1;
                        }
                        std::cmp::Ordering::Greater => {
                            changes.insert(b.beatmapset_id);
                            f += 1;
                        }
                    }
                }
                for entry in &initial[i..] {
                    changes.insert(entry.beatmapset_id);
                }
                for entry in &fin[f..] {
                    changes.insert(entry.beatmapset_id);
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
// render frame (concurrent × ~30 fps). Old implementation:
//   let lower = message.to_lowercase();
//   if lower.contains("error") || lower.starts_with("failed") { ... }
// allocated on every call; the em-dash in "done — downloaded …" hit the UTF-8
// slow path, making that case 5× slower (123 ns vs ~22 ns).
//
// New shape: classify on BeatmapStage enum — zero string scan, zero alloc.

// Mirrors BeatmapStage without importing the binary crate.
#[derive(Clone, Copy)]
#[repr(u8)]
#[allow(dead_code)]
enum StageMock {
    Pending,
    Downloading,
    Verifying,
    Success,
    Skipped,
    Failed,
    Aborted,
}

fn bench_message_style_classify(c: &mut Criterion) {
    // Representative messages from active_download_item in production, paired
    // with the BeatmapStage the producer would have set at that point.
    let cases: &[(&str, &str, StageMock)] = &[
        (
            "downloading from nerinyan",
            "downloading",
            StageMock::Downloading,
        ),
        ("checking nerinyan", "checking", StageMock::Downloading),
        ("skipped: already exists", "skipped", StageMock::Skipped),
        (
            "done — downloaded from nerinyan",
            "done",
            StageMock::Success,
        ),
        ("failed: checksum mismatch", "failed", StageMock::Failed),
        (
            "network error: connection reset",
            "error",
            StageMock::Failed,
        ),
        (
            "verifying from osu!direct",
            "verifying",
            StageMock::Verifying,
        ),
        (
            "retrying nerinyan after connection reset (attempt 2/3)",
            "retrying",
            StageMock::Downloading,
        ),
    ];

    let mut group = c.benchmark_group("message_style_classify");

    for &(message, label, stage) in cases {
        // Baseline: old production pattern (to_lowercase allocates).
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

        // New pattern: stage enum match — zero alloc, zero string scan.
        group.bench_with_input(
            BenchmarkId::new("stage_match", label),
            &stage,
            |b, &stage| {
                b.iter(|| {
                    let result = match black_box(stage) {
                        StageMock::Failed | StageMock::Aborted => 1u8,
                        StageMock::Success => 2u8,
                        StageMock::Skipped => 3u8,
                        StageMock::Pending | StageMock::Downloading | StageMock::Verifying => 0u8,
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
