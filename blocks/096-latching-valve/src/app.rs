use crate::Context;
use core::sync::atomic::{AtomicBool, Ordering};

/// `delay: 75ms` — the coil's rated maximum. Longer burns it.
const PULSE_MS: u32 = 75;
/// How long to leave the valve alone between test pulses.
const SETTLE_MS: u32 = 5_000;

/// The valve's physical position, which survives without power.
static LATCHED: AtomicBool = AtomicBool::new(false);

/// Pulse one coil for exactly `PULSE_MS`, with the other one released first.
fn pulse(ctx: &mut Context, forward: bool) {
    let delay = ctx.delay;
    let (a, b) = (device!(ctx, pulse_forward), device!(ctx, pulse_reverse));

    // Both coils on at once is a short across the driver.
    a.off();
    b.off();

    if forward {
        a.on();
    } else {
        b.on();
    }
    delay.delay_ms(PULSE_MS);
    a.off();
    b.off();
}

fn set_latched(ctx: &mut Context, open: bool) {
    if LATCHED.load(Ordering::Relaxed) == open {
        return;
    }
    pulse(ctx, open);
    LATCHED.store(open, Ordering::Relaxed);
    ctx.logger.info(format_args!(
        "Latching Gas Valve: {}",
        if open { "open" } else { "closed" }
    ));
}

pub fn setup(ctx: &mut Context) {
    // The valve could be in either position after a reset: drive it shut once.
    pulse(ctx, false);
    LATCHED.store(false, Ordering::Relaxed);
    ctx.logger
        .info("latching-valve: closed on boot, 75 ms pulses");
}

pub fn forever(ctx: &mut Context) {
    static mut OPEN: bool = false;

    // SAFETY: only `forever` touches this, and it is not re-entered.
    let open = unsafe { &mut *core::ptr::addr_of_mut!(OPEN) };
    *open = !*open;

    set_latched(ctx, *open);
    ctx.delay.delay_ms(SETTLE_MS);
}
