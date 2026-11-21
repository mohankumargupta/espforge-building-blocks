//! Happy-path fixture for the `<aip31068>` driver.
//!
//! The driver itself is the generated `lcd` device: `Devices::new` builds
//! `i2c_character_display::CharacterDisplayAIP31068`. The crate needs an
//! explicit `init()` after construction (the AiP31068's 8-bit function set,
//! display on, clear and entry mode), so `setup` runs it, then this app writes
//! the two lines once a second:
//!
//! - `TEST: aip31068 boot`
//! - `TEST: lcd aip31068-lcd / hello-i2c` once a second
//!
//! The panel is write-only, so what it received is checked with
//! `part lcd state`, not from the firmware.
//!
//! Pins match `drivers/aip31068/board.json` (SCL 6, SDA 5) and the address is
//! 0x3E, the part's only I2C address.

use crate::Context;

/// One write a second, so a stalled display shows up in the log.
const UPDATE_INTERVAL_MS: u32 = 1_000;

/// The top line, which fits the panel's 16 columns.
const BANNER: &str = "aip31068-lcd";
/// The bottom line.
const FOOTER: &str = "hello-i2c";

pub fn setup(ctx: &mut Context) {
    match device!(ctx, lcd).init() {
        Ok(()) => ctx.logger.info("TEST: aip31068 boot"),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: lcd init failed: {error:?}")),
    }
}

pub fn forever(ctx: &mut Context) {
    // `set_cursor(col, row)` is zero-indexed, so (0, 0) and (0, 1) are the two
    // rows' first columns.
    let banner = device!(ctx, lcd)
        .set_cursor(0, 0)
        .and_then(|lcd| lcd.print(BANNER))
        .is_ok();
    let footer = device!(ctx, lcd)
        .set_cursor(0, 1)
        .and_then(|lcd| lcd.print(FOOTER))
        .is_ok();
    if banner && footer {
        ctx.logger
            .info(format_args!("TEST: lcd {BANNER} / {FOOTER}"));
    } else {
        ctx.logger.error("TEST: lcd write failed");
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
