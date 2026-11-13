use crate::Context;

/// Duty is a fraction of full speed; the bridge turns it into PWM.
const RUN_DUTY: f32 = 0.6;
/// Long enough to see the conveyor move, short enough to watch it stop.
const RUN_MS: u32 = 5_000;
const PAUSE_MS: u32 = 2_000;

/// The intended shape: `<hbridge>` owns the two channels, the dead time between
/// direction changes, and `decay_mode: slow` (PWM on one input, the other low).
pub fn setup(ctx: &mut Context) {
    ctx.logger.info(
        "hbridge-motor: Conveyor DC Motor, IN1 GPIO6, IN2 GPIO7, slow decay (proposed)",
    );
}

pub fn forever(ctx: &mut Context) {
    let conveyor = device!(ctx, conveyor);

    // Forward, then brake, then reverse: the bridge keeps the inputs exclusive.
    if conveyor.set_direction(true, RUN_DUTY).is_err() {
        ctx.logger.warn("hbridge-motor: forward rejected");
    } else {
        ctx.logger.info(format_args!("conveyor: forward at {RUN_DUTY:.0}"));
        ctx.delay.delay_ms(RUN_MS);
    }

    conveyor.brake();
    ctx.logger.info("conveyor: braking");
    ctx.delay.delay_ms(PAUSE_MS);

    if conveyor.set_direction(false, RUN_DUTY).is_ok() {
        ctx.logger.info(format_args!("conveyor: reverse at {RUN_DUTY:.0}"));
        ctx.delay.delay_ms(RUN_MS);
    }

    conveyor.brake();
    ctx.delay.delay_ms(PAUSE_MS);
}
