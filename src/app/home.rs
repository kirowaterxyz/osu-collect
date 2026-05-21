use super::{
    messages::{AppMessage, set_info_message},
    next_field, prev_field,
    url_history::UrlHistoryFile,
};
use crate::{
    app::runtime::ProbeResult,
    config::Config,
    download::{ArchiveValidation, DownloadConfig, DownloadRequest},
    mirrors::{Mirror, MirrorKind},
    utils::{CompletionResult, complete_dir, expand_tilde, pretty_path},
};
use std::{collections::HashMap, env, str::FromStr};

/// Indicates what the collection-resolve row should look like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveState {
    Loading,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct InputField {
    pub label: &'static str,
    pub value: String,
    pub placeholder: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeField {
    Collection,
    Directory,
    CustomMirror,
    MirrorNerinyan,
    MirrorOsuDirect,
    MirrorSayobot,
    MirrorNekoha,
    Threads,
    AutoOverwrite,
    NoVideo,
}

const HOME_FIELDS: &[HomeField] = &[
    HomeField::Collection,
    HomeField::Directory,
    HomeField::CustomMirror,
    HomeField::MirrorOsuDirect,
    HomeField::MirrorNerinyan,
    HomeField::MirrorSayobot,
    HomeField::MirrorNekoha,
    HomeField::Threads,
    HomeField::AutoOverwrite,
    HomeField::NoVideo,
];

impl HomeField {
    pub fn is_text_input(self) -> bool {
        matches!(
            self,
            HomeField::Collection | HomeField::Directory | HomeField::CustomMirror
        )
    }

    pub fn is_stepper(self) -> bool {
        self == HomeField::Threads
    }
}

pub struct HomeTab {
    pub collection: InputField,
    pub directory: InputField,
    pub custom_mirror: InputField,
    pub threads: InputField,
    pub auto_overwrite: bool,
    pub nerinyan: bool,
    pub osu_direct: bool,
    pub sayobot: bool,
    pub nekoha: bool,
    pub no_video: bool,
    pub focus: HomeField,
    pub message: Option<AppMessage>,
    /// Resolve status shown below the collection URL field.
    /// Unlike `message`, this is not TTL-expired; it persists until the field changes.
    pub collection_resolve: Option<(ResolveState, String)>,
    /// Cache of the last successfully resolved collection: `(id, beatmapset_ids)`.
    /// Used by `App::request_download` to intersect with the persisted
    /// failed-maps file before dispatching the pipeline.
    pub resolved_collection: Option<(u32, Vec<u32>)>,
    /// Latency probe results per built-in mirror. `None` = not yet probed,
    /// `Some(None)` = probe in flight (`…`), `Some(Some(_))` = result received.
    pub mirror_latency: HashMap<MirrorKind, Option<ProbeResult>>,
    pub quit_prompt: bool,
    pub default_threads: u8,
    /// Previously resolved URLs, loaded from disk on startup.
    pub url_history: UrlHistoryFile,
    /// Whether the history dropdown is currently visible.
    pub dropdown_open: bool,
    /// Index of the highlighted entry in the dropdown (0 = first).
    pub dropdown_selected: Option<usize>,
    default_directory: String,
}

impl HomeTab {
    pub fn new(config: &Config) -> Self {
        let nerinyan = config.mirror.nerinyan;
        let osu_direct = config.mirror.osu_direct;
        let sayobot = config.mirror.sayobot;
        let nekoha = config.mirror.nekoha;
        let custom_template = config.mirror.custom_template().unwrap_or("");

        // One syscall: raw form for submit fallback, pretty form for placeholder.
        let cwd = env::current_dir();
        let default_directory = cwd
            .as_deref()
            .map(|dir| dir.to_string_lossy().into_owned())
            .unwrap_or_else(|_| ".".to_string());
        // Placeholder shows the tilde-collapsed path so long cwd is readable.
        let placeholder_directory = cwd
            .as_deref()
            .map(|dir| pretty_path(dir).into_owned())
            .unwrap_or_else(|_| ".".to_string());

        let default_threads = config.download.resolved_concurrent();
        let threads_value = config
            .download
            .concurrent
            .map(|value| value.to_string())
            .unwrap_or_default();

        Self {
            collection: InputField {
                label: "Collection URL or ID",
                value: String::new(),
                placeholder: "https://osucollector.com/collections/...".to_string(),
            },
            directory: InputField {
                label: "Download directory",
                value: String::new(),
                placeholder: placeholder_directory,
            },
            custom_mirror: InputField {
                label: "Custom mirror URL (optional)",
                value: custom_template.to_string(),
                placeholder: "https://example.com/d/{id}".to_string(),
            },
            threads: InputField {
                label: "Threads",
                value: threads_value,
                placeholder: default_threads.to_string(),
            },
            auto_overwrite: false,
            nerinyan,
            osu_direct,
            sayobot,
            nekoha,
            no_video: config.download.no_video,
            focus: HomeField::Collection,
            message: None,
            collection_resolve: None,
            resolved_collection: None,
            mirror_latency: HashMap::with_capacity(4),
            quit_prompt: false,
            default_threads,
            url_history: super::url_history::load(),
            dropdown_open: false,
            dropdown_selected: None,
            default_directory,
        }
    }

    /// Mark all built-in mirrors as "probe in flight" (`…`).
    pub fn mirror_probe_started(&mut self) {
        for kind in MirrorKind::BUILTINS {
            self.mirror_latency.insert(*kind, None);
        }
    }

    /// Store the result for a single mirror.
    pub fn set_mirror_latency(&mut self, kind: MirrorKind, result: ProbeResult) {
        self.mirror_latency.insert(kind, Some(result));
    }

    pub fn clear_collection_resolve(&mut self) {
        self.collection_resolve = None;
        self.resolved_collection = None;
    }

    pub fn set_collection_resolve(&mut self, state: ResolveState, text: impl Into<String>) {
        self.collection_resolve = Some((state, text.into()));
    }

    /// Cache the resolved beatmapset id list for the current collection. Read
    /// by `App::request_download` to intersect with persisted failures.
    pub fn set_resolved_collection(&mut self, collection_id: u32, beatmapset_ids: Vec<u32>) {
        self.resolved_collection = Some((collection_id, beatmapset_ids));
    }

    /// Open the history dropdown if there are entries.
    /// Does nothing when the collection field already has text.
    pub fn open_dropdown(&mut self) {
        if self.url_history.entries.is_empty() || !self.collection.value.is_empty() {
            return;
        }
        self.dropdown_open = true;
        self.dropdown_selected = Some(0);
    }

    /// Close the history dropdown and clear the selection.
    pub fn close_dropdown(&mut self) {
        self.dropdown_open = false;
        self.dropdown_selected = None;
    }

    /// Move the dropdown selection up (wraps).
    pub fn dropdown_prev(&mut self) {
        let len = self.url_history.entries.len();
        if len == 0 {
            return;
        }
        self.dropdown_selected = Some(match self.dropdown_selected {
            None | Some(0) => len - 1,
            Some(i) => i - 1,
        });
    }

    /// Move the dropdown selection down (wraps).
    pub fn dropdown_next(&mut self) {
        let len = self.url_history.entries.len();
        if len == 0 {
            return;
        }
        self.dropdown_selected = Some(match self.dropdown_selected {
            None => 0,
            Some(i) => (i + 1) % len,
        });
    }

    /// Accept the highlighted dropdown entry: fill the collection field and close.
    /// Returns the selected URL (to trigger resolve), or `None` if nothing is selected.
    pub fn dropdown_accept(&mut self) -> Option<String> {
        let idx = self.dropdown_selected?;
        let entry = self.url_history.entries.get(idx)?;
        let url = entry.url.clone();
        self.collection.value = url.clone();
        self.close_dropdown();
        Some(url)
    }

    pub fn next_field(&mut self) {
        self.focus = next_field(HOME_FIELDS, self.focus);
    }

    pub fn prev_field(&mut self) {
        self.focus = prev_field(HOME_FIELDS, self.focus);
    }

    /// Run tab-completion on the directory input field.
    ///
    /// Only acts when focus is `HomeField::Directory`. On a single match the
    /// value is completed in-place. On multiple matches the value is completed
    /// to the longest common prefix and the candidates are shown as an info
    /// message. On no match nothing changes.
    pub fn tab_complete_directory(&mut self) {
        if self.focus != HomeField::Directory {
            return;
        }
        match complete_dir(&self.directory.value) {
            CompletionResult::Single(completed) => {
                self.directory.value = completed;
            }
            CompletionResult::Ambiguous {
                completed,
                candidates,
            } => {
                self.directory.value = completed;
                // Show up to 5 candidates; truncate with "…" if more.
                const MAX_SHOWN: usize = 5;
                let display = if candidates.len() <= MAX_SHOWN {
                    candidates.join(", ")
                } else {
                    let shown = candidates[..MAX_SHOWN].join(", ");
                    format!("{shown}, … ({} more)", candidates.len() - MAX_SHOWN)
                };
                set_info_message(&mut self.message, display);
            }
            CompletionResult::NoMatch => {}
        }
    }

    /// Increment the thread count by one, capped at `default_threads`.
    pub fn step_up(&mut self) {
        self.step(1);
    }

    /// Decrement the thread count by one, floored at 1.
    pub fn step_down(&mut self) {
        self.step(-1);
    }

    fn step(&mut self, delta: i16) {
        let current = self.resolved_threads() as i16;
        let max = self.default_threads as i16;
        let next = (current + delta).clamp(1, max) as u8;
        self.threads.value = next.to_string();
    }

    pub fn handle_char(&mut self, ch: char) {
        // Any character typed while the dropdown is open dismisses it first.
        if self.dropdown_open {
            self.close_dropdown();
        }
        match self.focus {
            HomeField::Collection => self.collection.value.push(ch),
            HomeField::Directory => self.directory.value.push(ch),
            HomeField::CustomMirror => self.custom_mirror.value.push(ch),
            // Threads is a stepper — char input is silently ignored.
            HomeField::Threads
            | HomeField::MirrorNerinyan
            | HomeField::MirrorOsuDirect
            | HomeField::MirrorSayobot
            | HomeField::MirrorNekoha
            | HomeField::AutoOverwrite
            | HomeField::NoVideo => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.focus {
            HomeField::Collection => {
                self.collection.value.pop();
            }
            HomeField::Directory => {
                self.directory.value.pop();
            }
            HomeField::CustomMirror => {
                self.custom_mirror.value.pop();
            }
            // Threads is a stepper — backspace is silently ignored.
            HomeField::Threads
            | HomeField::MirrorNerinyan
            | HomeField::MirrorOsuDirect
            | HomeField::MirrorSayobot
            | HomeField::MirrorNekoha
            | HomeField::AutoOverwrite
            | HomeField::NoVideo => {}
        }
    }

    pub fn toggle_current(&mut self) {
        match self.focus {
            HomeField::MirrorNerinyan => {
                self.nerinyan = !self.nerinyan;
            }
            HomeField::MirrorOsuDirect => {
                self.osu_direct = !self.osu_direct;
            }
            HomeField::MirrorSayobot => {
                self.sayobot = !self.sayobot;
            }
            HomeField::MirrorNekoha => {
                self.nekoha = !self.nekoha;
            }
            HomeField::AutoOverwrite => {
                self.auto_overwrite = !self.auto_overwrite;
            }
            HomeField::NoVideo => {
                self.no_video = !self.no_video;
            }
            _ => {}
        }
    }

    pub fn build_mirror_list(&self) -> Vec<Mirror> {
        let builtin_checks: &[(bool, MirrorKind)] = &[
            (self.nerinyan, MirrorKind::Nerinyan),
            (self.osu_direct, MirrorKind::OsuDirect),
            (self.sayobot, MirrorKind::Sayobot),
            (self.nekoha, MirrorKind::Nekoha),
        ];

        let mut mirrors: Vec<Mirror> = builtin_checks
            .iter()
            .filter_map(|&(enabled, kind)| {
                if !enabled {
                    return None;
                }
                let mirror = Mirror::builtin(kind)?;
                Some(if self.no_video {
                    mirror.no_video()
                } else {
                    mirror
                })
            })
            .collect();

        let trimmed_custom = self.custom_mirror.value.trim();
        if !trimmed_custom.is_empty()
            && let Ok(custom) = Mirror::custom(trimmed_custom)
        {
            mirrors.push(custom);
        }

        mirrors
    }

    pub fn build_request(
        &self,
        archive_validation: ArchiveValidation,
    ) -> Result<DownloadRequest, String> {
        let collection_input = self.collection.value.trim();
        if collection_input.is_empty() {
            return Err("Collection ID or URL is required".to_string());
        }

        let directory = if self.directory.value.trim().is_empty() {
            self.default_directory.clone()
        } else {
            // Expand `~` at submit time so the filesystem layer receives an
            // absolute path regardless of how the user typed the value.
            expand_tilde(self.directory.value.trim())
        };

        let threads_value = if self.threads.value.trim().is_empty() {
            self.default_threads
        } else {
            parse_thread_count(&self.threads.value)?
        };

        if threads_value == 0 || threads_value > 50 {
            return Err("Thread count must be between 1 and 50".to_string());
        }

        let mirrors = self.build_mirror_list();
        if mirrors.is_empty() {
            return Err("Select at least one mirror".to_string());
        }

        let config = DownloadConfig {
            directory,
            mirrors,
            concurrent: threads_value,
            archive_validation,
        };

        Ok(DownloadRequest {
            collection_input: collection_input.to_string(),
            config,
            auto_overwrite: self.auto_overwrite,
            // Default `false`; `App::request_download` resolves the
            // retry-failed-on-download policy and overrides it (or surfaces a
            // modal under `Ask` before the download is dispatched).
            include_previously_failed: false,
        })
    }

    pub fn resolved_threads(&self) -> u8 {
        if self.threads.value.trim().is_empty() {
            self.default_threads
        } else {
            parse_thread_count(&self.threads.value).unwrap_or(self.default_threads)
        }
    }
}

fn parse_thread_count(value: &str) -> Result<u8, String> {
    u8::from_str(value.trim()).map_err(|_| "Thread count must be between 1 and 50".to_string())
}

#[cfg(test)]
#[path = "../../tests/unit/home.rs"]
mod tests;
