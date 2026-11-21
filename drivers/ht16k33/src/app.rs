//! Happy-path fixture for the project-local `<ht16k33>` driver.
//!
//! The driver itself is the generated `display` device: `Devices::new` builds
//! `ht16k33_driver::Ht16k33`, which starts the oscillator and the display.
//! This app then writes one frame to the display RAM a second, reads it back,
//! and logs the sixteen bytes it got:
//!
//! - `TEST: ht16k33 boot`
//! - `TEST: ht16k33 ram=…` once a second
//!
//! The pattern is a diagonal line, so a frame that is mistimed, shifted or
//! zeroed shows up differently in the log.
//!
//! Pins match `drivers/ht16k33/board.json` (SCL 6, SDA 5) and the address is
//! 0x70, the part's address with all three address jumpers low.

use crate::Context;

/// One frame a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

/// Two rows of a diagonal line: column 0 on row 0, column 1 on row 1, and so
/// on, twice round the sixteen bytes.
const PATTERN: [u8; ht16k33_driver::RAM_BYTES] = [
    0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40,
    0x80,
];

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: ht16k33 boot");
}

pub fn forever(ctx: &mut Context) {
    let display = device!(ctx, display);
    if let Err(error) = display.write_ram(&PATTERN) {
        ctx.logger
            .error(format_args!("TEST: ht16k33 write failed: {error:?}"));
    }
    match display.read_ram() {
        Ok(ram) => ctx.logger.info(format_args!(
            "TEST: ht16k33 ram={:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            ram[0], ram[1], ram[2], ram[3], ram[4], ram[5], ram[6], ram[7], ram[8], ram[9], ram[10],
            ram[11], ram[12], ram[13], ram[14], ram[15],
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: ht16k33 read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
