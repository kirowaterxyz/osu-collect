mod pool;

pub use pool::MirrorPool;

use crate::error::{Error, Result};
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

impl MirrorKind {
    /// Get the display label for this mirror
    #[inline]
    pub fn label(&self) -> &'static str {
        match self {
            MirrorKind::Nerinyan => "Nerinyan",
            MirrorKind::Catboy(region) => region.label(),
            MirrorKind::OsuDirect => "osu.direct",
            MirrorKind::Sayobot => "Sayobot",
            MirrorKind::Nekoha => "Nekoha",
            MirrorKind::Official => "osu! API",
            MirrorKind::Custom => "Custom",
        }
    }

    pub(crate) fn rate_limit_backoff(&self) -> Duration {
        match self {
            MirrorKind::Nerinyan => Duration::from_secs(45),
            MirrorKind::Catboy(_) => Duration::from_secs(30),
            MirrorKind::OsuDirect => Duration::from_secs(75),
            MirrorKind::Sayobot => Duration::from_secs(60),
            MirrorKind::Nekoha => Duration::from_secs(45),
            MirrorKind::Official => Duration::from_secs(60),
            MirrorKind::Custom => Duration::from_secs(60),
        }
    }

    pub(crate) fn download_template(&self, no_video: bool) -> Option<String> {
        match self {
            MirrorKind::Nerinyan => {
                let template = if no_video {
                    "https://api.nerinyan.moe/d/{id}?nv=1"
                } else {
                    "https://api.nerinyan.moe/d/{id}"
                };
                Some(template.to_string())
            }
            MirrorKind::Catboy(region) => {
                let suffix = if no_video { "n" } else { "" };
                Some(format!("{}/d/{{id}}{}", region.base_url(), suffix))
            }
            MirrorKind::OsuDirect => {
                let suffix = if no_video { "n" } else { "" };
                Some(format!("https://osu.direct/d/{{id}}{}", suffix))
            }
            MirrorKind::Sayobot => {
                let template = if no_video {
                    "https://dl.sayobot.cn/beatmaps/download/novideo/{id}"
                } else {
                    "https://dl.sayobot.cn/beatmaps/download/full/{id}"
                };
                Some(template.to_string())
            }
            MirrorKind::Nekoha => {
                // Nekoha doesn't support no-video downloads
                Some("https://mirror.nekoha.moe/api4/download/{id}".to_string())
            }
            MirrorKind::Official => {
                Some("https://osu.ppy.sh/api/v2/beatmapsets/{id}/download".to_string())
            }
            MirrorKind::Custom => None,
        }
    }
}

/// Mirror endpoint for downloading beatmapsets
#[derive(Debug, Clone)]
pub struct Mirror {
    pub(crate) kind: MirrorKind,
    pub(crate) template: Box<str>,
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
        })
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
