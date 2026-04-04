// Re-export core types from osu-downloader
pub use osu_downloader::{CatboyRegion, MirrorKind, MirrorPool};

use crate::utils::{AppError, Result};
use osu_downloader::Mirror;

// MirrorEndpoint wraps osu-downloader's Mirror but exposes public fields for compatibility
#[derive(Debug, Clone)]
pub struct MirrorEndpoint {
    pub kind: MirrorKind,
    pub template: Box<str>,
}

impl MirrorEndpoint {
    pub fn builtin(kind: MirrorKind, no_video: bool) -> Option<Self> {
        Mirror::builtin(kind, no_video).map(|mirror| Self {
            kind: mirror.kind(),
            template: mirror.url_for(0).replace("0", "{id}").into_boxed_str(),
        })
    }

    pub fn custom(template: &str) -> Result<Self> {
        validate_template(template)?;
        Ok(Self {
            kind: MirrorKind::Custom,
            template: template.into(),
        })
    }

    #[inline]
    pub fn url_for(&self, beatmapset_id: u32) -> String {
        self.template.replace("{id}", &beatmapset_id.to_string())
    }

    pub fn display_name(&self) -> &'static str {
        self.kind.label()
    }

    // Convert to osu-downloader's Mirror type
    pub fn to_mirror(&self) -> Mirror {
        if self.kind == MirrorKind::Custom {
            Mirror::custom(self.template.as_ref()).expect("template already validated")
        } else {
            Mirror::builtin(
                self.kind,
                self.template.contains("?nv=1") || self.template.ends_with('n'),
            )
            .expect("builtin mirror should have template")
        }
    }
}

impl From<Mirror> for MirrorEndpoint {
    fn from(mirror: Mirror) -> Self {
        Self {
            kind: mirror.kind(),
            template: mirror.url_for(0).replace("0", "{id}").into_boxed_str(),
        }
    }
}

pub fn validate_template(template: &str) -> Result<()> {
    if !template.contains("{id}") {
        return Err(AppError::config("Mirror URL must contain {id} placeholder"));
    }

    if !template.starts_with("http://") && !template.starts_with("https://") {
        return Err(AppError::config(
            "Mirror URL must start with http:// or https://",
        ));
    }

    Ok(())
}
