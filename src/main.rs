use osu_collect::app::run_app;
use osu_collect::auto_update::spawn_background_update;
use osu_collect::config::{ConfigService, LogLevel};
use osu_collect::realm_bridge::ffi::set_realm_debug_logging;
use osu_collect::utils;
#[cfg(windows)]
use osu_collect::windows_init;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    windows_init::relaunch_in_powershell_if_needed();
    #[cfg(windows)]
    windows_init::enable_ansi_support();

    let config_service = ConfigService::new();
    let config = config_service.load_or_default();

    let realm_debug =
        config.logging.enabled && matches!(config.logging.level, LogLevel::Debug | LogLevel::Trace);
    set_realm_debug_logging(realm_debug);
    let _logging_guard = utils::init_logging(&config.logging)?;
    spawn_background_update();
    run_app(config, None).await
}
