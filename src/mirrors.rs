pub use osu_downloader::{Mirror, MirrorKind};

use crate::utils::{AppError, Result};

pub fn validate_mirror_template(template: &str) -> Result<()> {
    Mirror::custom(template)
        .map(drop)
        .map_err(|e| AppError::config_dynamic(e.to_string()))
}

pub fn from_kind(kind: MirrorKind) -> Option<Mirror> {
    match kind {
        MirrorKind::Nerinyan => Some(Mirror::nerinyan()),
        MirrorKind::OsuDirect => Some(Mirror::osu_direct()),
        MirrorKind::Sayobot => Some(Mirror::sayobot()),
        MirrorKind::Nekoha => Some(Mirror::nekoha()),
        MirrorKind::Custom => None,
    }
}

#[cfg(test)]
#[path = "../tests/unit/mirrors.rs"]
mod tests;
