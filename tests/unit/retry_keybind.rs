use crate::{
    app::{App, AppCommand, collection::FailureReason},
    config::Config,
    download::{DownloadConfig, DownloadId, DownloadStage, FailedMap},
};
/// Retry key binding tests.
///
/// Covers `r` / `R` dispatch, NotFound skipping, letter suppression,
/// confirm modal threshold, and help overlay text.
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

fn make_app() -> App {
    App::new(Config::default())
}

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

fn shift(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

/// Navigate to a download tab that has `n` failed maps with the given reasons.
/// Returns the `DownloadId` of the page.
fn setup_download_tab_with_failures(app: &mut App, reasons: &[FailureReason]) -> DownloadId {
    use crate::app::collection::CollectionPage;
    use crate::config::constants::STATIC_TABS;

    let id: DownloadId = 99;
    let mut page = CollectionPage::new(id, "test".to_string(), 2);
    page.stage = DownloadStage::Completed;
    page.download_config = Some(DownloadConfig {
        directory: "/tmp".to_string(),
        mirrors: vec![],
        concurrent: 2,
        archive_validation: osu_downloader::ArchiveValidation::Magic,
    });
    page.output_dir = Some("/tmp/test".to_string());
    page.set_failed_maps(
        reasons
            .iter()
            .enumerate()
            .map(|(i, &reason)| FailedMap {
                beatmapset_id: (i as u32) + 1,
                title: Some(format!("Map {}", i + 1)),
                reason,
            })
            .collect(),
    );
    page.failed_section_expanded = true;

    app.downloads.push(page);
    app.active_tab = STATIC_TABS + app.downloads.len() - 1;
    id
}

// ── r on focused failed row ───────────────────────────────────────────────────

#[test]
fn r_on_focused_failed_row_emits_retry_failed_map() {
    let mut app = make_app();
    let download_id = setup_download_tab_with_failures(&mut app, &[FailureReason::NetworkError]);

    // focus the first (only) row
    app.active_download_page_mut().unwrap().failed_focus_next();

    let cmd = app.handle_key(press(KeyCode::Char('r')));
    assert!(
        matches!(
            cmd,
            Some(AppCommand::RetryFailedMap {
                download_id: did,
                beatmapset_id: 1
            }) if did == download_id
        ),
        "r on focused row should emit RetryFailedMap with the right IDs"
    );
}

#[test]
fn r_without_focus_emits_nothing() {
    let mut app = make_app();
    setup_download_tab_with_failures(&mut app, &[FailureReason::NetworkError]);
    // failed_focus is None by default
    let cmd = app.handle_key(press(KeyCode::Char('r')));
    assert!(
        cmd.is_none(),
        "r without a focused row must not emit any command"
    );
}

#[test]
fn r_on_not_found_row_emits_nothing() {
    let mut app = make_app();
    setup_download_tab_with_failures(&mut app, &[FailureReason::NotFound]);

    app.active_download_page_mut().unwrap().failed_focus_next();

    let cmd = app.handle_key(press(KeyCode::Char('r')));
    assert!(
        cmd.is_none(),
        "r on a NotFound row must be skipped silently"
    );
}

#[test]
fn r_removes_retried_row_from_failed_maps() {
    let mut app = make_app();
    setup_download_tab_with_failures(
        &mut app,
        &[FailureReason::NetworkError, FailureReason::RateLimited],
    );

    // focus first row (beatmapset_id = 1)
    app.active_download_page_mut().unwrap().failed_focus_next();

    let _ = app.handle_key(press(KeyCode::Char('r')));

    let page = app.active_download_page_mut().unwrap();
    assert_eq!(page.failed_maps.len(), 1, "retried row must be removed");
    assert_eq!(
        page.failed_maps[0].beatmapset_id, 2,
        "remaining row should be the second map"
    );
}

// ── R retry-all without threshold ────────────────────────────────────────────

#[test]
fn capital_r_with_few_failures_emits_retry_all_immediately() {
    let mut app = make_app();
    let download_id = setup_download_tab_with_failures(
        &mut app,
        &[FailureReason::NetworkError, FailureReason::RateLimited],
    );

    let cmd = app.handle_key(shift(KeyCode::Char('R')));
    assert!(
        matches!(cmd, Some(AppCommand::RetryAllFailed { download_id: did }) if did == download_id),
        "R with <50 failures must emit RetryAllFailed immediately"
    );
}

#[test]
fn capital_r_skips_not_found_in_retry_all() {
    let mut app = make_app();
    // only NotFound failures — none are retryable
    setup_download_tab_with_failures(
        &mut app,
        &[FailureReason::NotFound, FailureReason::NotFound],
    );

    let cmd = app.handle_key(shift(KeyCode::Char('R')));
    assert!(
        cmd.is_none(),
        "R with only NotFound failures must emit nothing"
    );
}

#[test]
fn capital_r_not_found_excluded_from_count_below_threshold() {
    let mut app = make_app();
    // 2 NotFound + 3 retryable = total 5, retryable = 3 → no modal
    let reasons: Vec<FailureReason> = std::iter::repeat_n(FailureReason::NotFound, 2)
        .chain(std::iter::repeat_n(FailureReason::NetworkError, 3))
        .collect();
    let download_id = setup_download_tab_with_failures(&mut app, &reasons);

    let cmd = app.handle_key(shift(KeyCode::Char('R')));
    assert!(
        matches!(cmd, Some(AppCommand::RetryAllFailed { download_id: did }) if did == download_id),
        "R with retryable count ≤ 50 must not open the confirm modal"
    );
    assert!(
        app.confirm_retry.is_none(),
        "confirm_retry must remain None when count ≤ 50"
    );
}

// ── R >50 confirm modal ───────────────────────────────────────────────────────

#[test]
fn capital_r_over_50_opens_confirm_modal() {
    let mut app = make_app();
    let reasons: Vec<FailureReason> =
        std::iter::repeat_n(FailureReason::NetworkError, 51).collect();
    let download_id = setup_download_tab_with_failures(&mut app, &reasons);

    let cmd = app.handle_key(shift(KeyCode::Char('R')));
    assert!(
        cmd.is_none(),
        "R with >50 failures must not immediately emit RetryAllFailed"
    );
    assert!(
        app.confirm_retry.is_some(),
        "confirm_retry modal must be open"
    );
    let modal = app.confirm_retry.as_ref().unwrap();
    assert_eq!(modal.download_id, download_id);
    assert_eq!(modal.retryable_count, 51);
}

#[test]
fn enter_on_confirm_modal_emits_retry_all_and_closes_modal() {
    let mut app = make_app();
    let reasons: Vec<FailureReason> =
        std::iter::repeat_n(FailureReason::NetworkError, 51).collect();
    let download_id = setup_download_tab_with_failures(&mut app, &reasons);

    app.handle_key(shift(KeyCode::Char('R')));
    assert!(app.confirm_retry.is_some());

    let cmd = app.handle_key(press(KeyCode::Enter));
    assert!(
        matches!(cmd, Some(AppCommand::RetryAllFailed { download_id: did }) if did == download_id),
        "enter on confirm modal must emit RetryAllFailed"
    );
    assert!(
        app.confirm_retry.is_none(),
        "modal must be closed after confirmation"
    );
}

#[test]
fn esc_on_confirm_modal_cancels_without_retrying() {
    let mut app = make_app();
    let reasons: Vec<FailureReason> =
        std::iter::repeat_n(FailureReason::NetworkError, 51).collect();
    setup_download_tab_with_failures(&mut app, &reasons);

    app.handle_key(shift(KeyCode::Char('R')));
    assert!(app.confirm_retry.is_some());

    let cmd = app.handle_key(press(KeyCode::Esc));
    assert!(cmd.is_none(), "esc must not emit any command");
    assert!(
        app.confirm_retry.is_none(),
        "modal must be closed after esc"
    );
}

// ── letter suppression ────────────────────────────────────────────────────────

#[test]
fn r_on_home_tab_text_input_does_not_emit_retry() {
    use crate::app::HomeField;

    let mut app = make_app();
    // home tab, collection field is focused by default (text input)
    assert_eq!(app.home.focus, HomeField::Collection);

    let cmd = app.handle_key(press(KeyCode::Char('r')));
    // r on the home tab's text field should NOT emit RetryFailedMap —
    // it types into the collection input instead
    assert!(
        !matches!(cmd, Some(AppCommand::RetryFailedMap { .. })),
        "r on a text input must not emit RetryFailedMap"
    );
}

// ── help overlay text ─────────────────────────────────────────────────────────

#[test]
fn help_overlay_contains_retry_bindings() {
    use ratatui::{Terminal, backend::TestBackend};

    let mut app = make_app();
    app.help_open = true;

    let backend = TestBackend::new(80, 40);
    let mut terminal = Terminal::new(backend).expect("test backend");
    terminal
        .draw(|frame| crate::tui::draw(frame, &app))
        .expect("render");

    let rendered: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();

    assert!(
        rendered.contains('r'),
        "help overlay must show the 'r' retry binding"
    );
    assert!(
        rendered.contains('R'),
        "help overlay must show the 'R' retry-all binding"
    );
}

// ── row focus navigation ──────────────────────────────────────────────────────

#[test]
fn up_down_cycles_through_failed_rows_when_expanded() {
    let mut app = make_app();
    setup_download_tab_with_failures(
        &mut app,
        &[FailureReason::NetworkError, FailureReason::RateLimited],
    );

    let page = app.active_download_page_mut().unwrap();
    assert!(
        page.failed_focus.is_none(),
        "focus starts at section header"
    );

    app.handle_key(press(KeyCode::Down));
    assert_eq!(
        app.active_download_page_mut().unwrap().failed_focus,
        Some(0)
    );
    app.handle_key(press(KeyCode::Down));
    assert_eq!(
        app.active_download_page_mut().unwrap().failed_focus,
        Some(1)
    );
    // wraps back to header
    app.handle_key(press(KeyCode::Down));
    assert!(
        app.active_download_page_mut()
            .unwrap()
            .failed_focus
            .is_none()
    );

    // going up from header wraps to last row
    app.handle_key(press(KeyCode::Up));
    assert_eq!(
        app.active_download_page_mut().unwrap().failed_focus,
        Some(1)
    );
}

#[test]
fn failed_focus_clears_on_collapse() {
    let mut app = make_app();
    setup_download_tab_with_failures(&mut app, &[FailureReason::NetworkError]);

    app.active_download_page_mut().unwrap().failed_focus_next();
    assert_eq!(
        app.active_download_page_mut().unwrap().failed_focus,
        Some(0)
    );

    // enter toggles the section (collapses it)
    app.handle_key(press(KeyCode::Enter));
    assert!(
        app.active_download_page_mut()
            .unwrap()
            .failed_focus
            .is_none(),
        "focus must clear when the section collapses"
    );
}
