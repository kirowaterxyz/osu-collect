use super::{Theme, blend};
use ratatui::style::Color;

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

#[test]
fn blend_full_tier_midpoint_is_between_inputs() {
    // With the Full (truecolor) theme initialized via default(), blend at t=0.5
    // must produce an Rgb midpoint strictly between the two inputs.
    let a = Color::Rgb(200, 100, 50);
    let b = Color::Rgb(0, 0, 0);
    let mid = blend(a, b, 0.5);
    assert_eq!(
        mid,
        Color::Rgb(100, 50, 25),
        "t=0.5 midpoint of (200,100,50) and (0,0,0)"
    );
}

#[test]
fn blend_full_tier_at_zero_returns_b() {
    let a = Color::Rgb(255, 0, 0);
    let b = Color::Rgb(10, 20, 30);
    assert_eq!(blend(a, b, 0.0), b, "t=0.0 must return b unchanged");
}

#[test]
fn blend_full_tier_at_one_returns_a() {
    let a = Color::Rgb(255, 0, 0);
    let b = Color::Rgb(10, 20, 30);
    assert_eq!(blend(a, b, 1.0), a, "t=1.0 must return a unchanged");
}

#[test]
fn blend_wash_is_not_bg_or_semantic() {
    // The 25% danger wash used for banner backgrounds must differ from both
    // pure bg and pure danger so the tint is visible.
    let t = Theme::full();
    let wash = blend(t.danger, t.bg, 0.25);
    assert_ne!(wash, t.bg, "danger wash must not equal plain bg");
    assert_ne!(wash, t.danger, "danger wash must not equal full danger");
    let wash_w = blend(t.warning, t.bg, 0.25);
    assert_ne!(wash_w, t.bg, "warning wash must not equal plain bg");
    assert_ne!(
        wash_w, t.warning,
        "warning wash must not equal full warning"
    );
}
