use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::sync::LazyLock;

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

// ── render_separator ──────────────────────────────────────────────────────────
//
// src/tui/widgets.rs:render_separator — called on every TUI render frame to
// draw a horizontal rule.  The current implementation allocates a fresh
// `String` via `"─".repeat(width)` on every call — ~30 allocs/sec at 30 fps.
//
// Baseline: `"─".repeat(n)` — 1 allocation per call, cost grows with width.
// Candidate: `String::with_capacity(n * 3)` + `extend(iter::repeat_n('─', n))`
//   or a lazily-built max-width buffer sliced to `n` — zero allocation on the
//   hot path.
//
// Bench inputs: representative terminal widths (80, 160, 220).

fn bench_render_separator(c: &mut Criterion) {
    let widths: &[usize] = &[80, 160, 220];
    let mut group = c.benchmark_group("render_separator");

    // Baseline: production pattern — fresh String allocation per call.
    for &w in widths {
        group.bench_with_input(BenchmarkId::new("repeat_alloc", w), &w, |b, &w| {
            b.iter(|| {
                let s: String = black_box("─").repeat(black_box(w));
                black_box(s);
            })
        });
    }

    // Candidate: build once with capacity, extend — still 1 alloc but
    // avoids the hidden realloc inside `repeat` when the multibyte char
    // is repeated into an undersized buffer.
    for &w in widths {
        group.bench_with_input(BenchmarkId::new("with_capacity_extend", w), &w, |b, &w| {
            b.iter(|| {
                let mut s = String::with_capacity(w * 3); // '─' is 3 bytes
                s.extend(std::iter::repeat_n('─', black_box(w)));
                black_box(s);
            })
        });
    }

    // Candidate: reuse a thread-local scratch buffer — zero alloc on reuse.
    for &w in widths {
        group.bench_with_input(BenchmarkId::new("reuse_scratch", w), &w, |b, &w| {
            let mut scratch = String::with_capacity(220 * 3);
            b.iter(|| {
                scratch.clear();
                scratch.extend(std::iter::repeat_n('─', black_box(w)));
                black_box(scratch.as_str());
            })
        });
    }

    // New shape: slice a pre-built LazyLock<String> — zero allocation on hot path.
    static H_LINE_BUF: LazyLock<String> = LazyLock::new(|| "─".repeat(256));
    for &w in widths {
        group.bench_with_input(BenchmarkId::new("static_slice", w), &w, |b, &w| {
            b.iter(|| {
                let end = black_box(w) * 3; // '─' is 3 bytes
                let s = &H_LINE_BUF.as_str()[..end];
                black_box(s);
            })
        });
    }

    group.finish();
}

// ── indeterminate_bar_spans ───────────────────────────────────────────────────
//
// src/tui/widgets.rs:indeterminate_bar_spans — called per active download slot
// per render frame for beatmapsets whose size is unknown.  Three `String`
// allocations per call: `"░".repeat(offset)` + `"█".repeat(segment)` +
// `"░".repeat(right)`.  With 4 active slots at 30 fps that is 360 allocs/sec.
//
// Baseline: 3× `char.repeat(n)` — up to 3 heap allocations per call.
// Candidate: 1× `String::with_capacity(width * 3)` filled in three passes —
//   single alloc regardless of segment count.  Strings are `'static`-lifetime
//   in `Span::styled`; the candidate measures the construction cost only.
//
// Bench inputs: (offset, segment, right) tuples matching BAR_WIDTH=16 travel.

