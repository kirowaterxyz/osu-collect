use super::{delete_last_word, delete_word_left};

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

#[test]
fn word_left_deletes_only_before_caret() {
    // caret after "bar" (char index 7): the trailing " baz" is preserved.
    let mut s = "foo bar baz".to_string();
    let caret = delete_word_left(&mut s, 7);
    assert_eq!(s, "foo  baz");
    assert_eq!(caret, 4, "caret lands at the deletion start");
}

#[test]
fn word_left_at_zero_is_noop() {
    let mut s = "foo".to_string();
    let caret = delete_word_left(&mut s, 0);
    assert_eq!(s, "foo");
    assert_eq!(caret, 0);
}

#[test]
fn word_left_respects_char_boundaries() {
    // caret after the whole value; multi-byte char must not be split.
    let mut s = "café build".to_string();
    let end = s.chars().count();
    let caret = delete_word_left(&mut s, end);
    assert_eq!(s, "café ");
    assert_eq!(caret, "café ".chars().count());
}
