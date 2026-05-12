use super::messages::AppMessage;
use crate::{
    config::{Config, OfficialConfig},
    download::{DownloadConfig, DownloadRequest},
    mirrors::{CatboyRegion, MirrorEndpoint, MirrorKind},
};
use std::{env, str::FromStr};
use tracing::warn;

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
    MirrorCatboyCentral,
    MirrorCatboyUs,
    MirrorCatboyAsia,
    MirrorOfficial,
    Threads,
    SkipExisting,
    AutoOverwrite,
    NoVideo,
}

pub struct HomeTab {
    pub collection: InputField,
    pub directory: InputField,
    pub custom_mirror: InputField,
    pub threads: InputField,
    pub skip_existing: bool,
    pub auto_overwrite: bool,
    pub nerinyan: bool,
    pub catboy_central: bool,
    pub catboy_us: bool,
    pub catboy_asia: bool,
    pub osu_direct: bool,
    pub sayobot: bool,
    pub nekoha: bool,
    pub official: bool,
    pub no_video: bool,
    pub verify_zip_eocd: bool,
    pub focus: HomeField,
    pub message: Option<AppMessage>,
    pub quit_prompt: bool,
    default_threads: u8,
    default_retries: u8,
    default_directory: String,
    official_config: OfficialConfig,
}

