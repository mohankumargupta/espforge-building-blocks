use crate::Context;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// `open_duration: 25s` / `close_duration: 25s`: the timeout for a full travel.
const TRAVEL_MS: u32 = 25_000;
/// Tick period for the travel timer and the endstop polling.
const TICK_MS: u32 = 20;

static OPENING: AtomicBool = AtomicBool::new(false);
static CLOSING: AtomicBool = AtomicBool::new(false);
static TRAVEL_LEFT_MS: AtomicU32 = AtomicU32::new(0);

/// Stop everything: the interlock releases both relays before any move.
fn stop(ctx: &mut Context) {
    device!(ctx, gate_open_relay).off();
    device!(ctx, gate_close_relay).off();
    OPENING.store(false, Ordering::Relaxed);
    CLOSING.store(false, Ordering::Relaxed);
    TRAVEL_LEFT_MS.store(0, Ordering::Relaxed);
}

fn start(ctx: &mut Context, opening: bool) {
    // Release the opposing relay first: both on is a motor short.
    stop(ctx);
    if opening {
        device!(ctx, gate_open_relay).on();
        OPENING.store(true, Ordering::Relaxed);
    } else {
        device!(ctx, gate_close_relay).on();
        CLOSING.store(true, Ordering::Relaxed);
    }
    TRAVEL_LEFT_MS.store(TRAVEL_MS, Ordering::Relaxed);
    ctx.logger.info(format_args!(
        "gate: {} (timeout {TRAVEL_MS} ms)",
        if opening { "opening" } else { "closing" }
    ));
}

pub fn setup(ctx: &mut Context) {
    stop(ctx);
    ctx.logger
        .info("gate-cover: open GPIO4, close GPIO5, limits GPIO2/3");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;

    // The endstops are active low (`inverted: true` in the YAML).
    let open_limit = device!(ctx, gate_open_limit).is_pressed();
    let close_limit = device!(ctx, gate_close_limit).is_pressed();

    // A limit switch in the direction of travel ends the move immediately.
    if OPENING.load(Ordering::Relaxed) && open_limit {
        stop(ctx);
        ctx.logger.info("gate: open limit reached");
    } else if CLOSING.load(Ordering::Relaxed) && close_limit {
        stop(ctx);
        ctx.logger.info("gate: close limit reached");
    }

    // The travel timeout is the safety net for a stuck gate or a dead limit.
    let remaining = TRAVEL_LEFT_MS.load(Ordering::Relaxed);
    if remaining > 0 {
        let left = remaining.saturating_sub(TICK_MS);
        TRAVEL_LEFT_MS.store(left, Ordering::Relaxed);
        if left == 0 {
            stop(ctx);
            ctx.logger.warn("gate: travel timeout, motor stopped");
        }
    }

    // A simple command loop: the open/close inputs are the two limit switches
    // here, so this block moves only once per boot and then waits.
    static mut STARTED: bool = false;
    // SAFETY: only `forever` touches this, and it is not re-entered.
    let started = unsafe { &mut *core::ptr::addr_of_mut!(STARTED) };
    if !*started && !open_limit {
        *started = true;
        start(ctx, true);
    }

    delay.delay_ms(TICK_MS);
}
