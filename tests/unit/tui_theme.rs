use super::Theme;

#[test]
fn default_theme_has_distinct_warning_and_text_dim() {
    let t = Theme::default();
    assert_ne!(
        t.warning, t.text_dim,
        "default: warning and text_dim must differ (quit toast contrast)"
    );
    assert_ne!(t.warning, t.bg, "default: warning must not match bg");
    assert_ne!(t.text_dim, t.bg, "default: text_dim must not match bg");
}

#[test]
fn sixteen_theme_has_distinct_warning_and_text_dim() {
    let t = Theme::sixteen();
    assert_ne!(
        t.warning, t.text_dim,
        "sixteen: warning and text_dim must differ (quit toast contrast)"
    );
    assert_ne!(t.warning, t.bg, "sixteen: warning must not match bg");
    assert_ne!(t.text_dim, t.bg, "sixteen: text_dim must not match bg");
}

#[test]
fn colorblind_safe_theme_has_distinct_warning_and_text_dim() {
    let t = Theme::colorblind_safe();
    assert_ne!(
        t.warning, t.text_dim,
        "colorblind-safe: warning and text_dim must differ (quit toast contrast)"
    );
    assert_ne!(
        t.warning, t.bg,
        "colorblind-safe: warning must not match bg"
    );
    assert_ne!(
        t.text_dim, t.bg,
        "colorblind-safe: text_dim must not match bg"
    );
}

#[test]
fn all_three_themes_construct_without_panic() {
    let _default = Theme::default();
    let _sixteen = Theme::sixteen();
    let _cb = Theme::colorblind_safe();
}

#[test]
fn colorblind_safe_success_and_danger_differ() {
    let t = Theme::colorblind_safe();
    assert_ne!(
        t.success, t.danger,
        "colorblind-safe: success and danger must be distinguishable"
    );
}
