//! Happy-path fixture for the project-local `<ads1x15>` driver.
//!
//! The driver itself is the generated `adc` device: `Devices::new` builds
//! `ads1x15_driver::Ads1x15`, which reads the configuration register to prove
//! the part answers. This app only adds machine-checkable output:
//!
//! - `TEST: ads1x15 boot`
//! - `TEST: adc a0=0.512 a1=1.024 a2=1.500 a3=0.128 v` once a second
//! - `TEST: adc a0-a1=-0.512 v`, the first differential pair
//!
//! Pins match `drivers/ads1x15/board.json` (SCL 6, SDA 5) and the address is
//! 0x48, the part's ADDR-to-ground default.

use crate::Context;
use ads1x15_driver::{Gain, Input};

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

/// The power-on range, ±2.048 V, for every channel.
const GAIN: Gain = Gain::TwoVolt048;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: ads1x15 boot");
}

pub fn forever(ctx: &mut Context) {
    let mut volts = [0.0f32; 4];
    for (channel, slot) in volts.iter_mut().enumerate() {
        match device!(ctx, adc).read(Input::SingleEnded(channel as u8), GAIN) {
            Ok(reading) => *slot = reading.volts,
            Err(error) => ctx
                .logger
                .error(format_args!("TEST: adc ain{channel} read failed: {error:?}")),
        }
    }
    ctx.logger.info(format_args!(
        "TEST: adc a0={:.3} a1={:.3} a2={:.3} a3={:.3} v",
        volts[0], volts[1], volts[2], volts[3]
    ));

    let pair = Input::Differential {
        positive: 0,
        negative: 1,
    };
    match device!(ctx, adc).read(pair, GAIN) {
        Ok(reading) => ctx
            .logger
            .info(format_args!("TEST: adc a0-a1={:.3} v", reading.volts)),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: adc a0-a1 read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
