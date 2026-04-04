use osu_collect::download::passes::FailureReport;
use osu_collect::mirrors::MirrorKind;

#[test]
fn failure_report_records_unique_beatmaps() {
    let mut report = FailureReport::default();
    report.record(100, "timeout".into(), Some(MirrorKind::Nerinyan));
    report.record(100, "timeout again".into(), Some(MirrorKind::Nerinyan));
    report.record(200, "not found".into(), None);
    assert_eq!(report.beatmaps().len(), 2);
}

#[test]
fn failure_report_tracks_mirror_stats() {
    let mut report = FailureReport::default();
    report.record(100, "Rate limited".into(), Some(MirrorKind::Nerinyan));
    report.record(200, "Rate limited".into(), Some(MirrorKind::Nerinyan));
    let top = report.describe_top_failure();
    assert!(top.is_some());
    assert!(top.unwrap().contains("Nerinyan"));
}

#[test]
fn failure_report_empty() {
    let report = FailureReport::default();
    assert!(report.is_empty());
    assert!(report.describe_top_failure().is_none());
}
