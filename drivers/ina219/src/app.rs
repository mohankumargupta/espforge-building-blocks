//! Happy-path fixture for the `<ina219>` driver.
//!
//! The driver itself is the generated `power` device: `Devices::new` builds
//! `ina219::SyncIna219` with the fixture's calibration, after resetting the
//! part and loading the calibration register. This app then reads one
//! measurement a second and logs the converted values:
//!
//! - `TEST: ina219 boot`
//! - `TEST: ina219 bus=5.00 v shunt=10.00 mv current=100.00 ma power=500.00 mw`
//!   once a second
//!
//! Pins match `drivers/ina219/board.json` (SCL 6, SDA 5) and the address is
//! 0x40, the part's address with both address pins low.

use crate::Context;

/// One measurement a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: ina219 boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, power).next_measurement() {
        Ok(Some(reading)) => ctx.logger.info(format_args!(
            "TEST: ina219 bus={:.2} v shunt={:.2} mv current={:.2} ma power={:.2} mw",
            f32::from(reading.bus_voltage.voltage_mv()) / 1000.0,
            f32::from(reading.shunt_voltage.shunt_voltage_mv()),
            reading.current.0 as f32 / 1000.0,
            reading.power.0 as f32 / 1000.0
        )),
        Ok(None) => {}
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: ina219 read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
