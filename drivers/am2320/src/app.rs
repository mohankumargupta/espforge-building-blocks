//! Happy-path fixture for the project-local `<am2320>` driver.
//!
//! The driver itself is the generated `climate` device: `Devices::new` builds
//! `am2320_driver::Am2320` over the I2C bus. This app only adds
//! machine-checkable output:
//!
//! - `TEST: am2320 boot`
//! - `TEST: climate t=22.50 c rh=45.00 %` once a second
//!
//! Pins match `drivers/am2320/board.json` (SCL 6, SDA 5) and the address is
//! 0x5C, the part's only I2C address.

use crate::Context;

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: am2320 boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, climate).read() {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: climate t={:.2} c rh={:.2} %",
            reading.temperature_c, reading.humidity_percent
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: climate read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
