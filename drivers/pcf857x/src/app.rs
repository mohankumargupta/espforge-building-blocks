//! Happy-path fixture for the `<pcf857x>` driver.
//!
//! The driver itself is the generated `io` device: `Devices::new` builds
//! `pcf857x::Pcf8574`, which writes nothing because the part has no registers.
//! Every second this app drives a fixed pattern on the pins and reads the port
//! back:
//!
//! - `TEST: pcf857x boot`
//! - `TEST: pcf857x pins=aa` once a second
//!
//! `get` releases the pins it reads, so the answer is the level on them, not
//! the byte last written: with nothing pulling them low it reads `0xff`, and
//! the part's control channel pulling the low nibble down
//! (`set:levels=0x0F`) makes the next line `TEST: pcf857x pins=0f`.
//!
//! Pins match `drivers/pcf857x/board.json` (SCL 6, SDA 5) and the address is
//! 0x20, the part's address with every address pin low.

use crate::Context;
use pcf857x::PinFlag;

/// One update a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

/// Every other pin driven high.
const OUTPUTS: u8 = 0xAA;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: pcf857x boot");
}

pub fn forever(ctx: &mut Context) {
    let io = device!(ctx, io);
    match io.set(OUTPUTS) {
        Ok(()) => ctx.logger.info("TEST: pcf857x wrote=aa"),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: pcf857x write failed: {error:?}")),
    }
    let all_pins = PinFlag::P0
        | PinFlag::P1
        | PinFlag::P2
        | PinFlag::P3
        | PinFlag::P4
        | PinFlag::P5
        | PinFlag::P6
        | PinFlag::P7;
    match io.get(all_pins) {
        Ok(pins) => ctx
            .logger
            .info(format_args!("TEST: pcf857x pins={pins:02x}")),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: pcf857x read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
