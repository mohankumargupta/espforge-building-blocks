//! Happy-path fixture for the project-local `<aip31068>` driver.
//!
//! The driver itself is the generated `lcd` device: `Devices::new` builds
//! `aip31068_driver::Aip31068`, which sends the HD44780 power-on sequence
//! (function set, display on, clear, entry mode). This app then writes the
//! two lines once a second, so a write error cannot hide:
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
    ctx.logger.info("TEST: aip31068 boot");
}

pub fn forever(ctx: &mut Context) {
    let banner = device!(ctx, lcd).write_line(0, BANNER);
    let footer = device!(ctx, lcd).write_line(1, FOOTER);
    match (banner, footer) {
        (Ok(()), Ok(())) => ctx
            .logger
            .info(format_args!("TEST: lcd {BANNER} / {FOOTER}")),
        (banner, footer) => ctx
            .logger
            .error(format_args!("TEST: lcd write failed: {banner:?} {footer:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
