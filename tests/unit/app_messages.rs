use super::{AppMessage, clear_app_message, set_loading_message};

#[test]
fn set_loading_replaces_the_slot() {
    let mut slot: Option<AppMessage> = None;
    set_loading_message(&mut slot, "fetching collections...");
    assert_eq!(
        slot.as_ref().map(|m| m.text.as_str()),
        Some("fetching collections...")
    );

    set_loading_message(&mut slot, "reading database...");
    assert_eq!(
        slot.as_ref().map(|m| m.text.as_str()),
        Some("reading database...")
    );
}

#[test]
fn clear_empties_the_slot() {
    let mut slot = Some(AppMessage {
        text: "loading".to_string(),
    });
    clear_app_message(&mut slot);
    assert!(slot.is_none());
}
