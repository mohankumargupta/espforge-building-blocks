use crate::Context;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// `filters: delayed_off: 30s`
const HOLD_MS: u32 = 30_000;
/// Sampling period; a PIR pulse is far longer than this.
const SAMPLE_MS: u32 = 50;

static MOTION: AtomicBool = AtomicBool::new(false);
static HOLD_LEFT_MS: AtomicU32 = AtomicU32::new(0);

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("pir-motion: Hallway Motion on GPIO2, delayed_off 30 s");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let pir = device!(ctx, hallway_pir);
    let light = device!(ctx, hall_light);

    if pir.is_pressed() {
        // Retriggerable: a new detection during the hold restarts it.
        HOLD_LEFT_MS.store(HOLD_MS, Ordering::Relaxed);
    }

    let remaining = HOLD_LEFT_MS.load(Ordering::Relaxed);
    let motion = remaining > 0;
    let previous = MOTION.swap(motion, Ordering::Relaxed);

    if motion {
        HOLD_LEFT_MS.store(remaining.saturating_sub(SAMPLE_MS), Ordering::Relaxed);
    }

    if motion && !previous {
        light.on();
        ctx.logger.info("Hallway Motion: detected");
    } else if !motion && previous {
        light.off();
        ctx.logger.info("Hallway Motion: cleared");
    }

    delay.delay_ms(SAMPLE_MS);
}
