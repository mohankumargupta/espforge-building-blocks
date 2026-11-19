//! Happy-path fixture for the project-local `<tlv493d>` driver.
//!
//! The driver itself is the generated `field` device: `Devices::new` builds
//! `tlv493d_driver::Tlv493d`, whose constructor reads one frame to prove the
//! part answers. This app then reads the ten-byte frame once a second and logs
//! the decoded field and temperature:
//!
//! - `TEST: tlv493d boot`
//! - `TEST: tlv493d bx=0.00 by=0.00 bz=0.00 mt t=25.00 c` once a second
//!
//! Pins match `drivers/tlv493d/board.json` (SCL 6, SDA 5) and the address is
//! 0x5E, the part's address with the ADDR strap low.

use crate::Context;

/// One measurement a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: tlv493d boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, field).read() {
        Ok(reading) => ctx.logger.info(format_args!(
            "TEST: tlv493d bx={:.2} by={:.2} bz={:.2} mt t={:.2} c",
            reading.x_mt, reading.y_mt, reading.z_mt, reading.temperature_c
        )),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: tlv493d read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
