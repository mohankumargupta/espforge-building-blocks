//! Happy-path fixture for the bundled `<adxl345>` driver (planadxl345.md).
//!
//! The driver itself is the generated `accel` device: `Devices::new` builds
//! `espforge_drivers::Adxl345`, which first reads `DEVID` and fails
//! initialization unless it is 0xE5. This app only adds machine-checkable
//! output:
//!
//! - `TEST: adxl345 boot`
//! - `TEST: accel x=0 y=0 z=1024 mg` once a second
//!
//! Pins match `drivers/adxl345/board.json` (CS 7, SCK 6, MOSI 3, MISO 2).
//!
//! The driver reports `counts * 4` mg, while the part converts 1 g to 256
//! counts (the datasheet's 3.9 mg/LSB), so 1 g reads back as 1024 mg. That is
//! the driver's rounding, not a wrong part.

use crate::Context;

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: adxl345 boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, accel).read() {
        Ok(reading) => {
            let (x, y, z) = reading.milli_g();
            ctx.logger
                .info(format_args!("TEST: accel x={x} y={y} z={z} mg"));
        }
        Err(error) => ctx.logger.error(format_args!("TEST: accel read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
