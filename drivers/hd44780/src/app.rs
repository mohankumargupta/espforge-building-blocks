//! Happy-path fixture for the project-local `<hd44780>` driver.
//!
//! The driver itself is the generated `lcd` device: `Devices::new` builds
//! `i2c_character_display::CharacterDisplayPCF8574T` from the bus. `setup`
//! initialises the display, turns the backlight on, clears it and prints two
//! lines:
//!
//! - `TEST: hd44780 boot`
//! - `TEST: hd44780 line1=hd44780 ok`
//! - `TEST: hd44780 line2=esp32c3`
//!
//! The display is write-only over this adapter, so the app can only log that
//! the writes ran; the virtual part's `part_state` is what proves the nibbles
//! reached the controller's DDRAM.
//!
//! Pins match `drivers/hd44780/board.json` (SCL 6, SDA 5), and the backpack
//! answers at the crate's default 0x27.

use crate::Context;

/// How often `forever` comes around; the display is written once in setup.
const IDLE_INTERVAL_MS: u32 = 1_000;

/// The two 16-column lines the recipe checks.
const LINE_1: &str = "HD44780 ok";
const LINE_2: &str = "esp32c3";

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: hd44780 boot");
    let lcd = device!(ctx, lcd);
    if let Err(error) = lcd.init() {
        ctx.logger
            .error(format_args!("TEST: hd44780 init failed: {error:?}"));
        return;
    }
    if let Err(error) = lcd.backlight(true) {
        ctx.logger
            .error(format_args!("TEST: hd44780 backlight failed: {error:?}"));
    }
    if let Err(error) = lcd.clear() {
        ctx.logger
            .error(format_args!("TEST: hd44780 clear failed: {error:?}"));
        return;
    }
    if let Err(error) = lcd.print(LINE_1) {
        ctx.logger
            .error(format_args!("TEST: hd44780 line1 failed: {error:?}"));
        return;
    }
    if let Err(error) = lcd.set_cursor(0, 1) {
        ctx.logger
            .error(format_args!("TEST: hd44780 cursor failed: {error:?}"));
        return;
    }
    if let Err(error) = lcd.print(LINE_2) {
        ctx.logger
            .error(format_args!("TEST: hd44780 line2 failed: {error:?}"));
        return;
    }
    ctx.logger.info("TEST: hd44780 line1=hd44780 ok");
    ctx.logger.info("TEST: hd44780 line2=esp32c3");
}

pub fn forever(ctx: &mut Context) {
    ctx.delay.delay_ms(IDLE_INTERVAL_MS);
}
