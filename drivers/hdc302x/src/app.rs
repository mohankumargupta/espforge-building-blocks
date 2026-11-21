//! Happy-path fixture for the `<hdc302x>` driver.
//!
//! The driver itself is the generated `climate` device: `Devices::new` builds
//! `hdc302x::Hdc302x`. This app then triggers one on-demand measurement a
//! second and logs the converted values:
//!
//! - `TEST: hdc302x boot`
//! - `TEST: hdc302x t=22.50 rh=45.00` once a second
//!
//! Pins match `drivers/hdc302x/board.json` (SCL 6, SDA 5) and the address is
//! 0x44, the part's address with both address pins low.

use crate::Context;
use hdc302x::LowPowerMode;

/// One measurement a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: hdc302x boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, climate).one_shot(LowPowerMode::LPM0) {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: hdc302x t={:.2} rh={:.2}",
            reading.centigrade().unwrap_or(f32::NAN),
            reading.humidity_percent().unwrap_or(f32::NAN)
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: hdc302x read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
