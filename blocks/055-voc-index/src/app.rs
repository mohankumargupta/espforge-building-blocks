use crate::Context;

/// `address: 0x59`
const ADDRESS: u8 = 0x59;
/// `measure_raw` with humidity compensation: 0x26 0x0F plus two data words.
const MEASURE_RAW: u8 = 0x26;
/// `update_interval: 10s`
const UPDATE_INTERVAL_MS: u32 = 10_000;
/// The part needs ~30 ms for one measurement plus the read.
const CONVERSION_MS: u32 = 50;

/// CRC8 for SHT/SGP sensors: polynomial 0x31, initial 0xFF, no reflection.
fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0xFFu8;
    for byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x31
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Sensirion's RH/T ticks: 0 %RH = 0x8000, 100 %RH = 0x6666.
fn humidity_ticks(percent: f32) -> u16 {
    let ticks = 0x8000.0 - percent.clamp(0.0, 100.0) * ((0x8000 - 0x6666) as f32 / 100.0);
    ticks.round() as u16
}

/// Temperature ticks: 0 C = 0x6666, 100 C = 0x0000.
fn temperature_ticks(celsius: f32) -> u16 {
    let ticks = 0x6666.0 - celsius.clamp(0.0, 100.0) * ((0x6666 - 0x0000) as f32 / 100.0);
    ticks.round() as u16
}

/// Split a 16-bit word into the two bytes the sensor expects, CRC included.
fn with_crc(word: u16, out: &mut [u8; 3]) {
    out[0] = (word >> 8) as u8;
    out[1] = word as u8;
    out[2] = crc8(&out[..2]);
}

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("voc-index: SGP40 at 0x59, raw signal with RH/T compensation");

    if device!(ctx, air).start().is_err() {
        ctx.logger.warn("voc-index: SCD4x did not start");
    }
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let bus = device!(ctx, i2c0);

    // Compensation inputs from a real reading; a failed read falls back to 25 C / 50 %.
    let (celsius, percent) = match device!(ctx, air).read() {
        Ok(reading) => (
            f32::from(reading.temperature_centi_c) / 100.0,
            f32::from(reading.humidity_centi_pct) / 100.0,
        ),
        Err(_) => (25.0, 50.0),
    };

    let mut command = [0u8; 8];
    command[0] = MEASURE_RAW;
    command[1] = 0x0F;
    with_crc(humidity_ticks(percent), &mut command[2..5].try_into().unwrap());
    with_crc(temperature_ticks(celsius), &mut command[5..8].try_into().unwrap());

    if bus.write(ADDRESS, &command).is_err() {
        ctx.logger.warn("voc-index: command write failed");
        delay.delay_ms(UPDATE_INTERVAL_MS);
        return;
    }

    delay.delay_ms(CONVERSION_MS);

    let mut response = [0u8; 3];
    if bus.read(ADDRESS, &mut response).is_err() {
        ctx.logger.warn("voc-index: no response");
    } else if response[2] != crc8(&response[..2]) {
        ctx.logger.warn("voc-index: CRC mismatch");
    } else {
        let raw = u16::from_be_bytes([response[0], response[1]]);
        ctx.logger.info(format_args!(
            "VOC raw: {raw} ticks at {celsius:.1} C / {percent:.0} %RH (index needs the algorithm)"
        ));
    }

    delay.delay_ms(UPDATE_INTERVAL_MS);
}
