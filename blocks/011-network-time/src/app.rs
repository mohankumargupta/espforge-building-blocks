use crate::{service, Context};
use core::sync::atomic::{AtomicU32, Ordering};
use embassy_executor::Spawner;

/// `on_time: seconds: 0, minutes: 0, hours: 8` — in UTC, once per day.
const TRIGGER_HOUR: u64 = 8;
const CHECK_INTERVAL_MS: u32 = 60_000;

/// The Unix day the trigger last fired. `u32::MAX` means "never", and a Unix
/// day fits in `u32` for the life of the hardware.
static LAST_FIRED_DAY: AtomicU32 = AtomicU32::new(u32::MAX);

pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    logger.info("network-time: connecting...");
    service!(ctx, home).wait_connected().await;

    // One SNTP query anchors the clock; `now()` below is local arithmetic.
    match service!(ctx, clock).sync().await {
        Ok(time) => logger.info(format_args!(
            "network-time: synced at unix {}",
            time.unix_seconds
        )),
        Err(error) => logger.warn(format_args!("network-time: NTP sync failed: {error:?}")),
    }
}

pub async fn forever(ctx: &'static Context) {
    if let Ok(now) = service!(ctx, clock).now() {
        let day = (now.seconds / 86_400) as u32;
        let seconds_in_day = now.seconds % 86_400;
        let hour = seconds_in_day / 3_600;
        let minute = (seconds_in_day % 3_600) / 60;

        // `on_time` fires on the matching hour and minute; the day guard keeps
        // the minute-long check from firing more than once.
        if hour == TRIGGER_HOUR
            && minute == 0
            && LAST_FIRED_DAY.swap(day, Ordering::Relaxed) != day
        {
            device!(ctx, daily_switch).on();
            ctx.logger.info("network-time: daily switch on");
        }
    }

    ctx.delay.delay_ms(CHECK_INTERVAL_MS).await;
}