impl HomeTab {
    pub fn new(config: &Config) -> Self {
        let mut nerinyan = config.mirror.nerinyan;
        let catboy_central = config.mirror.catboy_central;
        let catboy_us = config.mirror.catboy_us;
        let catboy_asia = config.mirror.catboy_asia;
        let osu_direct = config.mirror.osu_direct;
        let sayobot = config.mirror.sayobot;
        let nekoha = config.mirror.nekoha;
        let official_config = config.official.clone();
        let official = config.mirror.official
            && (crate::auth::load().is_some() || official_config.credentials().is_some());
        if config.mirror.official && !official {
            warn!(
                "official mirror enabled in config but no auth or credentials found; set official.client_id and official.client_secret"
            );
        }
        let custom_template = config.mirror.custom_template().unwrap_or("");

        if !nerinyan
            && !catboy_central
            && !catboy_us
            && !catboy_asia
            && !osu_direct
            && !sayobot
            && !nekoha
            && !official
            && custom_template.is_empty()
        {
            nerinyan = true;
        }

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
        let default_retries = config.download.resolved_max_retries();

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
                placeholder: "3".to_string(),
            },
            skip_existing: config.download.skip_existing,
            auto_overwrite: false,
            nerinyan,
            catboy_central,
            catboy_us,
            catboy_asia,
            osu_direct,
            sayobot,
            nekoha,
            official,
            no_video: config.download.no_video,
            verify_zip_eocd: config.download.verify_zip_eocd,
            focus: HomeField::Collection,
            message: None,
            quit_prompt: false,
            default_threads,
            default_retries,
            default_directory,
            official_config,
        }
    }

    pub fn next_field(&mut self) {
        self.focus = match self.focus {
            HomeField::Collection => HomeField::Directory,
            HomeField::Directory => HomeField::CustomMirror,
            HomeField::CustomMirror => HomeField::MirrorNerinyan,
            HomeField::MirrorNerinyan => HomeField::MirrorOsuDirect,
            HomeField::MirrorOsuDirect => HomeField::MirrorSayobot,
            HomeField::MirrorSayobot => HomeField::MirrorNekoha,
            HomeField::MirrorNekoha => HomeField::MirrorCatboyCentral,
            HomeField::MirrorCatboyCentral => HomeField::MirrorCatboyUs,
            HomeField::MirrorCatboyUs => HomeField::MirrorCatboyAsia,
            HomeField::MirrorCatboyAsia => HomeField::MirrorOfficial,
            HomeField::MirrorOfficial => HomeField::Threads,
            HomeField::Threads => HomeField::SkipExisting,
            HomeField::SkipExisting => HomeField::AutoOverwrite,
            HomeField::AutoOverwrite => HomeField::NoVideo,
            HomeField::NoVideo => HomeField::Collection,
        };
    }

    pub fn prev_field(&mut self) {
        self.focus = match self.focus {
            HomeField::Collection => HomeField::NoVideo,
            HomeField::Directory => HomeField::Collection,
            HomeField::CustomMirror => HomeField::Directory,
            HomeField::MirrorNerinyan => HomeField::CustomMirror,
            HomeField::MirrorOsuDirect => HomeField::MirrorNerinyan,
            HomeField::MirrorSayobot => HomeField::MirrorOsuDirect,
            HomeField::MirrorNekoha => HomeField::MirrorSayobot,
            HomeField::MirrorCatboyCentral => HomeField::MirrorNekoha,
            HomeField::MirrorCatboyUs => HomeField::MirrorCatboyCentral,
            HomeField::MirrorCatboyAsia => HomeField::MirrorCatboyUs,
            HomeField::MirrorOfficial => HomeField::MirrorCatboyAsia,
            HomeField::Threads => HomeField::MirrorOfficial,
            HomeField::SkipExisting => HomeField::Threads,
            HomeField::AutoOverwrite => HomeField::SkipExisting,
            HomeField::NoVideo => HomeField::AutoOverwrite,
        };
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
            | HomeField::MirrorCatboyCentral
            | HomeField::MirrorCatboyUs
            | HomeField::MirrorCatboyAsia
            | HomeField::MirrorOsuDirect
            | HomeField::MirrorSayobot
            | HomeField::MirrorNekoha
            | HomeField::MirrorOfficial
            | HomeField::SkipExisting
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
            | HomeField::MirrorCatboyCentral
            | HomeField::MirrorCatboyUs
            | HomeField::MirrorCatboyAsia
            | HomeField::MirrorOsuDirect
            | HomeField::MirrorSayobot
            | HomeField::MirrorNekoha
            | HomeField::MirrorOfficial
            | HomeField::SkipExisting
            | HomeField::AutoOverwrite
            | HomeField::NoVideo => {}
        }
    }

    pub fn toggle_current(&mut self) {
        match self.focus {
            HomeField::MirrorNerinyan => {
                self.nerinyan = !self.nerinyan;
            }
            HomeField::MirrorCatboyCentral => {
                self.catboy_central = !self.catboy_central;
            }
            HomeField::MirrorCatboyUs => {
                self.catboy_us = !self.catboy_us;
            }
            HomeField::MirrorCatboyAsia => {
                self.catboy_asia = !self.catboy_asia;
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
            HomeField::MirrorOfficial => {
                if !self.official
                    && crate::auth::load().is_none()
                    && self.official_config.credentials().is_none()
                {
                    warn!("no auth tokens or official credentials");
                } else {
                    self.official = !self.official;
                }
            }
            HomeField::SkipExisting => {
                self.skip_existing = !self.skip_existing;
                if self.skip_existing {
                    self.auto_overwrite = false;
                }
            }
            HomeField::AutoOverwrite => {
                self.auto_overwrite = !self.auto_overwrite;
                if self.auto_overwrite {
                    self.skip_existing = false;
                }
            }
            HomeField::NoVideo => {
                self.no_video = !self.no_video;
            }
            _ => {}
        }
    }

    pub fn set_error(&mut self, message: &str) {
        self.message = Some(AppMessage::error(message));
    }

    pub fn set_info(&mut self, message: &str) {
        self.message = Some(AppMessage::info(message));
    }

    pub fn clear_expired_message(&mut self) {
        if self.message.as_ref().is_some_and(AppMessage::is_expired) {
            self.message = None;
        }
    }

    pub fn build_mirror_list(&self) -> Vec<MirrorEndpoint> {
        let builtin_checks: &[(bool, MirrorKind)] = &[
            (self.nerinyan, MirrorKind::Nerinyan),
            (self.osu_direct, MirrorKind::OsuDirect),
            (self.sayobot, MirrorKind::Sayobot),
            (self.nekoha, MirrorKind::Nekoha),
            (
                self.catboy_central,
                MirrorKind::Catboy(CatboyRegion::Central),
            ),
            (self.catboy_us, MirrorKind::Catboy(CatboyRegion::Us)),
            (self.catboy_asia, MirrorKind::Catboy(CatboyRegion::Asia)),
        ];

        let mut mirrors: Vec<MirrorEndpoint> = builtin_checks
            .iter()
            .filter_map(|&(enabled, kind)| {
                if enabled {
                    MirrorEndpoint::builtin(kind, self.no_video)
                } else {
                    None
                }
            })
            .collect();

        if self.official {
            if let Some(auth) = crate::auth::load() {
                mirrors.push(MirrorEndpoint::official(auth.bearer_token()));
            } else if self.official_config.credentials().is_some() {
                mirrors.push(MirrorEndpoint::official_pending(Some(
                    self.official_config.clone(),
                )));
            } else {
                warn!("official mirror enabled but no auth or credentials found; skipping");
            }
        }

        let trimmed_custom = self.custom_mirror.value.trim();
        if !trimmed_custom.is_empty()
            && let Ok(custom_endpoint) = MirrorEndpoint::custom(trimmed_custom)
        {
            mirrors.push(custom_endpoint);
        }

        mirrors
    }

    pub fn build_request(&self) -> Result<DownloadRequest, String> {
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
            verify_zip_eocd: self.verify_zip_eocd,
            max_retries: self.default_retries,
        };

        Ok(DownloadRequest {
            collection_input: collection_input.to_string(),
            config,
            skip_existing: self.skip_existing,
            auto_overwrite: self.auto_overwrite,
        })
    }

    pub fn build_mirrors(&self) -> Vec<MirrorEndpoint> {
        self.build_mirror_list()
    }

    pub fn resolved_threads(&self) -> u8 {
        if self.threads.value.trim().is_empty() {
            self.default_threads
        } else {
            parse_thread_count(&self.threads.value).unwrap_or(self.default_threads)
        }
    }

    pub fn resolved_retries(&self) -> u8 {
        self.default_retries
    }
}

fn parse_thread_count(value: &str) -> Result<u8, String> {
    u8::from_str(value.trim()).map_err(|_| "Thread count must be between 1 and 50".to_string())
}
