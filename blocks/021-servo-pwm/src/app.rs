use crate::Context;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// `frequency: 50Hz`: one frame every 20 ms.
const FRAME_NS: u32 = 20_000_000;
/// `min_pulse_width: 500us`
const MIN_PULSE_NS: u32 = 500_000;
/// `max_pulse_width: 2500us`
const MAX_PULSE_NS: u32 = 2_500_000;
const ANGLE_MAX: u8 = 180;
/// `transition_length`: a larger step is a faster slew. 2 degrees per frame
/// sweeps the full range in 1.8 s.
const ANGLE_STEP: u8 = 2;

/// The sweep continues across `forever` calls, so its state cannot live on the
/// stack: the generated main calls this function repeatedly.
static ANGLE: AtomicU8 = AtomicU8::new(0);
static RISING: AtomicBool = AtomicBool::new(true);

/// The calibration curve: 0..180 degrees mapped onto 500..2500 us.
fn pulse_ns(angle: u8) -> u32 {
    MIN_PULSE_NS + (MAX_PULSE_NS - MIN_PULSE_NS) * angle as u32 / ANGLE_MAX as u32
}

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("servo-pwm: camera_pan on GPIO3, 50 Hz software PWM");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let servo = device!(ctx, servo_pwm);

    let angle = ANGLE.load(Ordering::Relaxed);
    let high_ns = pulse_ns(angle);

    // One frame: high for the pulse, low for the rest of it.
    servo.on();
    delay.delay_ns(high_ns);
    servo.off();
    delay.delay_ns(FRAME_NS - high_ns);

    // Slew one step towards the next end of the sweep.
    let rising = RISING.load(Ordering::Relaxed);
    let next = if rising {
        angle.saturating_add(ANGLE_STEP)
    } else {
        angle.saturating_sub(ANGLE_STEP)
    };

    if next >= ANGLE_MAX {
        ANGLE.store(ANGLE_MAX, Ordering::Relaxed);
        RISING.store(false, Ordering::Relaxed);
    } else if !rising && angle <= ANGLE_STEP {
        ANGLE.store(0, Ordering::Relaxed);
        RISING.store(true, Ordering::Relaxed);
    } else {
        ANGLE.store(next, Ordering::Relaxed);
    }
}