fn bench_indeterminate_bar_spans(c: &mut Criterion) {
    // (left_empty, filled_segment, right_empty) — sum == bar_width
    let cases: &[(&str, usize, usize, usize)] = &[
        ("start", 0, 4, 12),
        ("mid", 6, 4, 6),
        ("end", 12, 4, 0),
        ("wide", 50, 4, 50), // wider terminal bar
    ];

    let mut group = c.benchmark_group("indeterminate_bar_spans");

    // Baseline: 3× repeat — current production code path.
    for &(label, left, seg, right) in cases {
        group.bench_with_input(
            BenchmarkId::new("three_repeat", label),
            &(left, seg, right),
            |b, &(left, seg, right)| {
                b.iter(|| {
                    let mut v: Vec<String> = Vec::with_capacity(3);
                    if black_box(left) > 0 {
                        v.push("░".repeat(black_box(left)));
                    }
                    v.push("█".repeat(black_box(seg)));
                    if black_box(right) > 0 {
                        v.push("░".repeat(black_box(right)));
                    }
                    black_box(v);
                })
            },
        );
    }

    // Candidate: single pre-sized String filled in three extend passes.
    for &(label, left, seg, right) in cases {
        group.bench_with_input(
            BenchmarkId::new("single_scratch", label),
            &(left, seg, right),
            |b, &(left, seg, right)| {
                b.iter(|| {
                    let total = black_box(left) + black_box(seg) + black_box(right);
                    let mut s = String::with_capacity(total * 3);
                    s.extend(std::iter::repeat_n('░', black_box(left)));
                    s.extend(std::iter::repeat_n('█', black_box(seg)));
                    s.extend(std::iter::repeat_n('░', black_box(right)));
                    black_box(s);
                })
            },
        );
    }

    // New shape: slice pre-built LazyLock<String> buffers — zero allocation.
    static SHADE_BUF: LazyLock<String> = LazyLock::new(|| "░".repeat(256));
    static BLOCK_BUF: LazyLock<String> = LazyLock::new(|| "█".repeat(256));
    for &(label, left, seg, right) in cases {
        group.bench_with_input(
            BenchmarkId::new("static_slice", label),
            &(left, seg, right),
            |b, &(left, seg, right)| {
                b.iter(|| {
                    let left_s = &SHADE_BUF.as_str()[..black_box(left) * 3];
                    let seg_s = &BLOCK_BUF.as_str()[..black_box(seg) * 3];
                    let right_s = &SHADE_BUF.as_str()[..black_box(right) * 3];
                    black_box((left_s, seg_s, right_s));
                })
            },
        );
    }

    group.finish();
}

// ── tab_titles ────────────────────────────────────────────────────────────────
//
// src/app/state.rs:tab_titles — called on every render frame to build the tab
// bar.  Current shape: `Vec<String>` + `"Home".to_string()` × 3 static tabs
// + `page.title.clone()` × N dynamic tabs.  At 30 fps with N=0 collections
// that is 90 static-string allocs/sec; with N=5 it is 240/sec.
//
// Baseline: current production pattern — Vec<String> with .to_string() for
//   static tabs and .clone() for dynamic tabs.
// Candidate A: return `Vec<&str>` / mixed-lifetime approach, static tabs as
//   `&'static str` — zero alloc for static portion.
// Candidate B: return an iterator of `Cow<'_, str>` — static tabs borrow,
//   dynamic tabs are `Owned`.
//
// Bench inputs: N ∈ {0, 3, 10} to show per-tab scaling.

fn bench_tab_titles(c: &mut Criterion) {
    let dynamic_counts: &[usize] = &[0, 3, 10];
    let mut group = c.benchmark_group("tab_titles");

    // Baseline: production pattern — Vec<String> + to_string() + clone().
    for &n in dynamic_counts {
        let pages: Vec<String> = (0..n).map(|i| format!("collection {i}")).collect();

        group.bench_with_input(
            BenchmarkId::new("vec_string_clone", n),
            &pages,
            |b, pages| {
                b.iter(|| {
                    let mut titles = Vec::with_capacity(pages.len() + 3);
                    titles.push(black_box("Home").to_string());
                    titles.push(black_box("Updates").to_string());
                    titles.push(black_box("Config").to_string());
                    for page in pages {
                        titles.push(black_box(page).clone());
                    }
                    black_box(titles)
                })
            },
        );
    }

    // Candidate: static tabs as &'static str, dynamic as &str borrows —
    //   zero allocation for the static portion; Vec itself is still allocated.
    for &n in dynamic_counts {
        let pages: Vec<String> = (0..n).map(|i| format!("collection {i}")).collect();

        group.bench_with_input(
            BenchmarkId::new("static_str_borrow", n),
            &pages,
            |b, pages| {
                b.iter(|| {
                    let mut titles: Vec<&str> = Vec::with_capacity(pages.len() + 3);
                    titles.push(black_box("Home"));
                    titles.push(black_box("Updates"));
                    titles.push(black_box("Config"));
                    for page in pages {
                        titles.push(black_box(page.as_str()));
                    }
                    black_box(titles)
                })
            },
        );
    }

    group.finish();
}

