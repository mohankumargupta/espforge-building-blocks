//! 005 - NTP client: set the clock once, then read it locally.
//!
//! `<ntp>` does the DNS lookup and the SNTP exchange through
//! `network="$home"`, so nothing here names an address. `sync()` anchors the
//! server's Unix time to the monotonic clock, and `now()` reads it back with no
//! network I/O. The `TEST:` lines are what the gate in the justfile greps for.

use crate::Context;
use embassy_executor::Spawner;

/// Runs once at startup: wait for Wi-Fi, then anchor the clock.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    let station = service!(ctx, home);
    let clock = service!(ctx, clock);

    logger.info("TEST: ntp boot");
    logger.info("ntp-example: waiting for association and DHCP...");
    station.wait_connected().await;

    match station.stack().config_v4() {
        Some(config) => logger.info(format_args!("TEST: ip {}", config.address.address())),
        None => logger.info("TEST: ip none"),
    }

    // One query is enough: after this the clock advances on its own.
    logger.info("TEST: ntp query");
    match clock.sync().await {
        Ok(time) => logger.info(format_args!(
            "TEST: ntp ok {}.{:06} s since the Unix epoch (RTT {} us)",
            time.unix_seconds, time.microseconds, time.roundtrip_delay_us
        )),
        Err(error) => logger.info(format_args!("TEST: ntp failed: {error:?}")),
    }
}

/// Per-tick loop: read the clock locally, no network involved.
pub async fn forever(ctx: &'static Context) {
    let logger = ctx.logger;
    let clock = service!(ctx, clock);

    match clock.now() {
        Ok(now) => {
            let seconds_in_day = now.seconds % 86400;
            logger.info(format_args!(
                "TEST: clock UTC {:02}:{:02}:{:02}.{:06} ({} s since the Unix epoch)",
                seconds_in_day / 3600,
                (seconds_in_day % 3600) / 60,
                seconds_in_day % 60,
                now.micros,
                now.seconds
            ));
        }
        Err(error) => logger.info(format_args!("TEST: clock not set: {error}")),
    }

    // A local read costs nothing; the figure only paces the log.
    ctx.delay.delay_ms(60000).await;
}
