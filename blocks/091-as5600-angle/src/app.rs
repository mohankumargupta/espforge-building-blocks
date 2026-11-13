use crate::Context;

/// `address: 0x36`
const ADDRESS: u8 = 0x36;
/// Raw angle, 12 bits split across two registers.
const REGISTER_RAW_ANGLE: u8 = 0x0E;
/// Magnet status: MD (detected), ML (too weak), MH (too strong).
const REGISTER_STATUS: u8 = 0x1B;
const STATUS_MD: u8 = 0x20;
const STATUS_ML: u8 = 0x10;
const STATUS_MH: u8 = 0x08;
/// Counts per revolution.
const COUNTS_PER_REV: f32 = 4_096.0;
/// `update_interval: 200ms`
const UPDATE_INTERVAL_MS: u32 = 200;

fn read_register(ctx: &mut Context, register: u8) -> Option<u8> {
    let bus = device!(ctx, i2c0);
    let mut raw = [0u8; 1];
    bus.write_read(ADDRESS, &[register], &mut raw).ok()?;
    Some(raw[0])
}

pub fn setup(ctx: &mut Context) {
    match read_register(ctx, REGISTER_STATUS) {
        Some(status) if status & STATUS_MD == 0 => {
            ctx.logger
                .warn("as5600: no magnet detected; readings will be noise");
        }
        Some(status) if status & STATUS_ML != 0 => {
            ctx.logger.warn("as5600: magnet too weak, move it closer");
        }
        Some(status) if status & STATUS_MH != 0 => {
            ctx.logger.warn("as5600: magnet too strong, move it further");
        }
        Some(_) => ctx.logger.info("as5600: magnet detected, 4096 counts/rev"),
        None => ctx.logger.warn("as5600: no ACK at 0x36"),
    }
}

pub fn forever(ctx: &mut Context) {
    let bus = device!(ctx, i2c0);
    let mut raw = [0u8; 2];

    if bus
        .write_read(ADDRESS, &[REGISTER_RAW_ANGLE], &mut raw)
        .is_err()
    {
        ctx.logger.warn("as5600: angle read failed");
        ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
        return;
    }

    // The high register carries bits 11..8 in its low nibble.
    let counts = (u16::from(raw[0] & 0x0F) << 8) | u16::from(raw[1]);
    let degrees = f32::from(counts) * 360.0 / COUNTS_PER_REV;
    // A weather vane reads a compass bearing more naturally than an angle.
    let bearing = (degrees + 180.0) % 360.0;

    ctx.logger.info(format_args!(
        "Weather Vane Direction: {bearing:.1} deg ({counts} counts)"
    ));

    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
