use crate::{service, Context};
use embassy_executor::Spawner;

/// Runs once at startup: wait for association and DHCP, then report the
/// address. OTA, once it exists, would be a service started by the runtime and
/// would add no code here.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    let station = service!(ctx, home);

    logger.info("wifi-ota: connecting...");
    station.wait_connected().await;

    match station.stack().config_v4() {
        Some(config) => logger.info(format_args!("Connected, IP address: {}", config.address)),
        None => logger.info("Connected, but no IPv4 address"),
    }
}

pub async fn forever(ctx: &'static Context) {
    ctx.delay.delay_ms(1000).await;
}
