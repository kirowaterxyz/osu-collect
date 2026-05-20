use super::{message_style, scroll_window, truncate_to_width};
use crate::download::BeatmapStage;
use crate::tui::{
    DANGER, FILL_BLOCK, FILL_H_LINE, FILL_SHADE, FILL_SPACE, GLYPH_BLOCK, GLYPH_H_LINE,
    GLYPH_SHADE, GLYPH_SPACE, SUCCESS, TEXT_DIM, TEXT_FAINT, WARNING, glyph_fill,
};

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
fn truncate_to_width_handles_zero() {
    assert_eq!(truncate_to_width("hello", 0).0, "");
}

#[test]
fn truncate_to_width_one_returns_ellipsis() {
    assert_eq!(truncate_to_width("hello", 1).0, "…");
}

#[test]
fn truncate_to_width_unicode_safe() {
    assert_eq!(truncate_to_width("こんにちは世界", 4).0, "こんに…");
}

#[test]
fn message_style_rate_limited_overrides() {
    use ratatui::style::Style;
    assert_eq!(
        message_style(BeatmapStage::Success, true),
        Style::default().fg(WARNING)
    );
}

#[test]
fn message_style_stage_classification() {
    use ratatui::style::Style;
    assert_eq!(
        message_style(BeatmapStage::Success, false),
        Style::default().fg(SUCCESS)
    );
    assert_eq!(
        message_style(BeatmapStage::Skipped, false),
        Style::default().fg(TEXT_FAINT)
    );
    assert_eq!(
        message_style(BeatmapStage::Failed, false),
        Style::default().fg(DANGER)
    );
    assert_eq!(
        message_style(BeatmapStage::Aborted, false),
        Style::default().fg(DANGER)
    );
    assert_eq!(
        message_style(BeatmapStage::Downloading, false),
        Style::default().fg(TEXT_DIM)
    );
    assert_eq!(
        message_style(BeatmapStage::Pending, false),
        Style::default().fg(TEXT_DIM)
    );
    assert_eq!(
        message_style(BeatmapStage::Verifying, false),
        Style::default().fg(TEXT_DIM)
    );
}

#[test]
fn glyph_fill_zero_is_empty() {
    assert_eq!(glyph_fill(&FILL_BLOCK, GLYPH_BLOCK, 0).as_ref(), "");
    assert_eq!(glyph_fill(&FILL_SHADE, GLYPH_SHADE, 0).as_ref(), "");
    assert_eq!(glyph_fill(&FILL_H_LINE, GLYPH_H_LINE, 0).as_ref(), "");
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
            glyph_fill(&FILL_H_LINE, GLYPH_H_LINE, n).as_ref(),
            GLYPH_H_LINE.repeat(n),
            "H_LINE n={n}"
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
        glyph_fill(&FILL_H_LINE, GLYPH_H_LINE, n).as_ref(),
        GLYPH_H_LINE.repeat(n)
    );
}
