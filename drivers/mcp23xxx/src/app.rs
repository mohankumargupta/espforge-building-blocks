//! Happy-path fixture for the project-local `<mcp23xxx>` driver.
//!
//! The driver itself is the generated `io` device: `Devices::new` builds
//! `mcp23xxx_driver::Mcp23xxx`, which makes all sixteen pins outputs. This app
//! then makes port B inputs with its pull-ups on once, and every second drives
//! a fixed pattern on port A and reads both ports back:
//!
//! - `TEST: mcp23xxx boot`
//! - `TEST: mcp23xxx a=aa b=00` once a second
//!
//! Port A reads its latch (0xAA), so a shifted or lost write shows up in the
//! log. Port B is an input port, so the line follows the level the part's
//! control channel drives: `set:levels=0x5A00` makes it read 0x5a.
//!
//! Pins match `drivers/mcp23xxx/board.json` (SCL 6, SDA 5) and the address is
//! 0x20, the part's address with every address pin low.

use crate::Context;
use mcp23xxx_driver::Port;

/// One update a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

/// Every other pin of port A driven high.
const OUTPUTS: u8 = 0xAA;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: mcp23xxx boot");
    let io = device!(ctx, io);
    if let Err(error) = io.set_direction(Port::B, 0xFF) {
        ctx.logger
            .error(format_args!("TEST: mcp23xxx direction failed: {error:?}"));
    }
    if let Err(error) = io.set_pull_ups(Port::B, 0xFF) {
        ctx.logger
            .error(format_args!("TEST: mcp23xxx pull-ups failed: {error:?}"));
    }
}

pub fn forever(ctx: &mut Context) {
    let io = device!(ctx, io);
    if let Err(error) = io.write_outputs(Port::A, OUTPUTS) {
        ctx.logger
            .error(format_args!("TEST: mcp23xxx write failed: {error:?}"));
    }
    match (io.read_inputs(Port::A), io.read_inputs(Port::B)) {
        (Ok(a), Ok(b)) => ctx
            .logger
            .info(format_args!("TEST: mcp23xxx a={a:02x} b={b:02x}")),
        (a, b) => ctx.logger.error(format_args!(
            "TEST: mcp23xxx read failed: {a:?} {b:?}"
        )),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
