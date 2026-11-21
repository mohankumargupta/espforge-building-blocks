//! Happy-path fixture for the `<mcp9808>` driver.
//!
//! The driver itself is the generated `temperature` device: `Devices::new`
//! builds `mcp9808::MCP9808`. This app then reads the ambient temperature once
//! a second and logs it:
//!
//! - `TEST: mcp9808 boot`
//! - `TEST: mcp9808 temp=24.00 c` once a second
//!
//! Pins match `drivers/mcp9808/board.json` (SCL 6, SDA 5) and the address is
//! 0x18, the part's address with all three address pins low.

use crate::Context;

use mcp9808::reg_temp_generic::ReadableTempRegister;

/// One measurement a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: mcp9808 boot");
}

pub fn forever(ctx: &mut Context) {
    // `read_temperature` hands back the register. Decode the raw 13-bit
    // two's-complement value ourselves: the crate's `get_milli_celsius` maps a
    // negative reading to a positive one (`256 - n`), and its float helper is
    // behind a non-default feature.
    match device!(ctx, temperature).read_temperature() {
        Ok(register) => {
            let raw = register.get_raw_value() & 0x1FFF;
            let units = if raw & 0x1000 != 0 {
                raw as i32 - 0x2000
            } else {
                raw as i32
            };
            let milli_celsius = units * 1_000 / 16;
            ctx.logger.info(format_args!(
                "TEST: mcp9808 temp={:.2} c",
                milli_celsius as f32 / 1_000.0
            ));
        }
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: mcp9808 read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
