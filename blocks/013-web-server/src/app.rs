use crate::{service, Context};
use embassy_executor::Spawner;

/// The REST routes are generated from `# devices`; the app only waits for the
/// network. `/switch/onboard_led/turn_on` is the intended endpoint.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    logger.info("web-server: waiting for Wi-Fi");
    service!(ctx, home).wait_connected().await;
    logger.info("web-server: listening on port 80");
}

pub async fn forever(ctx: &'static Context) {
    ctx.delay.delay_ms(1000).await;
}
