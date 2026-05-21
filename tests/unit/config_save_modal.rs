use crate::app::config::ConfigTab;
use crate::config::{Config, DisplayConfig, DownloadConfig, ThemeMode};

// ── loaded_config snapshot update after save ─────────────────────────────────

/// After `loaded_config` is updated to the newly saved value the diff must
/// be empty, so a second press of `s` shows "no changes to save".
#[test]
fn diff_clears_after_loaded_config_update() {
    let mut tab = default_tab();
    tab.no_video = !tab.no_video;
    let pending = tab.build_config().expect("valid config");
    assert!(
        tab.has_pending_changes(&pending),
        "precondition: diff non-empty"
    );

    // Simulate what confirm_save_config does after a successful write.
    tab.loaded_config = pending.clone();

    let after = tab.build_config().expect("valid config");
    assert!(
        !tab.has_pending_changes(&after),
        "diff must be empty after loaded_config is updated"
    );
}

fn default_tab() -> ConfigTab {
    ConfigTab::new(&Config::default())
}

// ── has_pending_changes ──────────────────────────────────────────────────────

#[test]
fn no_diff_when_form_matches_loaded_config() {
    let tab = default_tab();
    let pending = tab.build_config().expect("valid config");
    assert!(!tab.has_pending_changes(&pending));
}

#[test]
fn detects_no_video_change() {
    let mut tab = default_tab();
    tab.no_video = !tab.no_video;
    let pending = tab.build_config().expect("valid config");
    assert!(tab.has_pending_changes(&pending));
}

#[test]
fn detects_theme_change() {
    let mut tab = default_tab();
    tab.theme = ThemeMode::ColorblindSafe;
    let pending = tab.build_config().expect("valid config");
    assert!(tab.has_pending_changes(&pending));
}

#[test]
fn detects_logging_enabled_change() {
    let mut tab = default_tab();
    tab.logging_enabled = !tab.logging_enabled;
    let pending = tab.build_config().expect("valid config");
    assert!(tab.has_pending_changes(&pending));
}

#[test]
fn detects_mirror_toggle() {
    let mut tab = default_tab();
    tab.nerinyan = !tab.nerinyan;
    let pending = tab.build_config().expect("valid config");
    assert!(tab.has_pending_changes(&pending));
}

// ── diff_entries ─────────────────────────────────────────────────────────────

#[test]
fn empty_diff_when_nothing_changed() {
    let tab = default_tab();
    let pending = tab.build_config().expect("valid config");
    assert!(tab.diff_entries(&pending).is_empty());
}

#[test]
fn diff_contains_no_video_entry() {
    let mut tab = default_tab();
    let original_no_video = tab.no_video;
    tab.no_video = !original_no_video;
    let pending = tab.build_config().expect("valid config");
    let diff = tab.diff_entries(&pending);

    let entry = diff
        .iter()
        .find(|e| e.label == "skip videos")
        .expect("skip videos entry must be present");

    assert_ne!(entry.old_value, entry.new_value);
}

#[test]
fn diff_contains_theme_entry_with_correct_labels() {
    let config = Config {
        display: DisplayConfig {
            theme: ThemeMode::Default,
        },
        ..Config::default()
    };
    let mut tab = ConfigTab::new(&config);
    tab.theme = ThemeMode::ColorblindSafe;
    let pending = tab.build_config().expect("valid config");
    let diff = tab.diff_entries(&pending);

    let entry = diff
        .iter()
        .find(|e| e.label == "theme")
        .expect("theme entry must be present");

    assert_eq!(entry.old_value, "default");
    assert_eq!(entry.new_value, "colorblind-safe");
}

#[test]
fn diff_skips_unchanged_fields() {
    let mut tab = default_tab();
    tab.no_video = true;
    // Re-sync loaded_config so that no_video is the baseline.
    let synced = Config {
        download: DownloadConfig {
            no_video: true,
            ..Config::default().download
        },
        ..Config::default()
    };
    let _tab = ConfigTab::new(&synced);

    // Now toggle something else — no_video should NOT appear in the diff.
    let mut tab2 = ConfigTab::new(&synced);
    tab2.theme = ThemeMode::Sixteen;
    let pending = tab2.build_config().expect("valid config");
    let diff = tab2.diff_entries(&pending);

    assert!(
        diff.iter().all(|e| e.label != "skip videos"),
        "unchanged field must be absent"
    );
    assert!(
        diff.iter().any(|e| e.label == "theme"),
        "changed field must be present"
    );
}

#[test]
fn diff_contains_multiple_changed_fields() {
    let mut tab = default_tab();
    tab.no_video = true;
    tab.theme = ThemeMode::Sixteen;
    tab.logging_enabled = true;
    let pending = tab.build_config().expect("valid config");
    let diff = tab.diff_entries(&pending);

    assert!(diff.iter().any(|e| e.label == "skip videos"));
    assert!(diff.iter().any(|e| e.label == "theme"));
    assert!(diff.iter().any(|e| e.label == "logging"));
}

#[test]
fn diff_threads_shows_resolved_default_when_empty() {
    // When threads field is empty, resolved value is default_threads.
    let tab = default_tab();
    // Both loaded and pending resolve the same default — no diff.
    let pending = tab.build_config().expect("valid config");
    let diff = tab.diff_entries(&pending);
    assert!(diff.iter().all(|e| e.label != "threads"));
}
