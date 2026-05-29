use super::delete_last_word;

fn deleted(input: &str) -> String {
    let mut s = input.to_string();
    delete_last_word(&mut s);
    s
}

#[test]
fn deletes_trailing_word() {
    assert_eq!(deleted("hello world"), "hello ");
    assert_eq!(deleted("hello "), "");
    assert_eq!(deleted("single"), "");
}

#[test]
fn deletes_path_segment() {
    assert_eq!(deleted("/home/user/foo"), "/home/user/");
    assert_eq!(deleted("/home/user/"), "/home/");
}

#[test]
fn empty_string_is_noop() {
    assert_eq!(deleted(""), "");
    assert_eq!(deleted("   "), "");
}

#[test]
fn respects_char_boundaries() {
    // multi-byte chars must not be split mid-codepoint
    assert_eq!(deleted("café build"), "café ");
}
