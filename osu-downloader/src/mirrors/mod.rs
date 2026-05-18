pub(crate) mod pool;

pub(crate) use pool::MirrorPool;

use crate::error::{Error, Result};
use reqwest::header::HeaderMap;
use std::time::Duration;

/// Mirror type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MirrorKind {
    /// Nerinyan mirror
    Nerinyan,
    /// osu.direct mirror
    OsuDirect,
    /// Sayobot mirror
    Sayobot,
    /// Nekoha mirror
    Nekoha,
    /// Official osu! API v2 mirror (requires OAuth)
    Official,
    /// Custom mirror with user-provided URL template
    Custom,
}

struct ProviderMeta {
    label: &'static str,
    backoff_secs: u64,
    template: &'static str,
    template_no_video: &'static str,
}

impl MirrorKind {
    fn meta(&self) -> Option<ProviderMeta> {
        match self {
            MirrorKind::Nerinyan => Some(ProviderMeta {
                label: "Nerinyan",
                backoff_secs: 45,
                template: "https://api.nerinyan.moe/d/{id}",
                template_no_video: "https://api.nerinyan.moe/d/{id}?nv=1",
            }),
            MirrorKind::OsuDirect => Some(ProviderMeta {
                label: "osu.direct",
                backoff_secs: 75,
                template: "https://osu.direct/d/{id}",
                template_no_video: "https://osu.direct/d/{id}n",
            }),
            MirrorKind::Sayobot => Some(ProviderMeta {
                label: "Sayobot",
                backoff_secs: 60,
                template: "https://dl.sayobot.cn/beatmaps/download/full/{id}",
                template_no_video: "https://dl.sayobot.cn/beatmaps/download/novideo/{id}",
            }),
            MirrorKind::Nekoha => Some(ProviderMeta {
                label: "Nekoha",
                backoff_secs: 45,
                template: "https://mirror.nekoha.moe/api4/download/{id}",
                template_no_video: "https://mirror.nekoha.moe/api4/download/{id}",
            }),
            MirrorKind::Official => Some(ProviderMeta {
                label: "osu! API",
                backoff_secs: 60,
                template: "https://osu.ppy.sh/api/v2/beatmapsets/{id}/download",
                template_no_video: "https://osu.ppy.sh/api/v2/beatmapsets/{id}/download",
            }),
            MirrorKind::Custom => None,
        }
    }

    /// Get the display label for this mirror
    #[inline]
    pub fn label(&self) -> &'static str {
        match self {
            MirrorKind::Custom => "Custom",
            other => other.meta().expect("builtin mirror has meta").label,
        }
    }

    pub(crate) fn rate_limit_backoff(&self) -> Duration {
        #[cfg(test)]
        {
            return Duration::from_millis(10);
        }

        #[cfg(not(test))]
        match self {
            MirrorKind::Custom => Duration::from_secs(60),
            other => {
                Duration::from_secs(other.meta().expect("builtin mirror has meta").backoff_secs)
            }
        }
    }

    pub(crate) fn download_template(&self, no_video: bool) -> Option<String> {
        match self {
            MirrorKind::Custom => None,
            other => {
                let meta = other.meta().expect("builtin mirror has meta");
                Some(
                    if no_video {
                        meta.template_no_video
                    } else {
                        meta.template
                    }
                    .to_string(),
                )
            }
        }
    }
}

/// Mirror endpoint for downloading beatmapsets
#[derive(Debug, Clone)]
pub struct Mirror {
    pub(crate) kind: MirrorKind,
    pub(crate) template: Box<str>,
    pub(crate) headers: Option<HeaderMap>,
}

impl Mirror {
    /// Create a custom mirror with a URL template
    ///
    /// Template must contain `{id}` placeholder and start with `http://` or `https://`
    pub fn custom(template: impl Into<String>) -> Result<Self> {
        let template = template.into();
        validate_template(&template)?;
        Ok(Self {
            kind: MirrorKind::Custom,
            template: template.into_boxed_str(),
            headers: None,
        })
    }

    /// Nerinyan mirror (<https://api.nerinyan.moe>)
    pub fn nerinyan() -> Self {
        Self::builtin(MirrorKind::Nerinyan, false).expect("nerinyan has template")
    }

    /// osu.direct mirror
    pub fn osu_direct() -> Self {
        Self::builtin(MirrorKind::OsuDirect, false).expect("osu.direct has template")
    }

    /// Sayobot mirror
    pub fn sayobot() -> Self {
        Self::builtin(MirrorKind::Sayobot, false).expect("sayobot has template")
    }

    /// Nekoha mirror (<https://nekoha.cc>)
    pub fn nekoha() -> Self {
        Self::builtin(MirrorKind::Nekoha, false).expect("nekoha has template")
    }

    /// Create a mirror from a [`MirrorKind`] with optional no-video support.
    ///
    /// Returns `None` for [`MirrorKind::Custom`] since custom mirrors require a user-provided template.
    /// For named constructors without no-video, use [`Mirror::nerinyan`], [`Mirror::osu_direct`], etc.
    pub fn builtin(kind: MirrorKind, no_video: bool) -> Option<Self> {
        kind.download_template(no_video).map(|template| Self {
            kind,
            template: template.into_boxed_str(),
            headers: None,
        })
    }

    /// Attach HTTP headers to requests sent through this mirror.
    pub fn set_headers(mut self, headers: HeaderMap) -> Self {
        self.headers = Some(headers);
        self
    }

    pub(crate) fn video(self, no_video: bool) -> Self {
        match Self::builtin(self.kind, no_video) {
            Some(mut mirror) => {
                mirror.headers = self.headers;
                mirror
            }
            None => self,
        }
    }

    pub(crate) fn headers(&self) -> Option<&HeaderMap> {
        self.headers.as_ref()
    }

    /// Get the mirror kind
    pub fn kind(&self) -> MirrorKind {
        self.kind
    }

    /// Get the display name for this mirror
    pub fn display_name(&self) -> &'static str {
        self.kind.label()
    }

    #[inline]
    pub(crate) fn url_for(&self, beatmapset_id: u32) -> String {
        self.template.replace("{id}", &beatmapset_id.to_string())
    }

    #[cfg(test)]
    pub(crate) fn with_kind_and_template(kind: MirrorKind, template: impl Into<String>) -> Self {
        Self {
            kind,
            template: template.into().into_boxed_str(),
            headers: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn url_for_id(&self, beatmapset_id: u32) -> String {
        self.url_for(beatmapset_id)
    }
}

#[cfg(test)]
#[path = "../../tests/mirrors.rs"]
mod tests;

fn validate_template(template: &str) -> Result<()> {
    if !template.contains("{id}") {
        return Err(Error::invalid_mirror(
            "Mirror URL must contain {id} placeholder",
        ));
    }

    if !template.starts_with("http://") && !template.starts_with("https://") {
        return Err(Error::invalid_mirror(
            "Mirror URL must start with http:// or https://",
        ));
    }

    Ok(())
}
