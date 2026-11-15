//! Happy-path fixture for `<mhz19b>` over UART.
//!
//! The driver itself is the generated `co2` device: `Devices::new` builds
//! `edrv_mhz19::blocking::MHZ19` on the UART1 port. This app only adds
//! machine-checkable output:
//!
//! - `TEST: mhz19b boot`
//! - `TEST: co2=400 ppm` once a second
//!
//! Pins match `drivers/mhz19b/board.json` (chip TX on GPIO 4, RX on GPIO 5),
//! and the line runs at the sensor's fixed 9600 baud.

use crate::Context;

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: mhz19b boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, co2).read_co2() {
        Ok(reading) => ctx
            .logger
            .info(format_args!("TEST: co2={} ppm", reading.co2_ppm)),
        Err(_) => ctx.logger.error("TEST: read failed"),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
