//! Happy-path fixture for the project-local `<aht20>` driver.
//!
//! The driver itself is the generated `climate` device: `Devices::new` builds
//! `aht20_driver::Aht20`, which reads the status byte and initializes the part
//! only if it says it is not calibrated. This app only adds machine-checkable
//! output:
//!
//! - `TEST: aht20 boot`
//! - `TEST: climate t=22.50 c rh=45.00 %` once a second
//!
//! Pins match `drivers/aht20/board.json` (SCL 6, SDA 5) and the address is
//! 0x38, the part's only I2C address.

use crate::Context;

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: aht20 boot");
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
