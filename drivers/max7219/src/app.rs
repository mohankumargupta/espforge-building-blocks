//! Happy-path fixture for the project-local, busless `<max7219>` driver.
//!
//! The driver itself is the generated `matrix` device: `Devices::new` builds
//! `max7219::MAX7219` from the three GPIO lines. `setup` sets the decode mode,
//! clears the digits, enables all eight, sets the intensity and powers the chip
//! on; every second `forever` writes the same frame to all eight digits:
//!
//! - `TEST: max7219 boot`
//! - `TEST: max7219 frame=aaaaaaaaaaaaaaaa` once a second
//!
//! The chip is write-only, so the app can only log that the writes succeeded;
//! the virtual part's `part_state` is what proves the bit-banged lines carried
//! the words.
//!
//! Pins match `drivers/max7219/board.json` (DATA 3, CS 7, SCK 6). There is no
//! MISO: the display never answers.

use crate::Context;
use max7219::DecodeMode;

/// One frame a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

/// Every row/digit lit: the chip's simplest "I am here" pattern.
const FRAME: [u8; 8] = [0xAA; 8];

/// Scan-limit register: all eight digits are multiplexed.
const SCAN_LIMIT: u8 = 0x0B;
const ALL_DIGITS: u8 = 0x07;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: max7219 boot");
    let matrix = device!(ctx, matrix);
    if let Err(error) = matrix.set_decode_mode(0, DecodeMode::NoDecode) {
        ctx.logger
            .error(format_args!("TEST: max7219 decode failed: {error:?}"));
    }
    if let Err(error) = matrix.clear_display(0) {
        ctx.logger
            .error(format_args!("TEST: max7219 clear failed: {error:?}"));
    }
    if let Err(error) = matrix.write_raw_byte(0, SCAN_LIMIT, ALL_DIGITS) {
        ctx.logger
            .error(format_args!("TEST: max7219 scan limit failed: {error:?}"));
    }
    if let Err(error) = matrix.set_intensity(0, 0x0F) {
        ctx.logger
            .error(format_args!("TEST: max7219 intensity failed: {error:?}"));
    }
    if let Err(error) = matrix.power_on() {
        ctx.logger
            .error(format_args!("TEST: max7219 power on failed: {error:?}"));
    }
}

pub fn forever(ctx: &mut Context) {
    let matrix = device!(ctx, matrix);
    if let Err(error) = matrix.write_raw(0, &FRAME) {
        ctx.logger
            .error(format_args!("TEST: max7219 write failed: {error:?}"));
        ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
        return;
    }
    ctx.logger.info("TEST: max7219 frame=aaaaaaaaaaaaaaaa");
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
