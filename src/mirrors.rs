pub use osu_downloader::{CatboyRegion, Mirror, MirrorKind, MirrorPool};

use crate::utils::{AppError, Result};

pub fn validate_mirror_template(template: &str) -> Result<()> {
    Mirror::custom(template)
        .map(drop)
        .map_err(|e| AppError::config_dynamic(e.to_string()))
}
