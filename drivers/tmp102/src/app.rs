//! Happy-path fixture for the project-local `<tmp102>` driver.
//!
//! The driver itself is the generated `temp` device: `Devices::new` builds
//! `tmp102_driver::Tmp102`, whose constructor reads the temperature register
//! once to prove the part answers. This app then reads it once a second and
//! logs the decoded temperature:
//!
//! - `TEST: tmp102 boot`
//! - `TEST: tmp102 t=25.00 c` once a second
//!
//! Pins match `drivers/tmp102/board.json` (SCL 6, SDA 5) and the address is
//! 0x48, the part's address with ADD0 tied to ground.

use crate::Context;

/// One measurement a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: tmp102 boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, temp).read() {
        Ok(temperature_c) => ctx
            .logger
            .info(format_args!("TEST: tmp102 t={temperature_c:.2} c")),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: tmp102 read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
