use super::Theme;

#[test]
fn full_theme_has_distinct_warning_and_text_dim() {
    let t = Theme::full();
    assert_ne!(
        t.warning, t.text_dim,
        "full: warning and text_dim must differ (quit toast contrast)"
    );
    assert_ne!(t.warning, t.bg, "full: warning must not match bg");
    assert_ne!(t.text_dim, t.bg, "full: text_dim must not match bg");
}

#[test]
fn compatible_theme_has_distinct_warning_and_text_dim() {
    let t = Theme::compatible();
    assert_ne!(
        t.warning, t.text_dim,
        "compatible: warning and text_dim must differ (quit toast contrast)"
    );
    assert_ne!(t.warning, t.bg, "compatible: warning must not match bg");
    assert_ne!(t.text_dim, t.bg, "compatible: text_dim must not match bg");
}

#[test]
fn both_themes_construct_without_panic() {
    let _full = Theme::full();
    let _compatible = Theme::compatible();
}

#[test]
fn full_theme_success_and_danger_differ() {
    let t = Theme::full();
    assert_ne!(
        t.success, t.danger,
        "full: success and danger must be distinguishable"
    );
}

#[test]
fn compatible_theme_success_and_danger_differ() {
    let t = Theme::compatible();
    assert_ne!(
        t.success, t.danger,
        "compatible: success and danger must be distinguishable"
    );
}
