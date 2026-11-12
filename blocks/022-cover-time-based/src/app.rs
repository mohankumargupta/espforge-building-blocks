use crate::Context;
use core::sync::atomic::{AtomicU8, Ordering};

/// `open_duration: 15s` / `close_duration: 15s`, ticked at 100 ms.
const TICK_MS: u32 = 100;
const TRANSIT_TICKS: u8 = 150;

const IDLE: u8 = 0;
const OPENING: u8 = 1;
const CLOSING: u8 = 2;

/// Position is estimated in transit ticks from fully closed (0..=150).
static POSITION: AtomicU8 = AtomicU8::new(0);
static STATE: AtomicU8 = AtomicU8::new(IDLE);
/// Pressed buttons on the previous tick, for edge detection.
static LAST_BUTTONS: AtomicU8 = AtomicU8::new(0);

fn percent(position: u8) -> u32 {
    position as u32 * 100 / TRANSIT_TICKS as u32
}

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("cover-time-based: blinds closed, 15 s each way");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let open_relay = device!(ctx, motor_open);
    let close_relay = device!(ctx, motor_close);
    let open_btn = device!(ctx, open_button);
    let close_btn = device!(ctx, close_button);
    let stop_btn = device!(ctx, stop_button);

    let mask = u8::from(open_btn.is_pressed())
        | (u8::from(close_btn.is_pressed()) << 1)
        | (u8::from(stop_btn.is_pressed()) << 2);
    let previous = LAST_BUTTONS.swap(mask, Ordering::Relaxed);
    let pressed = mask & !previous;

    if pressed & 0b100 != 0 {
        STATE.store(IDLE, Ordering::Relaxed);
        open_relay.off();
        close_relay.off();
        ctx.logger.info("cover: stop");
    } else if pressed & 0b001 != 0 {
        // Interlock: release the opposite relay before energising this one.
        close_relay.off();
        open_relay.on();
        STATE.store(OPENING, Ordering::Relaxed);
        ctx.logger.info(format_args!(
            "cover: opening from {}%",
            percent(POSITION.load(Ordering::Relaxed))
        ));
    } else if pressed & 0b010 != 0 {
        open_relay.off();
        close_relay.on();
        STATE.store(CLOSING, Ordering::Relaxed);
        ctx.logger.info(format_args!(
            "cover: closing from {}%",
            percent(POSITION.load(Ordering::Relaxed))
        ));
    }

    // Advance the estimate while a relay is energised.
    let mut position = POSITION.load(Ordering::Relaxed);
    match STATE.load(Ordering::Relaxed) {
        OPENING => {
            position = position.saturating_add(1).min(TRANSIT_TICKS);
            POSITION.store(position, Ordering::Relaxed);
            if position == TRANSIT_TICKS {
                open_relay.off();
                STATE.store(IDLE, Ordering::Relaxed);
                ctx.logger.info("cover: open");
            }
        }
        CLOSING => {
            position = position.saturating_sub(1);
            POSITION.store(position, Ordering::Relaxed);
            if position == 0 {
                close_relay.off();
                STATE.store(IDLE, Ordering::Relaxed);
                ctx.logger.info("cover: closed");
            }
        }
        _ => {}
    }

    delay.delay_ms(TICK_MS);
}
