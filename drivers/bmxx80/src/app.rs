//! Happy-path fixture for the project-local `<bmxx80>` driver.
//!
//! The driver itself is the generated `weather` device: `Devices::new` builds
//! `bmxx80_driver::Bme280`, which checks the chip id, soft-resets the part,
//! reads its calibration and starts normal-mode measurements. This app then
//! reads the compensated values once a second and logs them:
//!
//! - `TEST: bmxx80 boot`
//! - `TEST: bmxx80 t=25.00 p=1013.25 h=45.00` once a second
//!
//! Pins match `drivers/bmxx80/board.json` (SCL 6, SDA 5) and the address is
//! 0x76, the part's address with its SDO pin low.

use crate::Context;

/// One measurement a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: bmxx80 boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, weather).read() {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: bmxx80 t={:.2} p={:.2} h={:.2}",
            reading.temperature, reading.pressure, reading.humidity
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: bmxx80 read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
