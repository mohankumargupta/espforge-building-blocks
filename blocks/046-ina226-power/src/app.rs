use crate::Context;

/// `address: 0x40`
const ADDRESS: u8 = 0x40;
const REGISTER_CONFIG: u8 = 0x00;
const REGISTER_SHUNT_VOLTAGE: u8 = 0x01;
const REGISTER_BUS_VOLTAGE: u8 = 0x02;
const REGISTER_POWER: u8 = 0x03;
const REGISTER_CURRENT: u8 = 0x04;
const REGISTER_CALIBRATION: u8 = 0x05;

/// `shunt_resistance: 0.002 ohm`
const SHUNT_OHMS: f32 = 0.002;
/// `max_current: 20.0A`
const MAX_CURRENT_A: f32 = 20.0;
/// The datasheet's fixed LSBs.
const SHUNT_LSB_V: f32 = 2.5e-6;
const BUS_LSB_V: f32 = 1.25e-3;

/// `update_interval: 5s`
const UPDATE_INTERVAL_MS: u32 = 5_000;

/// Configuration: 16 averages, 1.1 ms bus and shunt conversions, continuous.
const CONFIG: u16 = 0x4527;

fn write_register(ctx: &mut Context, register: u8, value: u16) -> bool {
    let bus = device!(ctx, i2c0);
    bus.write(ADDRESS, &[register, (value >> 8) as u8, value as u8])
        .is_ok()
}

fn read_register(ctx: &mut Context, register: u8) -> Option<u16> {
    let bus = device!(ctx, i2c0);
    let mut raw = [0u8; 2];
    bus.write_read(ADDRESS, &[register], &mut raw).ok()?;
    Some(u16::from_be_bytes(raw))
}

pub fn setup(ctx: &mut Context) {
    let current_lsb = MAX_CURRENT_A / 32_768.0;
    let calibration = (0.00512 / (current_lsb * SHUNT_OHMS)) as u16;

    if !write_register(ctx, REGISTER_CONFIG, CONFIG) {
        ctx.logger.warn("ina226: config write failed");
    } else if !write_register(ctx, REGISTER_CALIBRATION, calibration) {
        ctx.logger.warn("ina226: calibration write failed");
    } else {
        ctx.logger.info(format_args!(
            "ina226: 0x40 configured, calibration 0x{calibration:04X}, current LSB {:.6} A",
            current_lsb
        ));
    }
}

pub fn forever(ctx: &mut Context) {
    let current_lsb = MAX_CURRENT_A / 32_768.0;

    let (Some(shunt_raw), Some(bus_raw), Some(current_raw), Some(power_raw)) = (
        read_register(ctx, REGISTER_SHUNT_VOLTAGE),
        read_register(ctx, REGISTER_BUS_VOLTAGE),
        read_register(ctx, REGISTER_CURRENT),
        read_register(ctx, REGISTER_POWER),
    ) else {
        ctx.logger.warn("ina226: register read failed");
        ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
        return;
    };

    // Shunt voltage is signed: a negative reading is current flowing backwards.
    let shunt_volts = (shunt_raw as i16) as f32 * SHUNT_LSB_V;
    let bus_volts = bus_raw as f32 * BUS_LSB_V;
    // The current register is signed too, and power is 25x the current LSB.
    let amps = (current_raw as i16) as f32 * current_lsb;
    let watts = power_raw as f32 * 25.0 * current_lsb;

    ctx.logger
        .info(format_args!("Bus Voltage: {bus_volts:.2} V"));
    ctx.logger
        .info(format_args!("Load Current: {amps:.3} A"));
    ctx.logger.info(format_args!("Total Power: {watts:.2} W"));
    ctx.logger.info(format_args!(
        "Shunt drop: {:.2} mV",
        shunt_volts * 1_000.0
    ));

    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
