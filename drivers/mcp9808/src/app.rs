//! Happy-path fixture for the project-local `<mcp9808>` driver.
//!
//! The driver itself is the generated `temperature` device: `Devices::new`
//! builds `mcp9808_driver::Mcp9808`, which identifies the part and sets its
//! resolution. This app then reads the ambient temperature once a second and
//! logs it:
//!
//! - `TEST: mcp9808 boot`
//! - `TEST: mcp9808 temp=24.00 c` once a second
//!
//! Pins match `drivers/mcp9808/board.json` (SCL 6, SDA 5) and the address is
//! 0x18, the part's address with all three address pins low.

use crate::Context;

/// One measurement a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: mcp9808 boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, temperature).read() {
        Ok(temperature) => ctx
            .logger
            .info(format_args!("TEST: mcp9808 temp={temperature:.2} c")),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: mcp9808 read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
