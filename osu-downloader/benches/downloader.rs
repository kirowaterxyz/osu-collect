use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use md5::{Digest, Md5};
use osu_downloader::{Mirror, MirrorKind, sanitize_filename};

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

criterion_group!(
    benches,
    bench_md5_hex_format,
    bench_mirror_url_for,
    bench_sanitize_filename
);
criterion_main!(benches);
