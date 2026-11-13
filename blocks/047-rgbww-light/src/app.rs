use crate::Context;

/// `cold_white_color_temperature: 6500 K` and the warm end.
const COLD_K: f32 = 6_500.0;
const WARM_K: f32 = 2_700.0;
/// LEDC would run at 10 bits; the blend works in floating point and scales.
const DUTY_MAX: f32 = 1_023.0;
/// Perceived brightness is linear after this power correction.
const GAMMA: f32 = 2.2;

/// One channel level, gamma-corrected and scaled to the timer's range.
fn level(linear: f32) -> u16 {
    (linear.clamp(0.0, 1.0).powf(GAMMA) * DUTY_MAX).round() as u16
}

/// Split a requested colour temperature into cold/warm fractions.
/// Working in mireds (1e6 / kelvin) makes the blend perceptually even.
fn white_balance(kelvin: f32) -> (f32, f32) {
    let cold_mired = 1e6 / COLD_K;
    let warm_mired = 1e6 / WARM_K;
    let target = 1e6 / kelvin.clamp(WARM_K, COLD_K);
    let fraction = ((target - cold_mired) / (warm_mired - cold_mired)).clamp(0.0, 1.0);
    (1.0 - fraction, fraction)
}

/// The intended shape: `<light>` owns the five channels and a transition;
/// the app hands it a colour space and a brightness. It is a proposal.
pub fn setup(ctx: &mut Context) {
    ctx.logger.info(format_args!(
        "rgbww-light: 6500-2700 K blend over five channels ({:.0}-{:.0} mired)",
        1e6 / COLD_K,
        1e6 / WARM_K
    ));
}

pub fn forever(ctx: &mut Context) {
    // Half brightness, neutral white: the case that exercises all five channels.
    let (cold, warm) = white_balance(4_000.0);
    let brightness = 0.5;

    let channels = [
        level(0.0),
        level(0.0),
        level(0.0),
        level(cold * brightness),
        level(warm * brightness),
    ];

    ctx.logger.info(format_args!(
        "ceiling: RGBWW {} {} {} {} {} (proposed)",
        channels[0], channels[1], channels[2], channels[3], channels[4]
    ));
    device!(ctx, ceiling).set_rgbww(0.0, 0.0, 0.0, cold, warm, brightness);

    ctx.delay.delay_ms(5_000);
}
