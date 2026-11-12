use crate::Context;

/// `speed_count: 3` gives OFF plus three speeds.
const MEDIUM: u8 = 2;

/// The intended shape: `<fan>` owns the speed table and drives `<pwm>` duty;
/// the app only asks for a speed. Both elements are proposals.
pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("fan-speed: ceiling fan on GPIO3, 3 speeds (proposed)");
}

pub fn forever(ctx: &mut Context) {
    device!(ctx, ceiling).set_speed(MEDIUM);
    ctx.delay.delay_ms(60_000);
}
