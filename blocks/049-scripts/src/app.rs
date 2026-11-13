use crate::Context;
use core::sync::atomic::{AtomicU8, AtomicU32, Ordering};

/// `delay: 500ms`
const ALERT_HOLD_MS: u32 = 500;
/// The state machine is ticked, not slept, so this is its resolution.
const TICK_MS: u32 = 20;

const IDLE: u8 = 0;
const ALERT: u8 = 1;
const WAIT_RELEASE: u8 = 2;

static STAGE: AtomicU8 = AtomicU8::new(IDLE);
/// Milliseconds spent in the current stage.
static ELAPSED_MS: AtomicU32 = AtomicU32::new(0);
/// `mode: restart`: every trigger replaces the run in flight.
static GENERATION: AtomicU32 = AtomicU32::new(0);
static SEEN_GENERATION: AtomicU32 = AtomicU32::new(0);

/// `script.execute: sequence_alert`
fn trigger(ctx: &mut Context) {
    GENERATION.fetch_add(1, Ordering::Relaxed);
    ELAPSED_MS.store(0, Ordering::Relaxed);
    STAGE.store(ALERT, Ordering::Relaxed);
    device!(ctx, alert_relay).on();
    ctx.logger.info("script: sequence_alert started");
}

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("scripts: sequence_alert, mode restart, 500 ms alert hold");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;

    // A press starts a fresh run, cancelling whatever stage was current.
    if device!(ctx, physical_button).is_pressed() {
        trigger(ctx);
    }

    // A cancelled run notices it is stale and abandons its remaining stages.
    let generation = GENERATION.load(Ordering::Relaxed);
    if SEEN_GENERATION.load(Ordering::Relaxed) != generation {
        SEEN_GENERATION.store(generation, Ordering::Relaxed);
    }

    let elapsed = ELAPSED_MS.fetch_add(TICK_MS, Ordering::Relaxed) + TICK_MS;

    match STAGE.load(Ordering::Relaxed) {
        ALERT if elapsed >= ALERT_HOLD_MS => {
            device!(ctx, alert_relay).off();
            STAGE.store(WAIT_RELEASE, Ordering::Relaxed);
            ELAPSED_MS.store(0, Ordering::Relaxed);
        }
        WAIT_RELEASE => {
            // `wait_until: binary_sensor.is_off` — the coroutine parks here.
            if !device!(ctx, physical_button).is_pressed() {
                ctx.logger.info("Alert cleared and button released");
                STAGE.store(IDLE, Ordering::Relaxed);
                ELAPSED_MS.store(0, Ordering::Relaxed);
            }
        }
        _ => {}
    }

    delay.delay_ms(TICK_MS);
}