// ── capture_osz_snapshot filename allocation ──────────────────────────────────
//
// src/download/precheck.rs:capture_osz_snapshot — called twice per precheck
// (before + after validation) to snapshot `.osz` files in the output dir.
// Current shape:
//   file_name.to_string_lossy().into_owned().into_boxed_str()
// This String + Box allocation fires for every archive entry (N per call).
// On a 500-map collection: 1000 allocs across both snapshots per precheck run.
//
// Candidate: OsStr::to_str() — returns &str directly for valid UTF-8 (the
// common case on all supported platforms), box that without going through
// String; skip entries with non-UTF-8 names.
//
// Bench inputs: N ∈ {50, 200, 500} typical `.osz` filenames.

fn make_osz_filenames(n: usize) -> Vec<std::ffi::OsString> {
    (0..n)
        .map(|i| {
            std::ffi::OsString::from(format!("{i:07} Artist Name - Song Title [Difficulty].osz"))
        })
        .collect()
}

fn bench_snapshot_filename_alloc(c: &mut Criterion) {
    let sizes: &[usize] = &[50, 200, 500];
    let mut group = c.benchmark_group("snapshot_filename_alloc");

    for &n in sizes {
        let filenames = make_osz_filenames(n);

        // Baseline: to_string_lossy().into_owned() — String alloc per entry.
        group.bench_with_input(
            BenchmarkId::new("to_string_lossy_owned", n),
            &filenames,
            |b, filenames| {
                b.iter(|| {
                    let names: Vec<Box<str>> = black_box(filenames)
                        .iter()
                        .map(|f| f.to_string_lossy().into_owned().into_boxed_str())
                        .collect();
                    black_box(names)
                })
            },
        );

        // Candidate: to_str() + skip non-UTF-8 — borrows then boxes, no String.
        group.bench_with_input(
            BenchmarkId::new("to_str_skip", n),
            &filenames,
            |b, filenames| {
                b.iter(|| {
                    let names: Vec<Box<str>> = black_box(filenames)
                        .iter()
                        .filter_map(|f| f.to_str().map(Box::from))
                        .collect();
                    black_box(names)
                })
            },
        );
    }

    group.finish();
}

// ── emit_status_format ────────────────────────────────────────────────────────
//
// src/download/events.rs:emit_status — called once per BeatmapsetStatus library
// event with a mirror kind.  Current shape allocates a new String via format!()
// for each status variant:
//   format!("checking {}", mirror.label())
//   format!("{} from {}", status::DOWNLOADING, mirror.label())
//   format!("verifying from {}", mirror.label())
// mirror.label() returns &'static str; the prefix constants are also &'static
// str.  With 4 concurrent downloads × ~3 status events each per mirror pass
// (contacting + downloading + verifying) × 3 retries ≈ 36 allocs/sec minimum.
//
// Candidate: pre-concatenate the common per-mirror strings into static storage
// (a LazyLock<HashMap<MirrorKind, String>>) or use Cow<'static, str> for the
// common prefixes.  For the bench we measure the raw format! overhead vs
// a manual write_to approach (single allocation per call, no intermediate
// capture).
//
// Bench inputs: the 3 main status variants × 4 built-in mirror labels.

