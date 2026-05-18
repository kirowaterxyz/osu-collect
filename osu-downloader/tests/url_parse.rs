use crate::parse_collection_id;

#[test]
fn accepts_numeric_id() {
    assert_eq!(parse_collection_id("12345").unwrap(), 12345);
    assert_eq!(parse_collection_id("  12345  ").unwrap(), 12345);
}

#[test]
fn accepts_collector_url() {
    assert_eq!(
        parse_collection_id("https://osucollector.com/collections/12345").unwrap(),
        12345
    );
    assert_eq!(
        parse_collection_id("https://osucollector.com/collections/12345/").unwrap(),
        12345
    );
}

#[test]
fn rejects_blank_or_garbage() {
    assert!(parse_collection_id("").is_err());
    assert!(parse_collection_id("   ").is_err());
    assert!(parse_collection_id("not-a-url").is_err());
}

#[test]
fn rejects_non_https_or_wrong_host() {
    assert!(parse_collection_id("http://osucollector.com/collections/1").is_err());
    assert!(parse_collection_id("https://example.com/collections/1").is_err());
}

#[test]
fn rejects_wrong_path() {
    assert!(parse_collection_id("https://osucollector.com/users/1").is_err());
    assert!(parse_collection_id("https://osucollector.com/collections/").is_err());
    assert!(parse_collection_id("https://osucollector.com/collections/1/x").is_err());
}

#[test]
fn rejects_non_numeric_id() {
    assert!(parse_collection_id("https://osucollector.com/collections/abc").is_err());
}
