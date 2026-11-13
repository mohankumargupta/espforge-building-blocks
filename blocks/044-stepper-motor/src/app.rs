use crate::Context;

/// `max_speed: 1000 steps/s`
const MAX_STEPS_PER_SEC: u32 = 1_000;
/// `acceleration` and `deceleration: 400 steps/s ^ 2`
const STEPS_PER_SEC_SQ: u32 = 400;
/// 200 full steps per revolution on a 1.8 degree motor, 1/16 microstepping.
const MICROSTEPS_PER_REV: i32 = 3_200;
/// The move this block demonstrates: a quarter turn out, then back.
const TARGET_STEPS: i32 = MICROSTEPS_PER_REV / 4;
/// STEP pulse width; the A4988 needs 1 us minimum.
const PULSE_NS: u32 = 2_000;

/// Distance covered while accelerating from rest to `MAX_STEPS_PER_SEC`.
/// v^2 = 2as, so s = v^2 / (2a).
fn ramp_steps() -> f32 {
    let v = MAX_STEPS_PER_SEC as f32;
    (v * v) / (2.0 * STEPS_PER_SEC_SQ as f32)
}

/// One trapezoidal move. Returns the steps taken.
fn move_to(ctx: &mut Context, target: i32) {
    let delay = ctx.delay;
    let step = device!(ctx, step);
    let dir = device!(ctx, dir);

    let distance = target.abs() as f32;
    let ramp = ramp_steps();
    // A short move never reaches the plateau: it accelerates then decelerates.
    let peak = if distance < 2.0 * ramp {
        (distance * STEPS_PER_SEC_SQ as f32).sqrt()
    } else {
        MAX_STEPS_PER_SEC as f32
    };
    let ramp = (peak * peak) / (2.0 * STEPS_PER_SEC_SQ as f32);

    if target < 0 {
        dir.on();
    } else {
        dir.off();
    }

    let mut done = 0.0f32;
    while done < distance {
        // Velocity: ramp up, hold, ramp down over the last `ramp` steps.
        let remaining = distance - done;
        let velocity = if done < ramp {
            (2.0 * STEPS_PER_SEC_SQ as f32 * done).sqrt().min(peak)
        } else if remaining < ramp {
            (2.0 * STEPS_PER_SEC_SQ as f32 * remaining).sqrt().min(peak)
        } else {
            peak
        }
        .max(1.0);

        // One step: high for the driver, low for the rest of the period.
        let period_ns = (1_000_000_000.0 / velocity) as u32;
        step.on();
        delay.delay_ns(PULSE_NS);
        step.off();
        if period_ns > PULSE_NS {
            delay.delay_ns(period_ns - PULSE_NS);
        }

        done += 1.0;
    }
}

pub fn setup(ctx: &mut Context) {
    ctx.logger.info(format_args!(
        "stepper-motor: 1000 steps/s, 400 steps/s^2, {:.0}-step ramp",
        ramp_steps()
    ));
}

pub fn forever(ctx: &mut Context) {
    ctx.logger.info("stepper: quarter turn out");
    move_to(ctx, TARGET_STEPS);
    ctx.delay.delay_ms(1_000);

    ctx.logger.info("stepper: quarter turn back");
    move_to(ctx, -TARGET_STEPS);
    ctx.delay.delay_ms(5_000);
}
