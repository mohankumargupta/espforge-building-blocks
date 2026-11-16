//! Happy-path fixture for the project-local `<bme280_spi>` driver
//! (planbme280_spi.md).
//!
//! The driver itself is the generated `climate` device: `Devices::new` builds
//! `bme280_spi_driver::Bme280Spi`, which checks `ID` for 0x60, reads the
//! calibration block and puts the chip in normal mode. This app only adds
//! machine-checkable output:
//!
//! - `TEST: bme280_spi boot`
//! - `TEST: climate t=22.50 c p=1013.25 hpa rh=45.00 %` once a second
//!
//! Pins match `drivers/bme280_spi/board.json` (CS 7, SCK 6, MOSI 3, MISO 2).

use crate::Context;

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: bme280_spi boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, climate).read() {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: climate t={:.2} c p={:.2} hpa rh={:.2} %",
            reading.temperature_c, reading.pressure_hpa, reading.humidity_percent
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: climate read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
