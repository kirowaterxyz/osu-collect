mod pool;

pub use pool::MirrorPool;

use crate::error::{Error, Result};
use reqwest::header::HeaderMap;
use std::time::Duration;

/// Region for Catboy mirror
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CatboyRegion {
    /// Central region (catboy.best)
    Central,
    /// US region (us.catboy.best)
    Us,
    /// Asia region (sg.catboy.best)
    Asia,
}

impl CatboyRegion {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            CatboyRegion::Central => "Catboy (Central)",
            CatboyRegion::Us => "Catboy (US)",
            CatboyRegion::Asia => "Catboy (Asia)",
        }
    }

    pub(crate) fn base_url(&self) -> &'static str {
        match self {
            CatboyRegion::Central => "https://catboy.best",
            CatboyRegion::Us => "https://us.catboy.best",
            CatboyRegion::Asia => "https://sg.catboy.best",
        }
    }
}

/// Mirror type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MirrorKind {
    /// Nerinyan mirror
    Nerinyan,
    /// Catboy mirror with region
    Catboy(CatboyRegion),
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
            MirrorKind::Catboy(_) | MirrorKind::Custom => None,
        }
    }

    /// Get the display label for this mirror
    #[inline]
    pub fn label(&self) -> &'static str {
        match self {
            MirrorKind::Catboy(region) => region.label(),
            MirrorKind::Custom => "Custom",
            other => other.meta().expect("non-catboy/custom has meta").label,
        }
    }

    pub(crate) fn rate_limit_backoff(&self) -> Duration {
        match self {
            MirrorKind::Catboy(_) => Duration::from_secs(30),
            MirrorKind::Custom => Duration::from_secs(60),
            other => Duration::from_secs(other.meta().expect("non-catboy/custom has meta").backoff_secs),
        }
    }

    pub(crate) fn download_template(&self, no_video: bool) -> Option<String> {
        match self {
            MirrorKind::Catboy(region) => {
                let suffix = if no_video { "n" } else { "" };
                Some(format!("{}/d/{{id}}{}", region.base_url(), suffix))
            }
            MirrorKind::Custom => None,
            other => {
                let meta = other.meta().expect("non-catboy/custom has meta");
                Some(if no_video { meta.template_no_video } else { meta.template }.to_string())
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

    /// Catboy mirror with specified region
    pub fn catboy(region: CatboyRegion) -> Self {
        Self::builtin(MirrorKind::Catboy(region), false).expect("catboy has template")
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
    /// For named constructors without no-video, use [`Mirror::nerinyan`], [`Mirror::catboy`], etc.
    pub fn builtin(kind: MirrorKind, no_video: bool) -> Option<Self> {
        kind.download_template(no_video).map(|template| Self {
            kind,
            template: template.into_boxed_str(),
            headers: None,
        })
    }

    /// Attach HTTP headers to requests sent through this mirror.
    pub fn with_headers(mut self, headers: HeaderMap) -> Self {
        self.headers = Some(headers);
        self
    }

    pub(crate) fn with_no_video(self, no_video: bool) -> Self {
        match Self::builtin(self.kind, no_video) {
            Some(mut mirror) => {
                mirror.headers = self.headers;
                mirror
            }
            None => self,
        }
    }

    /// Get HTTP headers attached to this mirror.
    pub fn headers(&self) -> Option<&HeaderMap> {
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

    /// Generate download URL for a beatmapset
    #[inline]
    pub fn url_for(&self, beatmapset_id: u32) -> String {
        self.template.replace("{id}", &beatmapset_id.to_string())
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mirror_templates() {
        assert_eq!(
            Mirror::nerinyan().url_for(123),
            "https://api.nerinyan.moe/d/123"
        );
        assert_eq!(
            Mirror::catboy(CatboyRegion::Us).url_for(456),
            "https://us.catboy.best/d/456"
        );
        assert_eq!(
            Mirror::osu_direct().url_for(789),
            "https://osu.direct/d/789"
        );
    }

    #[test]
    fn test_custom_mirror() {
        let mirror = Mirror::custom("https://example.com/dl/{id}").unwrap();
        assert_eq!(mirror.url_for(123), "https://example.com/dl/123");
    }

    #[test]
    fn test_invalid_custom_mirror() {
        assert!(Mirror::custom("https://example.com/dl/").is_err());
        assert!(Mirror::custom("ftp://example.com/{id}").is_err());
    }
}
