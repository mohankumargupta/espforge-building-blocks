//! Happy-path fixture for the project-local `<tca95xx>` driver.
//!
//! The driver itself is the generated `io` device: `Devices::new` builds
//! `tca95xx_driver::Tca95xx`, which configures all sixteen pins as outputs.
//! This app then drives a fixed pattern on the output latch once a second:
//!
//! - `TEST: tca95xx boot`
//! - `TEST: tca95xx outputs=0x00A5` once a second
//!
//! Pins match `drivers/tca95xx/board.json` (SCL 6, SDA 5) and the address is
//! 0x20, the part's address with every address pin low.

use crate::Context;

/// One update a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

/// P0.0, P0.2, P0.5 and P0.7 high; port 1 low.
const OUTPUTS: u16 = 0x00A5;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: tca95xx boot");
}

pub fn forever(ctx: &mut Context) {
    if let Err(error) = device!(ctx, io).write_outputs(OUTPUTS) {
        ctx.logger
            .error(format_args!("TEST: tca95xx write failed: {error:?}"));
    } else {
        ctx.logger
            .info(format_args!("TEST: tca95xx outputs={OUTPUTS:#06X}"));
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