fn bench_emit_status_format(c: &mut Criterion) {
    // Mirror labels as returned by MirrorKind::label() — &'static str.
    let labels: &[(&str, &str)] = &[
        ("Nerinyan", "nerinyan"),
        ("osu.direct", "osu_direct"),
        ("Sayobot", "sayobot"),
        ("Nekoha", "nekoha"),
    ];

    const DOWNLOADING: &str = "downloading";

    let mut group = c.benchmark_group("emit_status_format");

    for &(label, key) in labels {
        // Baseline: current shape — one format! per variant.
        group.bench_with_input(
            BenchmarkId::new("format_contacting", key),
            label,
            |b, label| {
                b.iter(|| {
                    let s = format!("checking {}", black_box(label));
                    black_box(s)
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("format_downloading", key),
            label,
            |b, label| {
                b.iter(|| {
                    let s = format!("{} from {}", DOWNLOADING, black_box(label));
                    black_box(s)
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("format_verifying", key),
            label,
            |b, label| {
                b.iter(|| {
                    let s = format!("verifying from {}", black_box(label));
                    black_box(s)
                })
            },
        );

        // Candidate A: String::with_capacity + push_str — same alloc count but
        // avoids format machinery; measures raw String construction cost.
        group.bench_with_input(
            BenchmarkId::new("push_str_contacting", key),
            label,
            |b, label| {
                b.iter(|| {
                    let mut s = String::with_capacity(9 + label.len());
                    s.push_str("checking ");
                    s.push_str(black_box(label));
                    black_box(s)
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("push_str_downloading", key),
            label,
            |b, label| {
                b.iter(|| {
                    let mut s = String::with_capacity(DOWNLOADING.len() + 6 + label.len());
                    s.push_str(DOWNLOADING);
                    s.push_str(" from ");
                    s.push_str(black_box(label));
                    black_box(s)
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("push_str_verifying", key),
            label,
            |b, label| {
                b.iter(|| {
                    let mut s = String::with_capacity(14 + label.len());
                    s.push_str("verifying from ");
                    s.push_str(black_box(label));
                    black_box(s)
                })
            },
        );
    }

    group.finish();
}

// ── render_constraints_vec ────────────────────────────────────────────────────
//
// src/tui/download.rs:render — called on every TUI render frame per open
// download tab.  Current shape:
//   let mut constraints = Vec::with_capacity(4);
//   if show_disk_warning { constraints.push(Constraint::Length(1)); }
//   constraints.push(Constraint::Length(INFO_HEIGHT));
//   constraints.push(Constraint::Length(GAUGE_HEIGHT));
//   constraints.push(Constraint::Min(0));
//   Layout::vertical(constraints).split(area)
// Vec<Constraint> is heap-allocated every frame; layout is structurally fixed
// (3 or 4 elements).  At 30 fps with 3 open tabs: 90 allocs/sec.
//
// Candidate: use a fixed-size stack array and pass a slice, avoiding the heap
// allocation entirely.  Layout::vertical() accepts &[Constraint] via Into<Layout>.
//
// Bench inputs: both branches (show_disk_warning = true / false) to capture the
// common case (false) and the warnings case (true).

fn bench_render_constraints_vec(c: &mut Criterion) {
    use ratatui::layout::Constraint;

    const INFO_HEIGHT: u16 = 8;
    const GAUGE_HEIGHT: u16 = 3;

    let mut group = c.benchmark_group("render_constraints_vec");

    // Baseline: Vec::with_capacity + conditional push — production shape.
    for show_disk_warning in [false, true] {
        let label = if show_disk_warning {
            "with_warning"
        } else {
            "no_warning"
        };
        group.bench_with_input(
            BenchmarkId::new("vec_with_capacity", label),
            &show_disk_warning,
            |b, &show_disk_warning| {
                b.iter(|| {
                    let mut constraints = Vec::with_capacity(4);
                    if black_box(show_disk_warning) {
                        constraints.push(Constraint::Length(1));
                    }
                    constraints.push(Constraint::Length(INFO_HEIGHT));
                    constraints.push(Constraint::Length(GAUGE_HEIGHT));
                    constraints.push(Constraint::Min(0));
                    black_box(constraints)
                })
            },
        );

        // Candidate: two static arrays; branch selects the right slice — zero
        // heap alloc.  Arrays are the bench input so they live long enough.
        let with_warn: [Constraint; 4] = [
            Constraint::Length(1),
            Constraint::Length(INFO_HEIGHT),
            Constraint::Length(GAUGE_HEIGHT),
            Constraint::Min(0),
        ];
        let without_warn: [Constraint; 3] = [
            Constraint::Length(INFO_HEIGHT),
            Constraint::Length(GAUGE_HEIGHT),
            Constraint::Min(0),
        ];
        let arrays = (with_warn, without_warn, show_disk_warning);
        group.bench_with_input(
            BenchmarkId::new("fixed_array_slice", label),
            &arrays,
            |b, (with_warn, without_warn, show_disk_warning)| {
                b.iter(|| {
                    let slice: &[Constraint] = if black_box(*show_disk_warning) {
                        &with_warn[..]
                    } else {
                        &without_warn[..]
                    };
                    black_box(slice)
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
    bench_render_separator,
    bench_indeterminate_bar_spans,
    bench_tab_titles,
    bench_snapshot_filename_alloc,
    bench_emit_status_format,
    bench_render_constraints_vec,
);
criterion_main!(benches);
