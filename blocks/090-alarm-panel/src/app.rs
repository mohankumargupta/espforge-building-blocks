use crate::Context;
use core::sync::atomic::{AtomicU8, AtomicU32, Ordering};

/// `codes: ["1234"]` — entered as three taps on the disarm button.
const CODE_TAPS: u8 = 3;
/// Exit delay: leave the room before the panel arms.
const EXIT_MS: u32 = 30_000;
/// Entry delay: time to disarm before the siren fires.
const ENTRY_MS: u32 = 15_000;
const SAMPLE_MS: u32 = 20;

const DISARMED: u8 = 0;
const PENDING: u8 = 1;
const ARMED_HOME: u8 = 2;
const ARMED_AWAY: u8 = 3;
const TRIGGERED: u8 = 4;

static STATE: AtomicU8 = AtomicU8::new(DISARMED);
static COUNTDOWN_MS: AtomicU32 = AtomicU32::new(0);
static TAPS: AtomicU8 = AtomicU8::new(0);
static LAST_PRESSED: AtomicU8 = AtomicU8::new(0);

const fn name(state: u8) -> &'static str {
    match state {
        PENDING => "pending",
        ARMED_HOME => "armed_home",
        ARMED_AWAY => "armed_away",
        TRIGGERED => "triggered",
        _ => "disarmed",
    }
}

fn transition(ctx: &mut Context, next: u8) {
    let siren = device!(ctx, siren_relay);

    // The siren follows the state, in every direction: no path leaves it on.
    if next == TRIGGERED {
        siren.on();
    } else {
        siren.off();
    }

    let countdown = match next {
        PENDING => EXIT_MS,
        _ => 0,
    };
    COUNTDOWN_MS.store(countdown, Ordering::Relaxed);
    STATE.store(next, Ordering::Relaxed);
    ctx.logger.info(format_args!("alarm: {}", name(next)));
}

pub fn setup(ctx: &mut Context) {
    transition(ctx, DISARMED);
    ctx.logger.info(format_args!(
        "alarm-panel: exit {EXIT_MS} ms, entry {ENTRY_MS} ms, code {CODE_TAPS} taps"
    ));
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let pressed = [
        device!(ctx, arm_away).is_pressed(),
        device!(ctx, arm_home).is_pressed(),
        device!(ctx, disarm).is_pressed(),
        device!(ctx, entry_pir).is_pressed(),
    ];
    let previous = LAST_PRESSED.load(Ordering::Relaxed);
    let mut edges = [false; 4];
    for (index, down) in pressed.iter().enumerate() {
        let was = previous & (1 << index) != 0;
        edges[index] = *down && !was;
    }
    let mask = pressed
        .iter()
        .enumerate()
        .fold(0u8, |acc, (index, down)| acc | ((*down as u8) << index));
    LAST_PRESSED.store(mask, Ordering::Relaxed);

    let state = STATE.load(Ordering::Relaxed);

    if edges[2] {
        // Disarm: the taps accumulate, and the panel clears once the code is in.
        let taps = TAPS.fetch_add(1, Ordering::Relaxed) + 1;
        if taps >= CODE_TAPS {
            TAPS.store(0, Ordering::Relaxed);
            transition(ctx, DISARMED);
        }
    } else if edges[1] && state == DISARMED {
        transition(ctx, ARMED_HOME);
    } else if edges[0] && state == DISARMED {
        transition(ctx, PENDING);
    } else if edges[3] && (state == ARMED_HOME || state == ARMED_AWAY) {
        // Entry: motion while armed starts the entry delay, not the siren.
        COUNTDOWN_MS.store(ENTRY_MS, Ordering::Relaxed);
        ctx.logger.info("alarm: entry detected, entry delay running");
    }

    // Count the current delay down and act on expiry.
    let remaining = COUNTDOWN_MS.load(Ordering::Relaxed);
    if remaining > 0 {
        let left = remaining.saturating_sub(SAMPLE_MS);
        COUNTDOWN_MS.store(left, Ordering::Relaxed);
        if left == 0 {
            let state = STATE.load(Ordering::Relaxed);
            match state {
                PENDING => transition(ctx, ARMED_AWAY),
                ARMED_HOME | ARMED_AWAY => transition(ctx, TRIGGERED),
                _ => {}
            }
        }
    }

    delay.delay_ms(SAMPLE_MS);
}
