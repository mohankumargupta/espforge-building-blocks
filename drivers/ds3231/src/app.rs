//! Happy-path fixture for the `<ds3231>` driver.
//!
//! The driver itself is the generated `rtc` device: `Devices::new` builds
//! `ds323x::Ds323x::new_ds3231`. This app then reads the date and time and the
//! die temperature once a second:
//!
//! - `TEST: ds3231 boot`
//! - `TEST: ds3231 2026-01-01 00:00:00 w=1` once a second
//! - `TEST: ds3231 t=25.00 c` once a second
//!
//! Pins match `drivers/ds3231/board.json` (SCL 6, SDA 5) and the address is
//! 0x68, the part's only I2C address. The part does not count seconds: a test
//! that wants the clock to move sets it with
//! `just ds3231-part-input "part rtc input set:hour=8"`.

use crate::Context;

use ds323x::{Datelike, DateTimeAccess, Rtcc, Timelike};

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: ds3231 boot");
}

pub fn forever(ctx: &mut Context) {
    // `datetime()` derives the weekday from the calendar date; the fixture's
    // part stores day 1 in its own weekday register, so read that instead.
    let weekday = device!(ctx, rtc).weekday();
    match (device!(ctx, rtc).datetime(), weekday) {
        (Ok(time), Ok(weekday)) => ctx.logger.info(format_args!(
            "TEST: ds3231 {}-{:02}-{:02} {:02}:{:02}:{:02} w={}",
            time.year(),
            time.month(),
            time.day(),
            time.hour(),
            time.minute(),
            time.second(),
            weekday
        )),
        _ => ctx.logger.error("TEST: ds3231 read failed"),
    }
    match device!(ctx, rtc).temperature() {
        Ok(celsius) => ctx
            .logger
            .info(format_args!("TEST: ds3231 t={celsius:.2} c")),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: ds3231 temperature failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
