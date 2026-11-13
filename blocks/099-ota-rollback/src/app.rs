use crate::{service, Context};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use embassy_executor::Spawner;

/// How long to keep proving the firmware before declaring it good.
const HEALTH_WINDOW_MS: u32 = 30_000;
/// After that, a re-check every minute catches a regression that appeared later.
const RECHECK_MS: u32 = 60_000;

static CONFIRMED: AtomicBool = AtomicBool::new(false);
static UPTIME_MS: AtomicU32 = AtomicU32::new(0);

pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let led = device!(ctx, status_led);

    // Boot: pending verification. The bootloader rolls back if this firmware
    // resets before `mark_app_valid`.
    ctx.logger
        .info("ota-rollback: booted in pending-verify state");

    // Health check 1: the network, which is what future updates arrive over.
    service!(ctx, home).wait_connected().await;
    ctx.logger.info("ota-rollback: Wi-Fi associated");

    // Health check 2: stay up long enough to prove there is no boot loop.
    led.on();
    ctx.delay.delay_ms(HEALTH_WINDOW_MS).await;

    match ctx.ota.mark_app_valid() {
        Ok(()) => {
            CONFIRMED.store(true, Ordering::Relaxed);
            led.off();
            ctx.logger.info("ota-rollback: image confirmed, rollback cancelled");
        }
        Err(error) => ctx.logger.warn(format_args!(
            "ota-rollback: could not confirm the image: {error}"
        )),
    }
}

pub async fn forever(ctx: &'static Context) {
    let elapsed = UPTIME_MS.fetch_add(RECHECK_MS, Ordering::Relaxed) + RECHECK_MS;

    // A firmware that was confirmed and then loses its link is a problem, but
    // no longer a rollback: the slot is already permanent.
    if CONFIRMED.load(Ordering::Relaxed) && service!(ctx, home).stack().config_v4().is_none() {
        ctx.logger
            .warn(format_args!("ota-rollback: link lost at {elapsed} ms uptime"));
        device!(ctx, status_led).on();
    }

    ctx.delay.delay_ms(RECHECK_MS).await;
}
