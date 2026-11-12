use crate::Context;
use core::sync::atomic::{AtomicI32, AtomicU8, Ordering};

/// The polling period bounds the fastest transition the decoder can see.
const SAMPLE_MS: u32 = 1;
/// `resolution: 4`: four quadrature transitions make one detent.
const RESOLUTION: i32 = 4;
/// `min_value` / `max_value`
const MIN_VALUE: i32 = 0;
const MAX_VALUE: i32 = 100;

/// The decoded knob position, shared across `forever` calls.
static VALUE: AtomicI32 = AtomicI32::new(50);
/// Transitions since the last whole detent.
static QUARTER_STEPS: AtomicI32 = AtomicI32::new(0);
/// A/B levels seen on the previous sample.
static LAST_STATE: AtomicU8 = AtomicU8::new(0);

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("rotary-encoder: volume dial on GPIO6/GPIO7, 0..100");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let pin_a = device!(ctx, encoder_a);
    let pin_b = device!(ctx, encoder_b);

    // With pull-ups, a closed switch reads "pressed" (low).
    let state = (u8::from(pin_a.is_pressed()) << 1) | u8::from(pin_b.is_pressed());
    let last = LAST_STATE.swap(state, Ordering::Relaxed);

    // Adjacent Gray-code transitions only: everything else is bounce.
    let delta = match (last, state) {
        (0b00, 0b01) | (0b01, 0b11) | (0b11, 0b10) | (0b10, 0b00) => 1,
        (0b00, 0b10) | (0b10, 0b11) | (0b11, 0b01) | (0b01, 0b00) => -1,
        _ => 0,
    };

    if delta != 0 {
        let steps = QUARTER_STEPS.load(Ordering::Relaxed) + delta;
        if steps >= RESOLUTION {
            QUARTER_STEPS.store(0, Ordering::Relaxed);
            let value = (VALUE.load(Ordering::Relaxed) + 1).min(MAX_VALUE);
            if value != VALUE.swap(value, Ordering::Relaxed) {
                ctx.logger.info(format_args!("Volume Dial: {value}"));
            }
        } else if steps <= -RESOLUTION {
            QUARTER_STEPS.store(0, Ordering::Relaxed);
            let value = (VALUE.load(Ordering::Relaxed) - 1).max(MIN_VALUE);
            if value != VALUE.swap(value, Ordering::Relaxed) {
                ctx.logger.info(format_args!("Volume Dial: {value}"));
            }
        } else {
            QUARTER_STEPS.store(steps, Ordering::Relaxed);
        }
    }

    delay.delay_ms(SAMPLE_MS);
}
