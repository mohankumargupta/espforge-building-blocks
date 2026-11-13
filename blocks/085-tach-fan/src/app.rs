use crate::Context;

/// `update_interval: 3s`
const WINDOW_MS: u32 = 3_000;
/// Two tach pulses per revolution: (pulses / 2) * (60 s / 3 s) = pulses * 10,
/// and with the YAML's multiplier that is 30 per count over 3 s of *pairs*.
const RPMS_PER_COUNT: u32 = 30;
/// The requested speed, as a fraction of full duty.
const REQUESTED_DUTY: f32 = 0.6;
/// Below this many RPM at that duty, the fan is stalled or unplugged.
const STALL_RPM: u32 = 300;

/// The intended shape: `<speed_fan>` owns the PWM and the tach window and
/// exposes `set_speed`/`rpm`. It is a proposal.
pub fn setup(ctx: &mut Context) {
    ctx.logger.info(
        "tach-fan: Chassis Fan, 25 kHz drive on GPIO6, tach GPIO7 (proposed)",
    );
}

pub fn forever(ctx: &mut Context) {
    let fan = device!(ctx, chassis);
    fan.set_duty(REQUESTED_DUTY);

    let pulses = fan.tach_pulses(WINDOW_MS);
    let rpm = pulses * RPMS_PER_COUNT;

    if rpm < STALL_RPM {
        ctx.logger.warn(format_args!(
            "Cooling Fan RPM: {rpm} at {:.0}% — stalled or unplugged",
            REQUESTED_DUTY * 100.0
        ));
    } else {
        ctx.logger
            .info(format_args!("Cooling Fan RPM: {rpm}"));
    }

    ctx.delay.delay_ms(WINDOW_MS);
}
