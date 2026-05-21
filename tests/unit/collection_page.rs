use crate::app::collection::{CollectionPage, FailureReason};
use crate::download::FailedMap;

fn page_with_failures(reasons: &[FailureReason]) -> CollectionPage {
    let mut page = CollectionPage::new(1, "test".to_string(), 2);
    page.set_failed_maps(
        reasons
            .iter()
            .enumerate()
            .map(|(i, &reason)| FailedMap {
                beatmapset_id: i as u32 + 1,
                title: None,
                reason,
            })
            .collect(),
    );
    page
}

#[test]
fn toggle_failed_section_flips_expanded() {
    let mut page = page_with_failures(&[FailureReason::NotFound]);
    assert!(!page.failed_section_expanded);
    page.toggle_failed_section();
    assert!(page.failed_section_expanded);
    page.toggle_failed_section();
    assert!(!page.failed_section_expanded);
}

#[test]
fn toggle_failed_section_noop_when_empty() {
    let mut page = CollectionPage::new(1, "test".to_string(), 2);
    page.toggle_failed_section();
    assert!(!page.failed_section_expanded);
}

#[test]
fn set_failed_maps_stores_reason_and_sorts_by_id() {
    let mut page = CollectionPage::new(1, "test".to_string(), 2);
    page.set_failed_maps(vec![
        FailedMap {
            beatmapset_id: 30,
            title: None,
            reason: FailureReason::NetworkError,
        },
        FailedMap {
            beatmapset_id: 10,
            title: None,
            reason: FailureReason::NotFound,
        },
        FailedMap {
            beatmapset_id: 20,
            title: None,
            reason: FailureReason::ValidationFailed,
        },
    ]);
    let ids: Vec<u32> = page.failed_maps.iter().map(|f| f.beatmapset_id).collect();
    assert_eq!(ids, vec![10, 20, 30]);
    assert_eq!(page.failed_maps[0].reason, FailureReason::NotFound);
    assert_eq!(page.failed_maps[1].reason, FailureReason::ValidationFailed);
    assert_eq!(page.failed_maps[2].reason, FailureReason::NetworkError);
}

#[test]
fn failure_reason_labels_are_correct() {
    assert_eq!(FailureReason::NotFound.label(), "not found");
    assert_eq!(FailureReason::RateLimited.label(), "rate-limited");
    assert_eq!(FailureReason::NetworkError.label(), "network error");
    assert_eq!(FailureReason::ValidationFailed.label(), "archive invalid");
    assert_eq!(FailureReason::Unknown.label(), "unknown error");
}
