//! Happy-path fixture for the `<pca9685>` driver.
//!
//! The driver itself is the generated `pwm` device: `Devices::new` builds
//! `pwm_pca9685::Pca9685`. `setup` programs the prescaler for 50 Hz and wakes
//! the part; this app then holds two channels at fixed duty cycles and logs
//! them once a second:
//!
//! - `TEST: pca9685 boot`
//! - `TEST: pca9685 ch0=25 ch1=50` once a second
//!
//! Pins match `drivers/pca9685/board.json` (SCL 6, SDA 5) and the address is
//! 0x40, the part's address with every address pin low.

use crate::Context;
use pwm_pca9685::Channel;

/// One refresh a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

/// Prescaler for a 50 Hz frame rate: 25 MHz / (4096 * 50 Hz) - 1.
const PRESCALE_50HZ: u8 = 121;

/// Channel 0: a quarter of the cycle.
const CH0_ON: u16 = 0;
const CH0_OFF: u16 = 1024;

/// Channel 1: half of the cycle.
const CH1_ON: u16 = 0;
const CH1_OFF: u16 = 2048;

pub fn setup(ctx: &mut Context) {
    let pwm = device!(ctx, pwm);
    if let Err(error) = pwm.set_prescale(PRESCALE_50HZ) {
        ctx.logger
            .error(format_args!("TEST: pca9685 prescale failed: {error:?}"));
        return;
    }
    if let Err(error) = pwm.enable() {
        ctx.logger
            .error(format_args!("TEST: pca9685 enable failed: {error:?}"));
        return;
    }
    ctx.logger.info("TEST: pca9685 boot");
}

pub fn forever(ctx: &mut Context) {
    let pwm = device!(ctx, pwm);
    if let Err(error) = pwm.set_channel_on_off(Channel::C0, CH0_ON, CH0_OFF) {
        ctx.logger
            .error(format_args!("TEST: pca9685 ch0 failed: {error:?}"));
        ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
        return;
    }
    if let Err(error) = pwm.set_channel_on_off(Channel::C1, CH1_ON, CH1_OFF) {
        ctx.logger
            .error(format_args!("TEST: pca9685 ch1 failed: {error:?}"));
        ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
        return;
    }
    ctx.logger
        .info(format_args!("TEST: pca9685 ch0=25 ch1=50"));
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
