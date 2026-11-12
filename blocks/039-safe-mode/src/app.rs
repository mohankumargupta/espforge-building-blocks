use crate::{service, Context};
use embassy_executor::Spawner;

/// The intended shape: `<safe_mode>` counts boots in RTC memory and decides,
/// before the app runs, whether to enter the recovery path. It is a proposal.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let guard = service!(ctx, guard);

    ctx.logger.info(format_args!(
        "safe-mode: boot attempt {} of {}",
        guard.boot_attempts(),
        10
    ));

    if guard.in_safe_mode() {
        ctx.logger
            .warn("safe-mode: fallback AP and OTA only (proposed)");
    } else {
        ctx.logger.info("safe-mode: normal boot");
    }
}

pub async fn forever(ctx: &'static Context) {
    ctx.delay.delay_ms(60_000).await;
}
