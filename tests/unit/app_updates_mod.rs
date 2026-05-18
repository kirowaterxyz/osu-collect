use osu_collect::app::updates::{ScanStatus, UpdatesTab, scroll_list};

#[test]
fn needs_initial_scan_reflects_cache_state() {
    let mut tab = UpdatesTab::new();
    assert!(tab.needs_initial_scan(), "idle tab needs a scan");

    tab.scan.scan_status = ScanStatus::ReadingDatabase;
    assert!(
        !tab.needs_initial_scan(),
        "in-flight scan should not restart"
    );

    tab.scan.scan_status = ScanStatus::FetchingCollection;
    assert!(!tab.needs_initial_scan());

    tab.scan.scan_status = ScanStatus::Ready;
    assert!(!tab.needs_initial_scan(), "cached results should be reused");

    tab.scan.scan_status = ScanStatus::Error;
    assert!(tab.needs_initial_scan(), "errored scans retry on tab entry");
}

#[test]
fn scroll_list_clamps_within_bounds() {
    let mut state = Some(0);
    scroll_list(&mut state, 3, -1);
    assert_eq!(state, Some(0));
    scroll_list(&mut state, 3, 1);
    assert_eq!(state, Some(1));
    scroll_list(&mut state, 3, 10);
    assert_eq!(state, Some(2));
}

#[test]
fn scroll_list_empty_leaves_state() {
    let mut state: Option<usize> = None;
    scroll_list(&mut state, 0, 1);
    assert_eq!(state, None);
}

#[test]
fn scroll_list_none_starts_at_zero() {
    let mut state: Option<usize> = None;
    scroll_list(&mut state, 5, 1);
    assert_eq!(state, Some(1));
}
