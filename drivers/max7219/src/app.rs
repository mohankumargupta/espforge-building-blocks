//! Happy-path fixture for the project-local `<max7219>` driver.
//!
//! The driver itself is the generated `matrix` device: `Devices::new` builds
//! `max7219_display::Max7219`, which only stores the SPI device. `setup` runs
//! the crate's `init()` sequence, powers the chip on and sets the intensity;
//! every second `forever` writes the same frame to all eight digits:
//!
//! - `TEST: max7219 boot`
//! - `TEST: max7219 frame=aaaaaaaaaaaaaaaa` once a second
//!
//! The chip is write-only, so the app can only log that the writes succeeded;
//! the virtual part's `part_state` is what proves the bytes arrived.
//!
//! Pins match `drivers/max7219/board.json` (CS 7, SCK 6, MOSI 3). There is no
//! MISO: the display never answers.

use crate::Context;

/// One frame a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

/// Every row/digit lit: the chip's simplest "I am here" pattern.
const FRAME: [u8; 8] = [0xAA; 8];

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: max7219 boot");
    let matrix = device!(ctx, matrix);
    if let Err(error) = matrix.init() {
        ctx.logger
            .error(format_args!("TEST: max7219 init failed: {error:?}"));
    }
    if let Err(error) = matrix.power_on() {
        ctx.logger
            .error(format_args!("TEST: max7219 power on failed: {error:?}"));
    }
    if let Err(error) = matrix.set_intensity(0, 0x0F) {
        ctx.logger
            .error(format_args!("TEST: max7219 intensity failed: {error:?}"));
    }
}

pub fn forever(ctx: &mut Context) {
    let matrix = device!(ctx, matrix);
    for (digit, value) in FRAME.iter().enumerate() {
        if let Err(error) = matrix.write_raw_digit(0, digit as u8, *value) {
            ctx.logger
                .error(format_args!("TEST: max7219 write failed: {error:?}"));
            ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
            return;
        }
    }
    ctx.logger.info("TEST: max7219 frame=aaaaaaaaaaaaaaaa");
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
