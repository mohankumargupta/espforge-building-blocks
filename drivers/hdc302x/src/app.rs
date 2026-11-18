//! Happy-path fixture for the project-local `<hdc302x>` driver.
//!
//! The driver itself is the generated `climate` device: `Devices::new` builds
//! `hdc302x_driver::Hdc302x`, which soft-resets the part. This app then
//! triggers one on-demand measurement a second and logs the converted values:
//!
//! - `TEST: hdc302x boot`
//! - `TEST: hdc302x t=22.50 rh=45.00` once a second
//!
//! Pins match `drivers/hdc302x/board.json` (SCL 6, SDA 5) and the address is
//! 0x44, the part's address with both address pins low.

use crate::Context;

/// One measurement a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: hdc302x boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, climate).read() {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: hdc302x t={:.2} rh={:.2}",
            reading.temperature_c, reading.humidity_percent
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: hdc302x read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
