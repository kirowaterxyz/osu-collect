use super::{messages::AppMessage, next_field, prev_field};
use crate::{
    config::Config,
    download::{ArchiveValidation, DownloadConfig, DownloadRequest},
    mirrors::{Mirror, MirrorKind},
};
use std::{env, str::FromStr};

#[derive(Debug, Clone)]
pub struct InputField {
    pub label: &'static str,
    pub value: String,
    pub placeholder: String,
}

impl InputField {
    fn push(&mut self, ch: char) {
        self.value.push(ch);
    }

    fn pop(&mut self) {
        self.value.pop();
    }
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
            HomeField::Collection
                | HomeField::Directory
                | HomeField::CustomMirror
                | HomeField::Threads
        )
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
    pub quit_prompt: bool,
    default_threads: u8,
    default_directory: String,
}

impl HomeTab {
    pub fn new(config: &Config) -> Self {
        let nerinyan = config.mirror.nerinyan;
        let osu_direct = config.mirror.osu_direct;
        let sayobot = config.mirror.sayobot;
        let nekoha = config.mirror.nekoha;
        let custom_template = config.mirror.custom_template().unwrap_or("");

        let default_directory = env::current_dir()
            .map(|dir| dir.to_string_lossy().into_owned())
            .unwrap_or_else(|_| ".".to_string());

        let placeholder_directory = default_directory.clone();

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
            quit_prompt: false,
            default_threads,
            default_directory,
        }
    }

    pub fn next_field(&mut self) {
        self.focus = next_field(HOME_FIELDS, self.focus);
    }

    pub fn prev_field(&mut self) {
        self.focus = prev_field(HOME_FIELDS, self.focus);
    }

    pub fn handle_char(&mut self, ch: char) {
        match self.focus {
            HomeField::Collection => self.collection.push(ch),
            HomeField::Directory => self.directory.push(ch),
            HomeField::CustomMirror => self.custom_mirror.push(ch),
            HomeField::Threads => {
                if ch.is_ascii_digit() {
                    self.threads.push(ch);
                }
            }
            HomeField::MirrorNerinyan
            | HomeField::MirrorOsuDirect
            | HomeField::MirrorSayobot
            | HomeField::MirrorNekoha
            | HomeField::AutoOverwrite
            | HomeField::NoVideo => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.focus {
            HomeField::Collection => self.collection.pop(),
            HomeField::Directory => self.directory.pop(),
            HomeField::CustomMirror => self.custom_mirror.pop(),
            HomeField::Threads => self.threads.pop(),
            HomeField::MirrorNerinyan
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
                if enabled {
                    Mirror::builtin(kind, self.no_video)
                } else {
                    None
                }
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
            self.directory.value.trim().to_string()
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
        })
    }

    pub fn build_mirrors(&self) -> Vec<Mirror> {
        self.build_mirror_list()
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
