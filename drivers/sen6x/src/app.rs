//! Happy-path fixture for the project-local `<sen6x>` driver.
//!
//! The driver itself is the generated `air` device: `Devices::new` builds
//! `sen6x_driver::Sen6x`, which starts continuous measurement. This app then
//! reads the nine measured values once a second and logs them:
//!
//! - `TEST: sen6x boot`
//! - `TEST: sen6x pm1=8.0 pm2.5=12.5 pm4=15.0 pm10=20.0 ug/m3 t=22.50 c
//!   rh=45.00 voc=100.0 nox=1.0 co2=400 ppm` once a second
//!
//! Pins match `drivers/sen6x/board.json` (SCL 6, SDA 5) and the address is
//! 0x6B, the part's only address.

use crate::Context;

/// One measurement a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: sen6x boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, air).read() {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: sen6x pm1={:.1} pm2.5={:.1} pm4={:.1} pm10={:.1} ug/m3 t={:.2} c rh={:.2} voc={:.1} nox={:.1} co2={} ppm",
            reading.pm1_0_ug_m3,
            reading.pm2_5_ug_m3,
            reading.pm4_0_ug_m3,
            reading.pm10_ug_m3,
            reading.temperature_c,
            reading.humidity_percent,
            reading.voc_index,
            reading.nox_index,
            reading.co2_ppm
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: sen6x read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
