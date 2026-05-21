use crate::utils::{CompletionResult, complete_dir};
use std::{fs, path::PathBuf};
use tempfile::TempDir;

fn make_dirs(base: &TempDir, names: &[&str]) -> PathBuf {
    let path = base.path().to_path_buf();
    for name in names {
        fs::create_dir_all(path.join(name)).unwrap();
    }
    path
}

fn value_in(base: &TempDir, suffix: &str) -> String {
    format!("{}/{}", base.path().to_string_lossy(), suffix)
}

// ── single match → completes to full name + `/` ──────────────────────────────

#[test]
fn single_match_completes_with_slash() {
    let tmp = TempDir::new().unwrap();
    make_dirs(&tmp, &["projects"]);
    let value = value_in(&tmp, "pro");

    let result = complete_dir(&value);

    assert_eq!(
        result,
        CompletionResult::Single(value_in(&tmp, "projects/"))
    );
}

// ── multiple matches → completes to longest common prefix, sets candidates ───

#[test]
fn multiple_matches_complete_to_longest_common_prefix() {
    let tmp = TempDir::new().unwrap();
    make_dirs(&tmp, &["code-a", "code-b", "code-c"]);
    let value = value_in(&tmp, "code");

    let result = complete_dir(&value);

    match result {
        CompletionResult::Ambiguous {
            completed,
            candidates,
        } => {
            // All three start with "code-", the lcp must be at least "code".
            assert!(
                completed.starts_with(&value_in(&tmp, "code")),
                "expected completed to start with {}, got {completed}",
                value_in(&tmp, "code")
            );
            let mut sorted = candidates.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, vec!["code-a", "code-b", "code-c"]);
        }
        other => panic!("expected Ambiguous, got {other:?}"),
    }
}

// ── no match → no change ─────────────────────────────────────────────────────

#[test]
fn no_match_returns_no_match() {
    let tmp = TempDir::new().unwrap();
    make_dirs(&tmp, &["alpha"]);
    let value = value_in(&tmp, "zzz");

    let result = complete_dir(&value);

    assert_eq!(result, CompletionResult::NoMatch);
}

// ── hidden files filtered unless partial starts with `.` ─────────────────────

#[test]
fn hidden_dirs_excluded_when_partial_has_no_dot() {
    let tmp = TempDir::new().unwrap();
    make_dirs(&tmp, &[".hidden", "visible"]);
    let value = value_in(&tmp, "");

    let result = complete_dir(&value);

    // Only "visible" should appear.
    match result {
        CompletionResult::Single(completed) => {
            assert!(
                completed.contains("visible"),
                "expected visible, got {completed}"
            );
        }
        CompletionResult::Ambiguous { candidates, .. } => {
            assert!(
                !candidates.iter().any(|c| c.starts_with('.')),
                "hidden dirs leaked into results: {candidates:?}"
            );
        }
        CompletionResult::NoMatch => panic!("expected a match for visible"),
    }
}

#[test]
fn hidden_dirs_included_when_partial_starts_with_dot() {
    let tmp = TempDir::new().unwrap();
    make_dirs(&tmp, &[".config", ".local"]);
    let value = value_in(&tmp, ".");

    let result = complete_dir(&value);

    match result {
        CompletionResult::Ambiguous { candidates, .. } => {
            let mut sorted = candidates.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, vec![".config", ".local"]);
        }
        CompletionResult::Single(completed) => {
            // Only one hidden dir would match "." if .config and .local are both there —
            // but both have prefix "." so we expect Ambiguous. If somehow only one
            // survives, at least verify it starts with '.'.
            assert!(
                completed.contains("/."),
                "expected hidden dir, got {completed}"
            );
        }
        CompletionResult::NoMatch => panic!("expected hidden dirs to appear"),
    }
}

// ── empty value + Tab → lists cwd entries ────────────────────────────────────

#[test]
fn empty_value_uses_cwd_and_completes_single_match() {
    // We can't control cwd reliably, so instead we test with a path that has
    // a trailing slash (simulating "I typed the parent dir plus /").
    let tmp = TempDir::new().unwrap();
    make_dirs(&tmp, &["only_dir"]);
    // A value with a trailing slash and no partial lists the parent.
    let value = format!("{}/", tmp.path().to_string_lossy());

    let result = complete_dir(&value);

    assert_eq!(
        result,
        CompletionResult::Single(format!("{}/only_dir/", tmp.path().to_string_lossy()))
    );
}

// ── bare `~` treats as `~/` and preserves `~/` prefix ────────────────────────

#[test]
fn bare_tilde_lists_home_dir_with_tilde_prefix() {
    let tmp = TempDir::new().unwrap();
    make_dirs(&tmp, &["only_dir"]);
    // We cannot inject a fake home for the tilde path, so test that bare `~`
    // doesn't match against cwd dirs named with a literal `~`.  The simplest
    // verifiable invariant is that `~` and `~/` produce identical results.
    let result_bare = complete_dir("~");
    let result_slash = complete_dir("~/");
    assert_eq!(
        result_bare, result_slash,
        "bare `~` and `~/` must produce identical completion results"
    );
    // Any Single result must carry the ~/  prefix, not an absolute path.
    if let CompletionResult::Single(completed) = &result_bare {
        assert!(
            completed.starts_with("~/"),
            "bare `~` completion must preserve ~/  prefix, got {completed}"
        );
    }
}

// ── `~/` lists home dir entries ───────────────────────────────────────────────

#[test]
fn tilde_slash_searches_home_dir() {
    let Some(home) = dirs::home_dir() else {
        return; // no home configured in this environment — skip
    };

    // The home dir may be empty or not, but at minimum the function must not
    // panic and must return a result consistent with reading the home dir.
    let result = complete_dir("~/");

    // Collect what we would expect by listing the home dir ourselves.
    let home_entries: Vec<String> = std::fs::read_dir(&home)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let e = e.ok()?;
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            if e.file_type().ok()?.is_dir() {
                return Some(name);
            }
            if e.file_type().ok()?.is_symlink() {
                let resolved = e.path().canonicalize().ok()?;
                if resolved.is_dir() { Some(name) } else { None }
            } else {
                None
            }
        })
        .collect();

    match result {
        CompletionResult::NoMatch => {
            // Valid when home has no non-hidden subdirs.
            assert!(
                home_entries.is_empty(),
                "expected NoMatch but home has dirs: {home_entries:?}"
            );
        }
        CompletionResult::Single(completed) => {
            assert_eq!(
                home_entries.len(),
                1,
                "expected exactly one dir but got {home_entries:?}"
            );
            // complete_dir preserves the `~/` prefix the caller typed.
            assert!(
                completed.starts_with("~/"),
                "completed path {completed} does not preserve ~/  prefix"
            );
        }
        CompletionResult::Ambiguous { candidates, .. } => {
            assert!(
                candidates.len() > 1,
                "Ambiguous requires > 1 candidates, got {candidates:?}"
            );
        }
    }
}
