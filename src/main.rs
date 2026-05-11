use osu_collect::app::run_app;
use osu_collect::auth;
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
    let client_id = std::env::var("OSU_CLIENT_ID").map_err(|_| {
        "OSU_CLIENT_ID env var not set; create an OAuth app at https://osu.ppy.sh/home/account/edit#oauth"
    })?;
    let client_secret =
        std::env::var("OSU_CLIENT_SECRET").map_err(|_| "OSU_CLIENT_SECRET env var not set")?;

    let client = reqwest::Client::new();
    auth::run_login_flow(&client, &client_id, &client_secret).await?;
    println!("login successful — tokens saved");
    Ok(())
}

fn cmd_logout() -> Result<(), Box<dyn std::error::Error>> {
    auth::delete()?;
    println!("logged out");
    Ok(())
}
