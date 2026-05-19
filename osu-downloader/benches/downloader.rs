use std::sync::mpsc;

use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use md5::{Digest, Md5};
use osu_downloader::{Mirror, sanitize_filename};

fn bench_md5_hex_format(c: &mut Criterion) {
    // Represents the hot path in HashWorker::new (worker.rs:53-58):
    //   hasher.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>()
    // This runs once per completed download to produce the MD5 hex digest.
    let data = b"some representative beatmap archive bytes for hashing purposes 1234567890";

    c.bench_function("md5_hex_format", |b| {
        b.iter(|| {
            let mut hasher = Md5::new();
            hasher.update(black_box(data));
            let digest = hasher.finalize();
            let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
            black_box(hex)
        })
    });
}

fn bench_mirror_url_for(c: &mut Criterion) {
    // Represents mirror.url_for(beatmapset_id) (mirrors/mod.rs:216-218):
    //   self.template.replace("{id}", &beatmapset_id.to_string())
    // Called in the inner retry loop for every mirror attempt.
    let mirrors = [
        Mirror::nerinyan(),
        Mirror::osu_direct(),
        Mirror::sayobot(),
        Mirror::nekoha(),
    ];
    let ids: &[u32] = &[1, 123, 99999, 1_234_567, u32::MAX];

    let mut group = c.benchmark_group("mirror_url_for");
    for &id in ids {
        group.bench_with_input(BenchmarkId::new("nerinyan", id), &id, |b, &id| {
            b.iter(|| black_box(mirrors[0].template().replace("{id}", &id.to_string())))
        });
    }
    group.finish();
}

fn bench_sanitize_filename(c: &mut Criterion) {
    // Represents sanitize_filename (download.rs:86-92):
    //   name.chars().map(|c| match c { forbidden => '_', c => c }).collect::<String>()
    // Called once per download to clean the Content-Disposition filename.
    let cases: &[(&str, u32)] = &[
        // typical clean filename — no replacements needed
        ("1234567 Artist - Song Title [Difficulty].osz", 1_234_567),
        // filename with many forbidden chars (worst case replacement path)
        ("1234567 Art:ist - Song/Title [Diff*cult\\y].osz", 1_234_567),
        // short fallback-triggering None input
        ("", 999),
        // long filename (~200 chars)
        (
            "9999999 A Very Long Artist Name With Spaces - A Very Long Song Title \
             That Goes On And On Including Extra Details [Expert Difficulty].osz",
            9_999_999,
        ),
    ];

    let mut group = c.benchmark_group("sanitize_filename");
    for (name, id) in cases {
        let label = if name.is_empty() {
            "empty"
        } else if name.contains(':') || name.contains('/') || name.contains('*') {
            "with_forbidden"
        } else if name.len() > 100 {
            "long_clean"
        } else {
            "typical_clean"
        };
        group.bench_with_input(
            BenchmarkId::new(label, id),
            &(*name, *id),
            |b, &(name, id)| {
                b.iter(|| black_box(sanitize_filename(Some(black_box(name)), black_box(id))))
            },
        );
    }
    group.finish();
}

