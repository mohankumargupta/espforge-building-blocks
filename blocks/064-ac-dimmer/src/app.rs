use crate::Context;

/// 50 Hz mains: a half cycle is 10 ms, and the delay lives inside it.
const HALF_CYCLE_US: u32 = 10_000;
/// The triac gate needs a few tens of microseconds, once per half cycle.
const GATE_STROBE_US: u32 = 50;
/// Brightness ramp step and period.
const STEP_MS: u32 = 200;
const STEP_PERCENT: u8 = 5;

/// Conduction angle -> delay after the zero crossing. Brightness is not linear
/// in the delay, so a real driver would square it here.
fn delay_for(percent: u8) -> u32 {
    let fraction = f32::from(100 - percent.min(100)) / 100.0;
    (fraction * HALF_CYCLE_US as f32) as u32
}

/// The intended shape: the driver owns the zero-cross interrupt and a one-shot
/// timer per half cycle, and `set_level` only changes the delay. It is a
/// proposal.
pub fn setup(ctx: &mut Context) {
    ctx.logger.info(format_args!(
        "ac-dimmer: 50 Hz, gate GPIO3, zero-cross GPIO2, {} us max delay (proposed)",
        HALF_CYCLE_US
    ));
}

pub fn forever(ctx: &mut Context) {
    static mut LEVEL: u8 = 0;
    static mut UP: bool = true;

    // SAFETY: only `forever` touches these, and it is not re-entered.
    let (level, up) = unsafe { (&mut LEVEL, &mut UP) };

    if *up {
        *level = level.saturating_add(STEP_PERCENT).min(100);
        if *level == 100 {
            *up = false;
        }
    } else {
        *level = level.saturating_sub(STEP_PERCENT);
        if *level == 0 {
            *up = true;
        }
    }

    let delay_us = delay_for(*level);
    device!(ctx, chandelier).set_level(*level);

    ctx.logger.info(format_args!(
        "chandelier: {level}% (gate {GATE_STROBE_US} us, {delay_us} us after zero cross, proposed)"
    ));
    ctx.delay.delay_ms(STEP_MS);
}
