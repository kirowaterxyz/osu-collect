use super::{input_cursor_col, message_style, panel_cursor, scroll_window, truncate_to_width};
use crate::app::InputField;
use crate::download::BeatmapStage;
use crate::tui::{
    FILL_BLOCK, FILL_SHADE, FILL_SPACE, GLYPH_BLOCK, GLYPH_SHADE, GLYPH_SPACE, danger, glyph_fill,
    success, text_dim, text_faint, warning,
};
use ratatui::layout::Rect;

#[test]
fn scroll_window_keeps_focus_centered() {
    let items: Vec<u8> = (0..20).collect();
    let (start, end) = scroll_window(&items, 10, 5);
    assert!((start..end).contains(&10));
    assert_eq!(end - start, 5);
}

#[test]
fn scroll_window_clamps_to_end() {
    let items: Vec<u8> = (0..10).collect();
    let (start, end) = scroll_window(&items, 9, 4);
    assert_eq!(end, 10);
    assert_eq!(start, 6);
}

#[test]
fn scroll_window_empty_visible() {
    let items: Vec<u8> = (0..10).collect();
    assert_eq!(scroll_window(&items, 3, 0), (0, 10));
}

#[test]
fn input_cursor_col_counts_prefix_label_and_value() {
    // `new` parks the caret at the end, so the column lands past the full value.
    let field = InputField::new("Threads", "ab", ""); // label lowercases to "threads" (7)
    // focus marker (2) + "threads" (7) + ": " (2) + caret offset (2) = 13
    assert_eq!(input_cursor_col(&field), 13);
}

#[test]
fn input_cursor_col_tracks_caret_offset_not_value_length() {
    let mut field = InputField::new("Threads", "ab", "");
    field.caret_home();
    // caret at 0: focus marker (2) + "threads" (7) + ": " (2) + 0 = 11
    assert_eq!(input_cursor_col(&field), 11);
}

#[test]
fn panel_cursor_none_when_no_column() {
    let inner = Rect::new(2, 3, 40, 10);
    assert_eq!(panel_cursor(inner, 5, 0, 10, None), None);
}

#[test]
fn panel_cursor_none_when_row_scrolled_out() {
    let inner = Rect::new(2, 3, 40, 10);
    assert_eq!(panel_cursor(inner, 12, 0, 10, Some(4)), None);
}

#[test]
fn panel_cursor_maps_row_and_clamps_column() {
    let inner = Rect::new(2, 3, 10, 10); // x=2, width=10 → last col 11
    // focused row 4, window starts at 2 → visible row 2 → y = 3 + 2 = 5
    assert_eq!(panel_cursor(inner, 4, 2, 10, Some(5)), Some((7, 5)));
    // column past the edge clamps to inner.x + width - 1 = 11
    assert_eq!(panel_cursor(inner, 4, 2, 10, Some(99)), Some((11, 5)));
}

#[test]
fn truncate_to_width_handles_zero() {
    assert_eq!(truncate_to_width("hello", 0).0, "");
}

#[test]
fn truncate_to_width_one_returns_ellipsis() {
    assert_eq!(truncate_to_width("hello", 1).0, "…");
}

#[test]
fn truncate_to_width_unicode_safe() {
    // Each CJK char is display-width 2. Budget 4 → reserve 1 for "…" → 3 cols for chars.
    // "こ" = 2 cols fits; "こん" = 4 cols exceeds 3 → result is "こ…" (3 cols total).
    assert_eq!(truncate_to_width("こんにちは世界", 4).0, "こ…");
    // Budget 7 → 6 cols for chars → "こんに" (6 cols) fits → "こんに…" (7 cols total).
    assert_eq!(truncate_to_width("こんにちは世界", 7).0, "こんに…");
    // ASCII still works: budget 5 → "hell…"
    assert_eq!(truncate_to_width("hello world", 5).0, "hell…");
}

#[test]
fn message_style_rate_limited_overrides() {
    use ratatui::style::Style;
    assert_eq!(
        message_style(BeatmapStage::Success, true),
        Style::default().fg(warning())
    );
}

#[test]
fn message_style_stage_classification() {
    use ratatui::style::Style;
    assert_eq!(
        message_style(BeatmapStage::Success, false),
        Style::default().fg(success())
    );
    assert_eq!(
        message_style(BeatmapStage::Skipped, false),
        Style::default().fg(text_faint())
    );
    assert_eq!(
        message_style(BeatmapStage::Failed, false),
        Style::default().fg(danger())
    );
    assert_eq!(
        message_style(BeatmapStage::Aborted, false),
        Style::default().fg(danger())
    );
    assert_eq!(
        message_style(BeatmapStage::Downloading, false),
        Style::default().fg(text_dim())
    );
    assert_eq!(
        message_style(BeatmapStage::Pending, false),
        Style::default().fg(text_dim())
    );
    assert_eq!(
        message_style(BeatmapStage::Verifying, false),
        Style::default().fg(text_dim())
    );
}

#[test]
fn glyph_fill_zero_is_empty() {
    assert_eq!(glyph_fill(&FILL_BLOCK, GLYPH_BLOCK, 0).as_ref(), "");
    assert_eq!(glyph_fill(&FILL_SHADE, GLYPH_SHADE, 0).as_ref(), "");
    assert_eq!(glyph_fill(&FILL_SPACE, GLYPH_SPACE, 0).as_ref(), "");
}

#[test]
fn glyph_fill_matches_repeat_for_all_glyphs() {
    for n in [1, 4, 12, 80, 160, 220, 256] {
        assert_eq!(
            glyph_fill(&FILL_BLOCK, GLYPH_BLOCK, n).as_ref(),
            GLYPH_BLOCK.repeat(n),
            "BLOCK n={n}"
        );
        assert_eq!(
            glyph_fill(&FILL_SHADE, GLYPH_SHADE, n).as_ref(),
            GLYPH_SHADE.repeat(n),
            "SHADE n={n}"
        );
        assert_eq!(
            glyph_fill(&FILL_SPACE, GLYPH_SPACE, n).as_ref(),
            GLYPH_SPACE.repeat(n),
            "SPACE n={n}"
        );
    }
}

#[test]
fn glyph_fill_fallback_above_max_width() {
    let n = 257;
    assert_eq!(
        glyph_fill(&FILL_BLOCK, GLYPH_BLOCK, n).as_ref(),
        GLYPH_BLOCK.repeat(n)
    );
    assert_eq!(
        glyph_fill(&FILL_SHADE, GLYPH_SHADE, n).as_ref(),
        GLYPH_SHADE.repeat(n)
    );
}
