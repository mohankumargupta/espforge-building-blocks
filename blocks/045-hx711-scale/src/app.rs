use crate::Context;

/// `gain: 128`: 24 data bits plus one gain pulse, so the next conversion is
/// also at 128.
const GAIN_PULSES: u32 = 25;
/// SCK half-period. The part tolerates 50 ns high and 100 ns low; a microsecond
/// is comfortable and keeps the bit-bang readable.
const CLOCK_HALF_NS: u32 = 1_000;
/// How long to wait for DOUT before calling the conversion late.
const READY_TIMEOUT_US: u32 = 100_000;
const POLL_NS: u32 = 1_000;
/// `calibrate_linear`: 452300 raw counts is 5.0 kg, and 0 counts is 0 kg.
const CALIBRATION_RAW: f32 = 452_300.0;
const CALIBRATION_KG: f32 = 5.0;
/// `sliding_window_moving_average: window_size: 10`
const WINDOW: usize = 10;
/// `update_interval: 1s`
const UPDATE_INTERVAL_MS: u32 = 1_000;

/// Read one signed 24-bit conversion (raw counts, gain pulses included).
fn read_raw(ctx: &mut Context) -> Option<i32> {
    let delay = ctx.delay;
    let sck = device!(ctx, sck);
    let dout = device!(ctx, dout);

    // DOUT low means a conversion is waiting.
    let mut waited_us = 0;
    while !dout.is_pressed() && waited_us < READY_TIMEOUT_US {
        delay.delay_ns(POLL_NS);
        waited_us += POLL_NS / 1_000;
    }
    if !dout.is_pressed() {
        return None;
    }

    let mut raw: i32 = 0;
    for _ in 0..24 {
        sck.on();
        delay.delay_ns(CLOCK_HALF_NS);
        // Sample while SCK is high: the MSB arrives first.
        raw = (raw << 1) | i32::from(dout.is_pressed());
        sck.off();
        delay.delay_ns(CLOCK_HALF_NS);
    }

    // Two's complement in 24 bits: set the top byte to all ones on a negative.
    if raw & 0x0080_0000 != 0 {
        raw |= !0x00FF_FFFF;
    }

    for _ in 24..GAIN_PULSES {
        sck.on();
        delay.delay_ns(CLOCK_HALF_NS);
        sck.off();
        delay.delay_ns(CLOCK_HALF_NS);
    }

    Some(raw)
}

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("hx711-scale: Hopper Scale, gain 128, 10-reading average");
}

pub fn forever(ctx: &mut Context) {
    let mut window = [0f32; WINDOW];
    let mut filled = 0;

    // One full update: one conversion, averaged over the last ten.
    if let Some(raw) = read_raw(ctx) {
        let kilograms = raw as f32 * (CALIBRATION_KG / CALIBRATION_RAW);
        window[filled % WINDOW] = kilograms;
        filled += 1;

        let used = filled.min(WINDOW);
        let sum: f32 = window[..used].iter().sum();
        let average = sum / used as f32;

        ctx.logger.info(format_args!(
            "Hopper Scale: {average:.3} kg (raw {raw})"
        ));
    } else {
        ctx.logger.warn("hx711-scale: DOUT never went low");
    }

    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
