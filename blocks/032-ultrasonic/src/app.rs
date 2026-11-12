use crate::Context;

/// 10 us trigger strobe.
const TRIGGER_NS: u32 = 10_000;
/// The echo is sampled every 2 us, so the width is quantised to 2 us.
const POLL_NS: u32 = 2_000;
const POLL_US: u32 = 2;
/// `timeout: 3m` plus margin: ~4 m of round trip.
const TIMEOUT_US: u32 = 25_000;
/// `update_interval: 2s`
const UPDATE_INTERVAL_MS: u32 = 2_000;
/// 343 m/s = 0.0343 cm/us, and the pulse covers the trip out and back.
const CM_PER_US: f32 = 0.0343;

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("ultrasonic: HC-SR04 trig GPIO5, echo GPIO6");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let trigger = device!(ctx, trig);
    let echo = device!(ctx, echo);

    // Idle low, then a clean 10 us high pulse.
    trigger.off();
    delay.delay_ns(2_000);
    trigger.on();
    delay.delay_ns(TRIGGER_NS);
    trigger.off();

    // Wait for ECHO to rise.
    let mut waited_us = 0;
    while !echo.is_pressed() && waited_us < TIMEOUT_US {
        delay.delay_ns(POLL_NS);
        waited_us += POLL_US;
    }
    if !echo.is_pressed() {
        ctx.logger.warn("ultrasonic: no echo (timeout)");
        delay.delay_ms(UPDATE_INTERVAL_MS);
        return;
    }

    // Measure how long it stays high.
    let mut high_us = 0;
    while echo.is_pressed() && high_us < TIMEOUT_US {
        delay.delay_ns(POLL_NS);
        high_us += POLL_US;
    }

    if echo.is_pressed() {
        ctx.logger.warn("ultrasonic: echo stuck high");
    } else {
        let distance_cm = high_us as f32 * CM_PER_US / 2.0;
        ctx.logger
            .info(format_args!("Sump Pit Water Level: {distance_cm:.1} cm"));
    }

    delay.delay_ms(UPDATE_INTERVAL_MS);
}