fn bench_hash_worker_update(c: &mut Criterion) {
    // Represents HashWorker::update (worker.rs):
    //   fn update(&self, data: Bytes) { sender.send(data); }
    // Called per ~128 KB network chunk during streaming download. Previously used
    // to_vec() (128 KB heap alloc per chunk); now sends a Bytes clone (Arc refcount
    // bump, ~2 ns) — the reqwest bytes_stream() already returns Bytes.
    const CHUNK_128K: usize = 128 * 1024;
    const CHUNK_4K: usize = 4 * 1024;

    let chunk_128k = Bytes::from(vec![0xABu8; CHUNK_128K]);
    let chunk_4k = Bytes::from(vec![0xABu8; CHUNK_4K]);

    let cases: &[(&str, &Bytes)] = &[("128k_chunk", &chunk_128k), ("4k_chunk", &chunk_4k)];

    let mut group = c.benchmark_group("hash_worker_update");
    for (label, chunk) in cases {
        group.throughput(Throughput::Bytes(chunk.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(label), chunk, |b, chunk| {
            // Replicate the exact production shape: unbounded channel, send a Bytes clone.
            let (sender, receiver) = mpsc::channel::<Bytes>();
            // Drain the receiver in a background thread so the channel never blocks.
            std::thread::spawn(move || while receiver.recv().is_ok() {});
            b.iter(|| {
                let _ = sender.send(black_box((*chunk).clone()));
            });
        });
    }
    group.finish();
}

fn bench_find_eocd_position(c: &mut Criterion) {
    // Represents find_eocd_position (validation.rs:176-180):
    //   buffer.windows(EOCD_SIGNATURE.len()).rposition(|w| w == EOCD_SIGNATURE)
    // Called once per archive during precheck (up to 65 536 bytes) to locate the
    // ZIP end-of-central-directory record. Hot when hundreds of archives are
    // validated in parallel.
    const EOCD_SIG: &[u8] = &[0x50, 0x4B, 0x05, 0x06];
    const BUF_SIZE: usize = 65_536;

    // Case 1: EOCD present at the very end (common happy path — minimal real archive).
    let mut buf_eocd_at_end = vec![0u8; BUF_SIZE];
    buf_eocd_at_end[BUF_SIZE - 22..BUF_SIZE - 22 + 4].copy_from_slice(EOCD_SIG);

    // Case 2: EOCD absent — full scan with no match (worst case for corrupted archives).
    let buf_no_eocd = vec![0u8; BUF_SIZE];

    // Case 3: EOCD near the middle (must find the last occurrence).
    let mut buf_eocd_mid = vec![0u8; BUF_SIZE];
    buf_eocd_mid[BUF_SIZE / 2..BUF_SIZE / 2 + 4].copy_from_slice(EOCD_SIG);

    // Inline the exact production pattern so the bench measures the real code shape.
    let find_eocd = |buffer: &[u8]| -> Option<usize> {
        buffer
            .windows(EOCD_SIG.len())
            .rposition(|window| window == EOCD_SIG)
    };

    let mut group = c.benchmark_group("find_eocd_position");
    group.throughput(Throughput::Bytes(BUF_SIZE as u64));

    group.bench_function("eocd_at_end", |b| {
        b.iter(|| black_box(find_eocd(black_box(&buf_eocd_at_end))))
    });
    group.bench_function("no_eocd", |b| {
        b.iter(|| black_box(find_eocd(black_box(&buf_no_eocd))))
    });
    group.bench_function("eocd_at_mid", |b| {
        b.iter(|| black_box(find_eocd(black_box(&buf_eocd_mid))))
    });

    group.finish();
}

// Inline of the exact production function (download.rs:146-169) — private, not pub.
fn split_content_disposition(header_value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in header_value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            ';' if !quoted => {
                parts.push(header_value[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(header_value[start..].trim());
    parts
}

fn bench_split_content_disposition(c: &mut Criterion) {
    // Represents split_content_disposition (download.rs:146-169):
    //   allocates Vec<&str> + state-machine char scan for every download response.
    // Called inside extract_filename_from_header on every successful mirror
    // response. Two real-world header shapes benched.

    // Common case: simple quoted filename.
    let simple = r#"attachment; filename="1234567 Artist - Song [Hard].osz""#;
    // Extended RFC 5987 UTF-8 encoded filename (less common but realistic).
    let extended = r#"attachment; filename="fallback.osz"; filename*=UTF-8''1234567%20Artist%20-%20Song%20%5BHard%5D.osz"#;

    let mut group = c.benchmark_group("split_content_disposition");
    group.bench_function("simple_filename", |b| {
        b.iter(|| black_box(split_content_disposition(black_box(simple))))
    });
    group.bench_function("extended_filename_star", |b| {
        b.iter(|| black_box(split_content_disposition(black_box(extended))))
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_md5_hex_format,
    bench_mirror_url_for,
    bench_sanitize_filename,
    bench_hash_worker_update,
    bench_find_eocd_position,
    bench_split_content_disposition,
);
criterion_main!(benches);
