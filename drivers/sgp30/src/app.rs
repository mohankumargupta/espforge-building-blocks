//! Happy-path fixture for the project-local `<sgp30>` driver.
//!
//! The driver itself is the generated `air` device: `Devices::new` builds
//! `sgp30_driver::Sgp30`, which initialises air quality measurement. This app
//! then reads eCO2 and TVOC once a second and logs them:
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
    ctx.logger.info("TEST: sgp30 boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, air).read() {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: sgp30 eco2={} ppm tvoc={} ppb",
            reading.eco2_ppm, reading.tvoc_ppb
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: sgp30 read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
