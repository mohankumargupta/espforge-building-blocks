//! Happy-path fixture for the bundled `<scd4x>` driver.
//!
//! The driver itself is the generated `co2` device: `Devices::new` builds
//! `espforge_drivers::Scd4x`, which has no init sequence, so `setup` calls
//! `start()` to begin periodic measurement. This app then reads the last
//! measurement once a second and logs it:
//!
//! - `TEST: scd4x boot`
//! - `TEST: scd4x started`
//! - `TEST: scd4x co2=400 ppm t=22.49 c rh=44.99` once a second
//!
//! Pins match `drivers/scd4x/board.json` (SCL 6, SDA 5) and the address is
//! 0x62, the part's only address.
//!
//! The bundled driver reports centi-units: `temperature_centi_c` and
//! `humidity_centi_percent` are hundredths, so the log line divides by 100.
//! Its integer conversion truncates, so the part's 22.50 °C and 45.00 %RH
//! read back as 22.49 °C and 44.99 %RH.

use crate::Context;

/// One measurement a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: scd4x boot");
    match device!(ctx, co2).start() {
        Ok(()) => ctx.logger.info("TEST: scd4x started"),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: scd4x start failed: {error:?}")),
    }
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, co2).read() {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: scd4x co2={} ppm t={:.2} c rh={:.2}",
            reading.co2_ppm,
            f32::from(reading.temperature_centi_c) / 100.0,
            f32::from(reading.humidity_centi_percent) / 100.0
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: scd4x read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
