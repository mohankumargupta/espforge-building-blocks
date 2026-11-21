//! Happy-path fixture for the `<bme280_spi>` driver (planbme280_spi.md).
//!
//! The driver itself is the generated `climate` device: `Devices::new` builds
//! `bme280::spi::BME280`, whose `new()` reads the chip id and calibration. The
//! crate then needs `init()` for the oversampling configuration, so `setup`
//! runs it. This app only adds machine-checkable output:
//!
//! - `TEST: bme280_spi boot`
//! - `TEST: climate t=22.50 c p=1013.25 hpa rh=45.00 %` once a second
//!
//! Pins match `drivers/bme280_spi/board.json` (CS 7, SCK 6, MOSI 3, MISO 2).
//! The crate reports pressure in pascals.

use crate::Context;

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    let mut delay = espforge_runtime::Delay::new();
    match device!(ctx, climate).init(&mut delay) {
        Ok(()) => ctx.logger.info("TEST: bme280_spi boot"),
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
