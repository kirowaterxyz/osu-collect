// Re-export core types from osu-downloader
pub use osu_downloader::{CatboyRegion, MirrorKind, MirrorPool};

use crate::{
    config::OfficialConfig,
    utils::{AppError, Result},
};
use osu_downloader::Mirror;

// MirrorEndpoint wraps osu-downloader's Mirror but exposes public fields for compatibility
#[derive(Debug, Clone)]
pub struct MirrorEndpoint {
    pub kind: MirrorKind,
    pub template: Box<str>,
    pub headers: Option<reqwest::header::HeaderMap>,
    pub official: Option<OfficialConfig>,
}

impl MirrorEndpoint {
    pub fn builtin(kind: MirrorKind, no_video: bool) -> Option<Self> {
        Mirror::builtin(kind, no_video).map(|mirror| Self {
            kind: mirror.kind(),
            template: mirror.url_for(0).replace("0", "{id}").into_boxed_str(),
            headers: None,
            official: None,
        })
    }

    pub fn official(bearer_token: &str) -> Self {
        let mut endpoint = Self::official_pending(None);
        endpoint.set_official_token(bearer_token);
        endpoint
    }

    pub fn official_pending(official: Option<OfficialConfig>) -> Self {
        Self {
            kind: MirrorKind::Official,
            template: "https://osu.ppy.sh/api/v2/beatmapsets/{id}/download".into(),
            headers: None,
            official,
        }
    }

    pub fn set_official_token(&mut self, bearer_token: &str) {
        let mut map = reqwest::header::HeaderMap::new();
        if let Ok(value) = reqwest::header::HeaderValue::from_str(&format!("Bearer {bearer_token}"))
        {
            map.insert(reqwest::header::AUTHORIZATION, value);
        }
        map.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        self.headers = Some(map);
    }

    pub fn custom(template: &str) -> Result<Self> {
        validate_template(template)?;
        Ok(Self {
            kind: MirrorKind::Custom,
            template: template.into(),
            headers: None,
            official: None,
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
        let mirror = match self.kind {
            MirrorKind::Custom => {
                Mirror::custom(self.template.as_ref()).expect("template already validated")
            }
            MirrorKind::Official => {
                Mirror::builtin(MirrorKind::Official, false).expect("official has template")
            }
            _ => Mirror::builtin(
                self.kind,
                self.template.contains("?nv=1") || self.template.ends_with('n'),
            )
            .expect("builtin mirror should have template"),
        };

        if let Some(headers) = self.headers.clone() {
            mirror.with_headers(headers)
        } else {
            mirror
        }
    }
}

impl From<Mirror> for MirrorEndpoint {
    fn from(mirror: Mirror) -> Self {
        Self {
            kind: mirror.kind(),
            template: mirror.url_for(0).replace("0", "{id}").into_boxed_str(),
            headers: mirror.headers().cloned(),
            official: None,
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
