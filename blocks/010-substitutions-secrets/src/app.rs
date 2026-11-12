use crate::{service, Context};
use embassy_executor::Spawner;

/// Substitutions and secrets are resolved before generation, so they change
/// nothing here: the app is the same Wi-Fi startup as block 002.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    logger.info("substitutions-secrets: connecting...");

    service!(ctx, home).wait_connected().await;
    logger.info("substitutions-secrets: connected");
}

pub async fn forever(ctx: &'static Context) {
    ctx.delay.delay_ms(1000).await;
}
