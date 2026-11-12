use crate::Context;

/// `sliding_window_moving_average: window_size: 5, send_every: 5`
const SAMPLES: u32 = 5;
const SAMPLE_INTERVAL_MS: u32 = 20;
/// `update_interval: 10s`
const UPDATE_INTERVAL_MS: u32 = 10_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("adc-sensor: battery voltage");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let adc = device!(ctx, battery_adc);

    let mut sum_mv = 0;
    for _ in 0..SAMPLES {
        sum_mv += adc.read_millivolts();
        delay.delay_ms(SAMPLE_INTERVAL_MS);
    }
    let average_mv = sum_mv / SAMPLES;

    // `multiply: 2.0` undoes the divider; the sensor publishes volts.
    let battery_v = average_mv as f32 * 2.0 / 1000.0;
    ctx.logger
        .info(format_args!("Battery Voltage: {:.2} V", battery_v));

    delay.delay_ms(UPDATE_INTERVAL_MS);
}
