use crate::Context;

/// `address: 0x23` (ADDR pin low; 0x5C when it is high).
const ADDRESS: u8 = 0x23;
/// Continuous high-resolution mode: 1 lx, ~120 ms per measurement.
const COMMAND_CONTINUOUS_HIGH_RES: u8 = 0x10;
/// `measurement_duration: 69` ms plus margin before the first read.
const STARTUP_MS: u32 = 180;
/// The datasheet's divisor: the raw count is 1.2 * lux.
const COUNTS_PER_LUX: f32 = 1.2;
/// `update_interval: 10s`
const UPDATE_INTERVAL_MS: u32 = 10_000;

pub fn setup(ctx: &mut Context) {
    if device!(ctx, i2c0)
        .write(ADDRESS, &[COMMAND_CONTINUOUS_HIGH_RES])
        .is_ok()
    {
        ctx.logger
            .info("bh1750: continuous high-resolution mode started");
        ctx.delay.delay_ms(STARTUP_MS);
    } else {
        ctx.logger.warn("bh1750: no ACK at 0x23");
    }
}

pub fn forever(ctx: &mut Context) {
    let bus = device!(ctx, i2c0);
    let mut raw = [0u8; 2];

    match bus.read(ADDRESS, &mut raw) {
        Ok(()) => {
            let counts = u16::from_be_bytes(raw);
            let lux = f32::from(counts) / COUNTS_PER_LUX;
            ctx.logger
                .info(format_args!("Ambient Illuminance: {lux:.1} lx"));
        }
        Err(_) => ctx.logger.warn("bh1750: read failed"),
    }

    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
