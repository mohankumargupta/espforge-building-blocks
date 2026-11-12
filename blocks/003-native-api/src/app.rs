use crate::{service, Context};
use embassy_executor::Spawner;

/// Entities (`onboard_led` above) would be exposed by generated code once the
/// network is up, so the app only waits for the association.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    logger.info("native-api: waiting for Wi-Fi");
    service!(ctx, home).wait_connected().await;
    logger.info("native-api: entities are exposed through the API");
}

pub async fn forever(ctx: &'static Context) {
    ctx.delay.delay_ms(1000).await;
}
