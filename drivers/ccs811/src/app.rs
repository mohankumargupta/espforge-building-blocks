//! Happy-path fixture for the project-local `<ccs811>` driver.
//!
//! The driver itself is the generated `air` device: `Devices::new` builds
//! `ccs811_driver::Ccs811`, which checks the hardware id, starts the
//! application firmware if the part is in boot mode, and asks for one
//! measurement a second. This app then reads eCO2 and TVOC once a second:
//!
//! - `TEST: ccs811 boot`
//! - `TEST: ccs811 eco2=400 tvoc=8` once a second
//!
//! Pins match `drivers/ccs811/board.json` (SCL 6, SDA 5) and the address is
//! 0x5A, the part's address with its ADDR pin low.

use crate::Context;

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: ccs811 boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, air).read() {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: ccs811 eco2={} tvoc={}",
            reading.eco2_ppm, reading.tvoc_ppb
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: ccs811 read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
