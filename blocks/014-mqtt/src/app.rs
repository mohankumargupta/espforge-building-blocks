use crate::{service, Context};
use embassy_executor::Spawner;

/// The intended shape: `<mqtt>` is a proposal. Its connection task would
/// publish `onboard_led` state and subscribe to its command topic; the app has
/// nothing to do until there is a trigger/action vocabulary (block 009).
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    logger.info("mqtt: waiting for Wi-Fi");
    service!(ctx, home).wait_connected().await;

    logger.info("mqtt: connecting to the broker");
    service!(ctx, broker).wait_connected().await;
    logger.info("mqtt: publishing to test-node/#");
}

pub async fn forever(ctx: &'static Context) {
    ctx.delay.delay_ms(1000).await;
}
