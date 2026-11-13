use crate::Context;

/// `update_interval: 10s`
const WINDOW_MS: u32 = 10_000;
/// Sampling period: a pulse shorter than this can be missed.
const SAMPLE_US: u32 = 1_000;
/// J305/M4011 tube factor: counts per minute to microsieverts per hour.
const USV_PER_CPM: f32 = 0.00812;

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("geiger-counter: J305 tube on GPIO4, 10 s window");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let tube = device!(ctx, geiger_pulse);

    let mut counts: u32 = 0;
    let mut elapsed_us: u32 = 0;
    let mut was_high = false;

    // Count rising edges over one window rather than polling for a level.
    while elapsed_us < WINDOW_MS * 1_000 {
        let high = tube.is_pressed();
        if high && !was_high {
            counts += 1;
        }
        was_high = high;
        delay.delay_ns(SAMPLE_US * 1_000);
        elapsed_us += SAMPLE_US;
    }

    // `multiply: 6` — 60 s of counts from a 10 s sample.
    let cpm = counts * (60 / (WINDOW_MS / 1_000));
    let dose = cpm as f32 * USV_PER_CPM;

    ctx.logger
        .info(format_args!("Radiation Pulse Count: {cpm} CPM"));
    ctx.logger
        .info(format_args!("Radiation Dose Equivalent: {dose:.3} uSv/h"));
}
