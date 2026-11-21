//! Happy-path fixture for the project-local `<pcf857x>` driver.
//!
//! The driver itself is the generated `io` device: `Devices::new` builds
//! `pcf857x_driver::Pcf857x`, which writes nothing because the part has no
//! registers. Every second this app drives a fixed pattern on the pins and
//! reads the port back:
//!
//! - `TEST: pcf857x boot`
//! - `TEST: pcf857x pins=aa` once a second
//!
//! With nothing else on the pins a written byte reads back, so a lost write
//! shows up in the log. The part's control channel pulls released pins low:
//! `set:levels=0x0F` makes the next line `TEST: pcf857x pins=0a`.
//!
//! Pins match `drivers/pcf857x/board.json` (SCL 6, SDA 5) and the address is
//! 0x20, the part's address with every address pin low.

use crate::Context;

/// One update a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

/// Every other pin driven high.
const OUTPUTS: u8 = 0xAA;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: pcf857x boot");
}

pub fn forever(ctx: &mut Context) {
    let io = device!(ctx, io);
    if let Err(error) = io.write(OUTPUTS) {
        ctx.logger
            .error(format_args!("TEST: pcf857x write failed: {error:?}"));
    }
    match io.read() {
        Ok(pins) => ctx
            .logger
            .info(format_args!("TEST: pcf857x pins={pins:02x}")),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: pcf857x read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
