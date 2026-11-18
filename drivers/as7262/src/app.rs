//! Happy-path fixture for the project-local `<as7262>` driver.
//!
//! The driver itself is the generated `spectrum` device: `Devices::new` builds
//! `as7262_driver::As7262`, which soft-resets the part and sets its
//! integration time. This app then takes one one-shot measurement a second and
//! logs the six raw channels:
//!
//! - `TEST: as7262 boot`
//! - `TEST: as7262 v=1200 b=2400 g=3200 y=3100 o=2800 r=2200` once a second
//!
//! Pins match `drivers/as7262/board.json` (SCL 6, SDA 5) and the address is
//! 0x49, the part's only I2C address.

use crate::Context;

/// One measurement a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: as7262 boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, spectrum).read() {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: as7262 v={} b={} g={} y={} o={} r={}",
            reading.violet, reading.blue, reading.green, reading.yellow, reading.orange, reading.red
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: as7262 read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
