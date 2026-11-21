//! Happy-path fixture for the `<bme280_i2c>` driver.
//!
//! The driver itself is the generated `climate` device: `Devices::new` builds
//! `bme280::i2c::BME280`. The crate needs `init()` (chip id, calibration,
//! oversampling) before it can measure, so `setup` runs it. This app only adds
//! machine-checkable output:
//!
//! - `TEST: bme280_i2c boot`
//! - `TEST: climate t=22.50 c p=1013.25 hpa rh=45.00 %` once a second
//!
//! Pins match `drivers/bme280_i2c/board.json` (SCL 6, SDA 5) and the address
//! is 0x76, the part's SDO-low default. The crate reports pressure in pascals.

use crate::Context;

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    let mut delay = espforge_runtime::Delay::new();
    match device!(ctx, climate).init(&mut delay) {
        Ok(()) => ctx.logger.info("TEST: bme280_i2c boot"),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: climate init failed: {error:?}")),
    }
}

pub fn forever(ctx: &mut Context) {
    let mut delay = espforge_runtime::Delay::new();
    match device!(ctx, climate).measure(&mut delay) {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: climate t={:.2} c p={:.2} hpa rh={:.2} %",
            reading.temperature,
            reading.pressure / 100.0,
            reading.humidity
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: climate read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
