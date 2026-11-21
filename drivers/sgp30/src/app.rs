//! Happy-path fixture for the `<sgp30>` driver.
//!
//! The driver itself is the generated `air` device: `Devices::new` builds
//! `sgp30::Sgp30`. The crate needs `init()` to start air-quality measurement,
//! so `setup` runs it; this app then reads eCO2 and TVOC once a second:
//!
//! - `TEST: sgp30 boot`
//! - `TEST: sgp30 eco2=400 ppm tvoc=0 ppb` once a second
//!
//! Pins match `drivers/sgp30/board.json` (SCL 6, SDA 5) and the address is
//! 0x58, the part's only address.

use crate::Context;

/// One measurement a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    match device!(ctx, air).init() {
        Ok(()) => ctx.logger.info("TEST: sgp30 boot"),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: sgp30 init failed: {error:?}")),
    }
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, air).measure() {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: sgp30 eco2={} ppm tvoc={} ppb",
            reading.co2eq_ppm, reading.tvoc_ppb
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: sgp30 read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
