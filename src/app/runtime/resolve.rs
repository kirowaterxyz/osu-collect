use crate::{
    app::home::ResolveState, app::url_history, core::collection::CollectionService, utils,
};
use osu_downloader::collection::CollectionClient;
use tokio::{sync::mpsc, sync::watch, time};

const DEBOUNCE_MS: u64 = 300;

/// Result of a URL-field resolve attempt, sent back to the main loop.
#[derive(Debug)]
pub enum HomeResolveEvent {
    /// Resolve started; show a loading indicator.
    Loading,
    /// Metadata fetched successfully.
    /// `url` is the literal value that was in the collection field when the task was spawned.
    Resolved {
        url: String,
        name: String,
        map_count: usize,
    },
    /// Fetch failed; `reason` is a short user-facing message.
    Failed { reason: String },
    /// Field is empty or unparseable; clear any prior display.
    Cleared,
}

/// Abort any previous resolve task and start a new debounced one.
///
/// If `value` does not parse as a collection ID, sends `Cleared` immediately
/// and returns without spawning a task.
pub fn schedule_resolve(
    value: &str,
    resolve_handle: &mut Option<tokio::task::JoinHandle<()>>,
    resolve_cancel_tx: &mut Option<watch::Sender<bool>>,
    home_resolve_tx: &mpsc::UnboundedSender<HomeResolveEvent>,
) {
    // Abort any in-flight task.
    if let Some(handle) = resolve_handle.take() {
        handle.abort();
    }
    // Signal cancellation to any task that is still starting up.
    if let Some(tx) = resolve_cancel_tx.take() {
        let _ = tx.send(true);
    }

    let trimmed = value.trim();
    let Ok(collection_id) = utils::parse_collection_id(trimmed) else {
        // Not a parseable URL/ID — clear display immediately.
        let _ = home_resolve_tx.send(HomeResolveEvent::Cleared);
        return;
    };

    let (cancel_tx, cancel_rx) = watch::channel(false);
    *resolve_cancel_tx = Some(cancel_tx);

    let url = trimmed.to_string();
    let tx = home_resolve_tx.clone();
    let handle = tokio::spawn(async move {
        run_resolve(url, collection_id, cancel_rx, tx).await;
    });
    *resolve_handle = Some(handle);
}

async fn run_resolve(
    url: String,
    collection_id: u32,
    mut cancel_rx: watch::Receiver<bool>,
    tx: mpsc::UnboundedSender<HomeResolveEvent>,
) {
    // Debounce: wait 300 ms, cancel if the field changes again.
    tokio::select! {
        _ = time::sleep(time::Duration::from_millis(DEBOUNCE_MS)) => {}
        _ = cancel_rx.changed() => return,
    }

    let _ = tx.send(HomeResolveEvent::Loading);

    let client = CollectionClient::new();
    let service = crate::core::collection::HttpCollectionService::new(client);

    tokio::select! {
        result = service.fetch_collection(collection_id) => {
            let event = match result {
                Ok(collection) => HomeResolveEvent::Resolved {
                    url,
                    name: collection.name,
                    map_count: collection.beatmapsets.len(),
                },
                Err(err) => HomeResolveEvent::Failed {
                    reason: user_facing_error(&err.to_string()),
                },
            };
            let _ = tx.send(event);
        }
        _ = cancel_rx.changed() => {}
    }
}

/// Collapse verbose API error messages to a short user-facing phrase.
fn user_facing_error(err: &str) -> String {
    if err.contains("not found") || err.contains("404") {
        "collection not found".to_string()
    } else if err.contains("rate limited") || err.contains("429") {
        "rate limited — try again later".to_string()
    } else if err.contains("timed out") || err.contains("timeout") {
        "network timeout".to_string()
    } else {
        "network error".to_string()
    }
}

pub fn handle_home_resolve_event(event: HomeResolveEvent, home: &mut crate::app::HomeTab) {
    match event {
        HomeResolveEvent::Loading => {
            home.set_collection_resolve(ResolveState::Loading, "resolving…");
        }
        HomeResolveEvent::Resolved {
            url,
            name,
            map_count,
        } => {
            let maps_word = if map_count == 1 { "map" } else { "maps" };
            home.set_collection_resolve(
                ResolveState::Success,
                format!("\"{}\" · {} {}", name, map_count, maps_word),
            );
            // Persist to history and save to disk.
            let entry = url_history::UrlHistoryEntry {
                url,
                name,
                count: map_count,
                last_used: current_timestamp(),
            };
            url_history::push(&mut home.url_history, entry);
            url_history::save(&home.url_history);
        }
        HomeResolveEvent::Failed { reason } => {
            home.set_collection_resolve(ResolveState::Error, reason);
        }
        HomeResolveEvent::Cleared => {
            home.clear_collection_resolve();
        }
    }
}

fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // RFC3339-ish timestamp using only std — the `time` crate formatting
    // feature is not guaranteed available here. Format: 2006-01-02T15:04:05Z.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    // Gregorian calendar calculation (Tomohiko Sakamoto / Zeller variant).
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
#[path = "../../../tests/unit/home_resolve.rs"]
mod tests;
