pub(crate) mod pool;

pub(crate) use pool::MirrorPool;

use crate::error::{Error, Result};
use reqwest::header::HeaderMap;
use std::fmt::Write as _;
use std::time::Duration;

/// Mirror type identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MirrorKind {
    /// Nerinyan mirror.
    Nerinyan,
    /// osu.direct mirror.
    OsuDirect,
    /// Sayobot mirror.
    Sayobot,
    /// Nekoha mirror.
    Nekoha,
    /// Custom mirror with user-provided URL template.
    Custom,
}

struct ProviderMeta {
    label: &'static str,
    #[cfg_attr(test, allow(dead_code))]
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
            MirrorKind::Custom => None,
        }
    }

    /// All built-in mirror kinds, in default registration order. Excludes
    /// [`MirrorKind::Custom`].
    pub const BUILTINS: &'static [MirrorKind] = &[
        MirrorKind::OsuDirect,
        MirrorKind::Nerinyan,
        MirrorKind::Sayobot,
        MirrorKind::Nekoha,
    ];

    /// Display label for this mirror.
    #[inline]
    pub fn label(&self) -> &'static str {
        match self {
            MirrorKind::Custom => "Custom",
            other => other.meta().expect("builtin mirror has meta").label,
        }
    }

    /// Display host for this mirror (e.g. `"osu.direct"`). Returns `"custom"`
    /// for [`MirrorKind::Custom`]. Suitable for UI labels.
    #[inline]
    pub fn host(&self) -> &'static str {
        match self {
            MirrorKind::Nerinyan => "api.nerinyan.moe",
            MirrorKind::OsuDirect => "osu.direct",
            MirrorKind::Sayobot => "dl.sayobot.cn",
            MirrorKind::Nekoha => "mirror.nekoha.moe",
            MirrorKind::Custom => "custom",
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

    fn download_template(&self, no_video: bool) -> Option<&'static str> {
        match self {
            MirrorKind::Custom => None,
            other => {
                let meta = other.meta().expect("builtin mirror has meta");
                Some(if no_video {
                    meta.template_no_video
                } else {
                    meta.template
                })
            }
        }
    }
}

/// URL template split into the parts before and after `{id}`, parsed once at
/// construction so `url_for` can build the URL with a single allocation.
#[derive(Debug, Clone)]
struct SplitTemplate {
    prefix: Box<str>,
    suffix: Box<str>,
}

impl SplitTemplate {
    fn new(template: &str) -> Self {
        // validate_template already guarantees `{id}` is present.
        let (prefix, suffix) = template.split_once("{id}").unwrap_or((template, ""));
        Self {
            prefix: prefix.into(),
            suffix: suffix.into(),
        }
    }
}

/// Mirror endpoint for downloading beatmapsets.
#[derive(Debug, Clone)]
pub struct Mirror {
    pub(crate) kind: MirrorKind,
    pub(crate) template: Box<str>,
    split: SplitTemplate,
    pub(crate) headers: Option<HeaderMap>,
    pub(crate) no_video: bool,
}

impl Mirror {
    /// Custom mirror with a URL template.
    ///
    /// Template must contain `{id}` and start with `http://` or `https://`.
    pub fn custom(template: impl Into<String>) -> Result<Self> {
        let template = template.into();
        validate_template(&template)?;
        let split = SplitTemplate::new(&template);
        Ok(Self {
            kind: MirrorKind::Custom,
            template: template.into_boxed_str(),
            split,
            headers: None,
            no_video: false,
        })
    }

    /// Construct a built-in mirror from its [`MirrorKind`].
    ///
    /// Returns `None` for [`MirrorKind::Custom`] since custom mirrors have no
    /// predefined template — use [`Mirror::custom`] for those.
    pub fn builtin(kind: MirrorKind) -> Option<Self> {
        match kind {
            MirrorKind::Custom => None,
            other => Some(Self::new_builtin(other)),
        }
    }

    /// Nerinyan mirror (<https://api.nerinyan.moe>).
    pub fn nerinyan() -> Self {
        Self::new_builtin(MirrorKind::Nerinyan)
    }

    /// osu.direct mirror.
    pub fn osu_direct() -> Self {
        Self::new_builtin(MirrorKind::OsuDirect)
    }

    /// Sayobot mirror.
    pub fn sayobot() -> Self {
        Self::new_builtin(MirrorKind::Sayobot)
    }

    /// Nekoha mirror.
    pub fn nekoha() -> Self {
        Self::new_builtin(MirrorKind::Nekoha)
    }

    /// Every built-in mirror, in the library's default preference order.
    pub fn builtins() -> Vec<Mirror> {
        vec![
            Mirror::nerinyan(),
            Mirror::osu_direct(),
            Mirror::sayobot(),
            Mirror::nekoha(),
        ]
    }

    fn new_builtin(kind: MirrorKind) -> Self {
        let template = kind
            .download_template(false)
            .expect("builtin mirror has template");
        let split = SplitTemplate::new(template);
        Self {
            kind,
            template: template.into(),
            split,
            headers: None,
            no_video: false,
        }
    }

    /// Attach HTTP headers to requests sent through this mirror.
    #[must_use]
    pub fn with_headers(mut self, headers: HeaderMap) -> Self {
        self.headers = Some(headers);
        self
    }

    /// Strip video from the served archive where the mirror supports it.
    /// No-op for custom mirrors and for mirrors without a no-video variant.
    #[must_use]
    pub fn no_video(mut self) -> Self {
        self.no_video = true;
        if let Some(template) = self.kind.download_template(true) {
            self.split = SplitTemplate::new(template);
            self.template = template.into();
        }
        self
    }

    /// Mirror kind.
    pub fn kind(&self) -> MirrorKind {
        self.kind
    }

    /// URL template used by this mirror. Contains `{id}` for substitution.
    pub fn template(&self) -> &str {
        &self.template
    }

    pub(crate) fn headers(&self) -> Option<&HeaderMap> {
        self.headers.as_ref()
    }

    #[inline]
    pub(crate) fn url_for(&self, beatmapset_id: u32) -> String {
        let id_digits = beatmapset_id.checked_ilog10().unwrap_or(0) as usize + 1;
        let cap = self.split.prefix.len() + id_digits + self.split.suffix.len();
        let mut url = String::with_capacity(cap);
        url.push_str(&self.split.prefix);
        write!(url, "{beatmapset_id}").expect("write to String is infallible");
        url.push_str(&self.split.suffix);
        url
    }

    #[cfg(test)]
    pub(crate) fn with_kind_and_template(kind: MirrorKind, template: impl Into<String>) -> Self {
        let template = template.into();
        let split = SplitTemplate::new(&template);
        Self {
            kind,
            template: template.into_boxed_str(),
            split,
            headers: None,
            no_video: false,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/mirrors.rs"]
mod tests;

fn validate_template(template: &str) -> Result<()> {
    match template.matches("{id}").count() {
        0 => return Err(Error::mirror("Mirror URL must contain {id} placeholder")),
        1 => {}
        _ => {
            return Err(Error::mirror(
                "Mirror URL must contain exactly one {id} placeholder",
            ));
        }
    }

    if !template.starts_with("http://") && !template.starts_with("https://") {
        return Err(Error::mirror(
            "Mirror URL must start with http:// or https://",
        ));
    }

    Ok(())
}
