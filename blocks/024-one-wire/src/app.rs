use crate::Context;

/// `update_interval: 15s`
const UPDATE_INTERVAL_MS: u32 = 15_000;

/// The intended shape: `<one_wire>` performs the reset/presence handshake and
/// the ROM search, and `<dallas_temp>` exposes one addressed device. Neither
/// element exists yet, so `temperature_c()` is a proposal, not an API.
pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("one-wire: water tank probe on GPIO4 (proposed)");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, water_tank).temperature_c() {
        Ok(celsius) => ctx
            .logger
            .info(format_args!("Water Tank Temperature: {celsius:.2} C")),
        Err(_) => ctx
            .logger
            .warn("one-wire: no presence pulse or bad scratchpad CRC"),
    }

    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
