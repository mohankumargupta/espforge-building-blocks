//! Happy-path fixture for the project-local `<mpu9250>` driver
//! (planmpu9250.md).
//!
//! The driver itself is the generated `imu` device: `Devices::new` builds
//! `mpu9250_driver::Mpu9250`, which resets the part, checks `WHO_AM_I` for
//! 0x71, applies the configuration and wakes it. This app only adds
//! machine-checkable output:
//!
//! - `TEST: mpu9250 boot`
//! - `TEST: imu ax=… ay=… az=… g gx=… gy=… gz=… dps t=… c` once a second
//!
//! Pins match `drivers/mpu9250/board.json` (CS 7, SCK 6, MOSI 3, MISO 2).

use crate::Context;

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: mpu9250 boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, imu).read() {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: imu ax={:.2} ay={:.2} az={:.2} g gx={:.2} gy={:.2} gz={:.2} dps t={:.2} c",
            reading.ax_g,
            reading.ay_g,
            reading.az_g,
            reading.gx_dps,
            reading.gy_dps,
            reading.gz_dps,
            reading.temperature_c,
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: imu read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
