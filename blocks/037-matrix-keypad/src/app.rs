use crate::Context;
use core::sync::atomic::{AtomicU8, Ordering};

/// Let the row line settle before sampling the columns.
const SETTLE_NS: u32 = 5_000;
/// Slow enough to debounce, fast enough to feel immediate.
const SCAN_MS: u32 = 10;
const NO_KEY: u8 = 0;

/// The key reported on the previous scan, for press-edge detection.
static LAST_KEY: AtomicU8 = AtomicU8::new(NO_KEY);

/// Drive one row low (strobe) or release it high.
fn drive_row(ctx: &mut Context, row: usize, low: bool) {
    match (row, low) {
        (0, true) => device!(ctx, row0).off(),
        (0, false) => device!(ctx, row0).on(),
        (_, true) => device!(ctx, row1).off(),
        (_, false) => device!(ctx, row1).on(),
    }
}

fn column_pressed(ctx: &mut Context, column: usize) -> bool {
    match column {
        0 => device!(ctx, col0).is_pressed(),
        _ => device!(ctx, col1).is_pressed(),
    }
}

/// Row-major key number: 1..4 for `keys: "1234"`, 0 when nothing is pressed.
fn scan(ctx: &mut Context) -> u8 {
    let delay = ctx.delay;

    for row in 0..2 {
        drive_row(ctx, row, true);
        delay.delay_ns(SETTLE_NS);

        for column in 0..2 {
            if column_pressed(ctx, column) {
                drive_row(ctx, row, false);
                return (row * 2 + column + 1) as u8;
            }
        }

        drive_row(ctx, row, false);
    }

    NO_KEY
}

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("matrix-keypad: 2x2 pad on GPIO0-3, keys 1-4");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let key = scan(ctx);
    let previous = LAST_KEY.swap(key, Ordering::Relaxed);

    // Single-key lockout: report only the transition out of "no key".
    if key != NO_KEY && previous == NO_KEY {
        ctx.logger.info(format_args!("Keypad Digit: {key}"));
    }

    delay.delay_ms(SCAN_MS);
}
