use crate::Context;

/// `address: 0x68`
const ADDRESS: u8 = 0x68;
/// Acceleration 0x3B..0x40, temperature 0x41..0x42, gyro 0x43..0x48.
const REGISTER_ACCEL: u8 = 0x3B;
const BURST_BYTES: usize = 14;
/// `update_interval: 100ms`
const UPDATE_INTERVAL_MS: u32 = 100;
/// Power management (wake) and the digital low-pass filter (44 Hz).
const REGISTER_PWR_MGMT_1: u8 = 0x6B;
const REGISTER_CONFIG: u8 = 0x1A;
const DLPF_44HZ: u8 = 0x03;
/// Default full-scale ranges: +/-2 g and +/-250 degrees/s.
const ACCEL_LSB_PER_G: f32 = 16_384.0;
const GYRO_LSB_PER_DPS: f32 = 131.0;

fn write_register(ctx: &mut Context, register: u8, value: u8) -> bool {
    device!(ctx, i2c0).write(ADDRESS, &[register, value]).is_ok()
}

fn axis(raw: &[u8], index: usize) -> i16 {
    i16::from_be_bytes([raw[index], raw[index + 1]])
}

pub fn setup(ctx: &mut Context) {
    // The part boots asleep: clear the sleep bit before anything else.
    if !write_register(ctx, REGISTER_PWR_MGMT_1, 0x00) {
        ctx.logger.warn("mpu6050-imu: no ACK at 0x68");
    } else if !write_register(ctx, REGISTER_CONFIG, DLPF_44HZ) {
        ctx.logger.warn("mpu6050-imu: DLPF write failed");
    } else {
        ctx.logger
            .info("mpu6050-imu: awake, DLPF 44 Hz, +/-2 g and 250 dps");
    }
}

pub fn forever(ctx: &mut Context) {
    let bus = device!(ctx, i2c0);
    let mut raw = [0u8; BURST_BYTES];

    if bus
        .write_read(ADDRESS, &[REGISTER_ACCEL], &mut raw)
        .is_err()
    {
        ctx.logger.warn("mpu6050-imu: burst read failed");
        ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
        return;
    }

    let ax = f32::from(axis(&raw, 0)) / ACCEL_LSB_PER_G;
    let ay = f32::from(axis(&raw, 2)) / ACCEL_LSB_PER_G;
    let az = f32::from(axis(&raw, 4)) / ACCEL_LSB_PER_G;
    // Bytes 6..8 are the die temperature: raw / 340 + 36.53 C.
    let temperature = f32::from(axis(&raw, 6)) / 340.0 + 36.53;
    let gx = f32::from(axis(&raw, 8)) / GYRO_LSB_PER_DPS;

    ctx.logger.info(format_args!(
        "Acceleration X/Y/Z: {ax:.2} / {ay:.2} / {az:.2} g"
    ));
    ctx.logger
        .info(format_args!("Angular Velocity X: {gx:.1} dps"));
    ctx.logger.info(format_args!(
        "IMU die temperature: {temperature:.1} C"
    ));

    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
