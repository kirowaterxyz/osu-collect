use osu_collect::app::run_app;
use osu_collect::auth;
use osu_collect::auto_update::spawn_background_update;
use osu_collect::config::{ConfigService, LogLevel, OfficialConfig};
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

    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("login") => return cmd_login().await,
        Some("logout") => return cmd_logout(),
        _ => {}
    }

    let config_service = ConfigService::new();
    let config = config_service.load_or_default();

    let realm_debug =
        config.logging.enabled && matches!(config.logging.level, LogLevel::Debug | LogLevel::Trace);
    set_realm_debug_logging(realm_debug);
    let _logging_guard = utils::init_logging(&config.logging)?;
    spawn_background_update();
    run_app(config, None).await
}

async fn cmd_login() -> Result<(), Box<dyn std::error::Error>> {
    let config = ConfigService::new().load_or_default();
    let env_credentials = OfficialConfig {
        client_id: std::env::var("OSU_CLIENT_ID").ok(),
        client_secret: std::env::var("OSU_CLIENT_SECRET").ok(),
    };
    let (client_id, client_secret) = env_credentials
        .credentials()
        .or_else(|| config.official.credentials())
        .ok_or("set OSU_CLIENT_ID and OSU_CLIENT_SECRET or official credentials in config")?;

    let client = reqwest::Client::new();
    auth::run_login_flow(&client, client_id, client_secret).await?;
    println!("login successful — tokens saved");
    Ok(())
}

fn cmd_logout() -> Result<(), Box<dyn std::error::Error>> {
    auth::delete()?;
    println!("logged out");
    Ok(())
}
