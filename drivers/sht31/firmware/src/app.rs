//! Test fixture for `<sht31>` (testsht31.md).
//!
//! The driver itself is the generated `air` device: `Devices::new` builds
//! `embedded_sht3x::Sht3x` at boot, which sends the soft reset the virtual part
//! models. This app only adds machine-checkable output:
//!
//! - `TEST: sht31 boot`
//! - `TEST: status=0x8010`
//! - `TEST: reading t=22.50 rh=45.00` once a second
//!
//! Pins match `drivers/sht31/board.json` (SCL 6, SDA 5) and the address is
//! 0x44, the part's ADDR-low default.

use crate::Context;

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: sht31 boot");

    // `Sht3x::new` has already sent the soft reset. A fresh part answers
    // 0x8010: ALERT_PENDING | RESET.
    match device!(ctx, air).get_status() {
        Ok(status) => ctx.logger.info(format_args!("TEST: status=0x{:04x}", status.bits())),
        Err(_) => ctx.logger.error("TEST: status read failed"),
    }
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, air).single_measurement() {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: reading t={:.2} rh={:.2}",
            reading.temperature.0,
            reading.relative_humidity.value()
        )),
        Err(_) => ctx.logger.error("TEST: measurement failed"),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
