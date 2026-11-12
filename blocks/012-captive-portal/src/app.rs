use crate::{service, Context};
use embassy_executor::Spawner;

/// The intended shape, not a compiling app: `<wifi_ap>` and
/// `<captive_portal>` do not exist. The station is tried first; if it never
/// associates, the fallback hotspot serves the provisioning page, and setup
/// continues once a client has written credentials.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    logger.info("captive-portal: connecting...");

    if service!(ctx, home).try_connect().await.is_err() {
        logger.warn("captive-portal: no network, starting fallback AP");
        service!(ctx, fallback).start().await;
        service!(ctx, portal).wait_for_credentials().await;

        logger.info("captive-portal: credentials received, reconnecting");
        service!(ctx, home).reconnect().await;
    }

    logger.info("captive-portal: online");
}

pub async fn forever(ctx: &'static Context) {
    ctx.delay.delay_ms(1000).await;
}
