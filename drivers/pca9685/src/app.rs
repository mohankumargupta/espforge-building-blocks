//! Happy-path fixture for the project-local `<pca9685>` driver.
//!
//! The driver itself is the generated `pwm` device: `Devices::new` builds
//! `pca9685_driver::Pca9685`, which sleeps the part, programs the prescaler
//! for 50 Hz and wakes it with auto-increment. This app then holds two
//! channels at fixed duty cycles and logs them once a second:
//!
//! - `TEST: pca9685 boot`
//! - `TEST: pca9685 ch0=25 ch1=50` once a second
//!
//! Pins match `drivers/pca9685/board.json` (SCL 6, SDA 5) and the address is
//! 0x40, the part's address with every address pin low.

use crate::Context;

/// One refresh a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

/// Channel 0: a quarter of the cycle.
const CH0_PERCENT: f32 = 25.0;
/// Channel 1: half of it.
const CH1_PERCENT: f32 = 50.0;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: pca9685 boot");
}

pub fn forever(ctx: &mut Context) {
    for (channel, percent) in [(0u8, CH0_PERCENT), (1, CH1_PERCENT)] {
        if let Err(error) = device!(ctx, pwm).set_duty(channel, percent) {
            ctx.logger
                .error(format_args!("TEST: pca9685 ch{channel} failed: {error:?}"));
            ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
            return;
        }
    }
    ctx.logger.info(format_args!(
        "TEST: pca9685 ch0={CH0_PERCENT:.0} ch1={CH1_PERCENT:.0}"
    ));
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
