#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::CustomMirrorList;
use crate::mirrors::MirrorKind;

#[test]
fn from_templates_appends_trailing_empty_slot() {
    let list = CustomMirrorList::from_templates(&["https://a.example/d/{id}"]);
    // One entered row + the always-present empty entry slot.
    assert_eq!(list.row_count(), 2);
    assert_eq!(list.row(0).unwrap().value, "https://a.example/d/{id}");
    assert_eq!(list.row(1).unwrap().value, "");
}

#[test]
fn empty_config_still_has_one_entry_slot() {
    let list = CustomMirrorList::from_templates(&[]);
    assert_eq!(list.row_count(), 1);
    assert_eq!(list.row(0).unwrap().value, "");
}

#[test]
fn ensure_trailing_empty_grows_when_last_row_filled() {
    let mut list = CustomMirrorList::from_templates(&[]);
    list.row_mut(0)
        .unwrap()
        .set_value("https://a.example/d/{id}");
    list.ensure_trailing_empty();
    assert_eq!(list.row_count(), 2);
    assert_eq!(list.row(1).unwrap().value, "");
    // Idempotent: a second call does not stack more empty rows.
    list.ensure_trailing_empty();
    assert_eq!(list.row_count(), 2);
}

#[test]
fn compact_drops_interior_empties_keeps_one_trailing() {
    let mut list =
        CustomMirrorList::from_templates(&["https://a.example/d/{id}", "https://b.example/d/{id}"]);
    // Clear the first row, leaving an interior empty.
    list.row_mut(0).unwrap().set_value("");
    list.compact();
    assert_eq!(list.row_count(), 2);
    assert_eq!(list.row(0).unwrap().value, "https://b.example/d/{id}");
    assert_eq!(list.row(1).unwrap().value, "");
}

#[test]
fn valid_count_ignores_empty_and_malformed_rows() {
    let list = CustomMirrorList::from_templates(&[
        "https://a.example/d/{id}", // valid
        "not-a-url",                // malformed
        "https://b.example/d/{id}", // valid
    ]);
    assert_eq!(list.valid_count(), 2);
}

#[test]
fn nonempty_templates_keeps_malformed_for_persistence() {
    let list = CustomMirrorList::from_templates(&["https://a.example/d/{id}", "oops"]);
    let persisted = list.nonempty_templates();
    assert_eq!(persisted.len(), 2);
    assert_eq!(persisted[0].as_ref(), "https://a.example/d/{id}");
    assert_eq!(persisted[1].as_ref(), "oops");
}

#[test]
fn build_mirrors_yields_one_per_valid_row_in_order() {
    let list = CustomMirrorList::from_templates(&[
        "https://a.example/d/{id}",
        "bad",
        "https://b.example/d/{id}",
    ]);
    let mirrors = list.build_mirrors(true);
    assert_eq!(mirrors.len(), 2);
    assert!(mirrors.iter().all(|m| m.kind() == MirrorKind::Custom));
    assert_eq!(mirrors[0].host(), "a.example");
    assert_eq!(mirrors[1].host(), "b.example");
}
