//! Happy-path fixture for the project-local `<bh1750>` driver.
//!
//! The driver itself is the generated `light` device: `Devices::new` builds
//! `bh1750_driver::Bh1750`, which powers the part on. This app then takes one
//! one-shot high-resolution measurement a second and logs the illuminance:
//!
//! - `TEST: bh1750 boot`
//! - `TEST: bh1750 lux=300.0` once a second
//!
//! Pins match `drivers/bh1750/board.json` (SCL 6, SDA 5) and the address is
//! 0x23, the part's address with its ADDR pin low.

use crate::Context;

/// One measurement a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: bh1750 boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, light).read() {
        Ok(lux) => ctx
            .logger
            .info(format_args!("TEST: bh1750 lux={lux:.1}")),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: bh1750 read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
