//! Happy-path fixture for the project-local, busless `<nxp74hc595>` driver.
//!
//! The driver itself is the generated `shift` device: `Devices::new` builds
//! `drive_74hc595::ShiftRegister` from the five GPIO lines. `setup` enables
//! the outputs, clears the latches and loads one pattern; every second
//! `forever` loads a second pattern:
//!
//! - `TEST: nxp74hc595 boot`
//! - `TEST: nxp74hc595 load=5a`
//! - `TEST: nxp74hc595 load=aa` once a second
//!
//! The register is write-only, so the app can only log that the loads ran;
//! the virtual part's `part_state` is what proves the bit-banged edges carried
//! the byte.
//!
//! Pins match `drivers/nxp74hc595/board.json` (OE 4, SER 3, SRCLR 5, SRCLK 6,
//! RCLK 7). There is no MISO: the shift register never answers.

use crate::Context;

/// One frame a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

/// The pattern loaded once during setup, before the steady frame.
const STARTUP_PATTERN: u8 = 0x5A;

/// The steady frame: four high outputs, four low, MSB first.
const FRAME: u8 = 0xAA;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: nxp74hc595 boot");
    let shift = device!(ctx, shift);
    shift.begin();
    shift.enable_output();
    shift.output_clear();
    shift.load(STARTUP_PATTERN);
    ctx.logger
        .info(format_args!("TEST: nxp74hc595 load={STARTUP_PATTERN:02x}"));
}

pub fn forever(ctx: &mut Context) {
    let shift = device!(ctx, shift);
    shift.load(FRAME);
    ctx.logger
        .info(format_args!("TEST: nxp74hc595 load={FRAME:02x}"));
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
