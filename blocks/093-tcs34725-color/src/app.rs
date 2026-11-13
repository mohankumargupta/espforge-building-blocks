use crate::Context;

/// `address: 0x29`
const ADDRESS: u8 = 0x29;
/// Command bit (0x80) plus the register number.
const REGISTER_ENABLE: u8 = 0x80 | 0x00;
const REGISTER_ATIME: u8 = 0x80 | 0x01;
const REGISTER_CONTROL: u8 = 0x80 | 0x0F;
const REGISTER_RGBC: u8 = 0x80 | 0x14;
/// PON | AEN: power on and enable the ADC.
const ENABLE_RGBC: u8 = 0x03;
/// `integration_time: 50ms` (0xFF - 50/2.4 = 0xEB) and `gain: 4x` (0x01).
const ATIME_50MS: u8 = 0xEB;
const CONTROL_GAIN_4X: u8 = 0x01;
/// The 50 ms integration plus some margin.
const INTEGRATION_MS: u32 = 60;
/// Colour does not change quickly here.
const UPDATE_INTERVAL_MS: u32 = 2_000;

fn write_register(ctx: &mut Context, register: u8, value: u8) -> bool {
    device!(ctx, i2c0).write(ADDRESS, &[register, value]).is_ok()
}

pub fn setup(ctx: &mut Context) {
    if !write_register(ctx, REGISTER_ATIME, ATIME_50MS)
        || !write_register(ctx, REGISTER_CONTROL, CONTROL_GAIN_4X)
        || !write_register(ctx, REGISTER_ENABLE, ENABLE_RGBC)
    {
        ctx.logger.warn("tcs34725: configuration failed at 0x29");
    } else {
        ctx.logger.info(
            "tcs34725: Surface Color Sensor, 4x gain, 50 ms integration",
        );
    }
}

/// Lux from the clear channel, with the datasheet's glass-attenuation and
/// 4x-gain factors folded in.
fn lux(clear: f32, red: f32, green: f32, blue: f32) -> f32 {
    const GAIN_FACTOR: f32 = 4.0 * 430.0;
    let clear_comp = clear - ((red + green + blue) / 3.0);
    ((-0.32466 * red) + (1.57837 * green) + (-0.73191 * blue))
        .max(clear_comp * 0.5)
        / GAIN_FACTOR
        * 4.0
}

/// McCamy's approximation: CCT from the CIE 1931 chromaticity coordinates.
fn colour_temperature(red: f32, green: f32, blue: f32) -> f32 {
    let sum = red + green + blue;
    if sum <= 0.0 {
        return 0.0;
    }
    let x = red / sum;
    let y = green / sum;
    let denominator = 0.1858 - y;
    if denominator.abs() < f32::EPSILON {
        return 0.0;
    }
    let n = (x - 0.3320) / denominator;
    4_490.0 * n * n * n + 3_523.0 * n * n + 6_823.0 * n + 5_517.0
}

pub fn forever(ctx: &mut Context) {
    let bus = device!(ctx, i2c0);
    let mut raw = [0u8; 8];

    if bus.write_read(ADDRESS, &[REGISTER_RGBC], &mut raw).is_err() {
        ctx.logger.warn("tcs34725: read failed");
        ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
        return;
    }

    // The result is clear, red, green, blue as little-endian 16-bit pairs.
    let channel = |index: usize| f32::from(u16::from_le_bytes([raw[index], raw[index + 1]]));
    let (clear, red, green, blue) = (channel(0), channel(2), channel(4), channel(6));

    let lux = lux(clear, red, green, blue);
    let cct = colour_temperature(red, green, blue);
    let saturation = if clear > 0.0 { 1.0 - (blue / clear) } else { 0.0 };

    ctx.logger.info(format_args!(
        "Color R/G/B/C: {red:.0} {green:.0} {blue:.0} {clear:.0}"
    ));
    ctx.logger
        .info(format_args!("Surface Colour: {lux:.1} lx"));
    ctx.logger
        .info(format_args!("CCT: {cct:.0} K, saturation {saturation:.2}"));

    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
