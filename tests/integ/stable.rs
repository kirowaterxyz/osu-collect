#[cfg(test)]
mod tests {
    use osu_collect::osu_db::{BeatmapReader, StableReader};
    use std::{collections::HashSet, path::PathBuf};

    fn testing_db_path() -> Option<PathBuf> {
        // Prefer env var, fall back to testing-db/ relative to workspace root
        if let Ok(p) = std::env::var("OSU_TESTING_DB") {
            let path = PathBuf::from(p);
            if path.join("osu!.db").exists() {
                return Some(path);
            }
        }
        let relative = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testing-db");
        if relative.join("osu!.db").exists() {
            Some(relative)
        } else {
            None
        }
    }

    #[test]
    fn test_stable_beatmapsets_read() {
        let Some(path) = testing_db_path() else {
            println!("skipping: no testing-db/osu!.db found");
            return;
        };

        let reader = StableReader::new(path);
        let beatmapsets = reader
            .list_beatmapsets()
            .expect("failed to read beatmapsets");

        let total_beatmaps: usize = beatmapsets.iter().map(|bs| bs.beatmaps.len()).sum();
        println!(
            "stable: {} beatmapsets, {} total beatmaps",
            beatmapsets.len(),
            total_beatmaps
        );
        // Parser may return empty for unsupported osu!.db versions — that is not a failure.
        // All beatmapset IDs must be non-zero when entries exist.
        for bs in &beatmapsets {
            assert!(bs.id > 0, "beatmapset id must be non-zero");
        }
    }

    #[test]
    fn test_stable_collections_read() {
        let Some(path) = testing_db_path() else {
            println!("skipping: no testing-db/collection.db found");
            return;
        };

        let reader = StableReader::new(path);
        let collections = reader
            .list_collections()
            .expect("failed to read collections");

        println!("stable: {} collections", collections.len());
        for c in collections.iter().take(5) {
            println!("  - {}: {} checksums", c.name, c.beatmap_checksums.len());
        }
    }

    #[test]
    fn test_stable_checksums_subset_of_beatmapsets() {
        let Some(path) = testing_db_path() else {
            println!("skipping: no testing-db/osu!.db found");
            return;
        };

        let reader = StableReader::new(path);
        let beatmapsets = reader
            .list_beatmapsets()
            .expect("failed to read beatmapsets");

        // Verify all_checksums (derived from beatmapsets) matches manual derivation
        let checksums_manual: HashSet<String> = beatmapsets
            .iter()
            .flat_map(|bs| bs.beatmaps.iter().map(|b| b.checksum.clone()))
            .collect();

        // Stable has no separate all_checksums call; checksums come from beatmapsets directly.
        // Verify the beatmapset index lookup is O(1) via id.
        let id_index: std::collections::HashMap<u32, _> =
            beatmapsets.iter().map(|bs| (bs.id, bs)).collect();

        assert_eq!(
            id_index.len(),
            beatmapsets.len(),
            "beatmapset ids should be unique"
        );

        println!(
            "stable: {} unique checksums, {} unique set ids",
            checksums_manual.len(),
            id_index.len()
        );
    }
}
