//! Test fixture for the external `<tmp123>` driver (plantmp123.md).
//!
//! The driver is `drivers/tmp123/drivers/tmp123.kdl` plus the `tmp123-driver`
//! crate; this app only adds machine-checkable output:
//!
//! - `TEST: tmp123 boot`
//! - `TEST: reading t=25.00 c` once a second
//!
//! Pins match `drivers/tmp123/board.json` (CS 7, SCK 6, SO 2). The sensor has
//! no data input, so MOSI (GPIO 3) is declared and unused.

use crate::Context;

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: tmp123 boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, temp).read_celsius() {
        Ok(celsius) => ctx.logger.info(format_args!("TEST: reading t={:.2} c", celsius)),
        Err(_) => ctx.logger.error("TEST: reading failed"),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
