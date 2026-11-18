//! 001 - Wi-Fi basics: associate with the access point and report the address
//! DHCP gave us.
//!
//! `station.wait_connected()` does both halves of the wait: the link comes up
//! when the station associates, and the configuration comes up when the DHCP
//! lease arrives. Only after that does `stack().config_v4()` hold an address,
//! so reading it any earlier is how this example would report "none".

use crate::Context;
use embassy_executor::Spawner;

/// Runs once at startup, after the generated `main` has started the station.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    let station = service!(ctx, home);

    logger.info("TEST: wifi boot");
    logger.info("wifi-basic: waiting for association and DHCP...");

    station.wait_connected().await;

    // `config_v4()` is `None` until DHCP has an address; a `Some` here is the
    // proof the whole path worked (radio, handshake, lease).
    match station.stack().config_v4() {
        Some(config) => logger.info(format_args!("TEST: ip {}", config.address.address())),
        None => logger.info("TEST: ip none"),
    }
}

pub async fn forever(ctx: &'static Context) {
    // The network tasks the station spawned keep running on their own; the app
    // has nothing to do, so idle instead of exiting.
    ctx.delay.delay_ms(1000).await;
}
