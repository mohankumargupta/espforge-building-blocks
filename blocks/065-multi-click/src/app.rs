use crate::Context;

/// Sampling period: the resolution of every bound below.
const SAMPLE_MS: u32 = 10;
/// `ON for at most 350ms`
const SHORT_MS: u32 = 350;
/// `OFF for at most 350ms` between clicks.
const GAP_MS: u32 = 350;
/// `OFF for at least 200ms` ends the sequence.
const QUIET_MS: u32 = 200;
/// `ON for at least 1000ms`
const LONG_MS: u32 = 1_000;

/// What the button is doing right now, in click-counting terms.
struct Clicker {
    pressed: bool,
    /// Time in the current press or gap.
    elapsed_ms: u32,
    /// Completed short presses not yet reported.
    clicks: u8,
    /// The press for this cycle is already too long to count as a click.
    long_fired: bool,
}

impl Clicker {
    const fn new() -> Self {
        Self {
            pressed: false,
            elapsed_ms: 0,
            clicks: 0,
            long_fired: false,
        }
    }

    /// Advance one sample and report whatever pattern completed.
    fn sample(&mut self, pressed: bool, ctx: &Context) {
        if pressed != self.pressed {
            self.finish_edge(ctx);
            self.pressed = pressed;
            self.elapsed_ms = 0;
            self.long_fired = false;
        } else {
            self.elapsed_ms += SAMPLE_MS;
        }

        if pressed {
            // The long press fires while still held.
            if !self.long_fired && self.elapsed_ms >= LONG_MS {
                self.long_fired = true;
                self.clicks = 0;
                ctx.logger.info("Long-press hold triggered");
            }
        } else if self.clicks > 0 && self.elapsed_ms >= QUIET_MS {
            // The quiet window expired: what we have is the whole sequence.
            match self.clicks {
                1 => ctx.logger.info("Single-click triggered"),
                2 => ctx.logger.info("Double-click triggered"),
                count => ctx.logger.info(format_args!("{count} clicks triggered")),
            }
            self.clicks = 0;
        }
    }

    /// Classify the edge that just ended.
    fn finish_edge(&mut self, ctx: &Context) {
        let _ = ctx;
        if self.pressed {
            // A release: short presses count, held presses were already reported.
            if !self.long_fired && self.elapsed_ms <= SHORT_MS {
                self.clicks = self.clicks.saturating_add(1);
            } else if !self.long_fired {
                // Too long for a click, too short for a hold: drop it.
                self.clicks = 0;
            }
        } else if self.elapsed_ms > GAP_MS {
            // A long gap ends the sequence even without the quiet window.
            self.clicks = 0;
        }
    }
}

pub fn setup(ctx: &mut Context) {
    ctx.logger.info(
        "multi-click: short 350 ms, gap 350 ms, quiet 200 ms, hold 1000 ms",
    );
}

pub fn forever(ctx: &mut Context) {
    static mut CLICKER: Clicker = Clicker::new();

    // SAFETY: only `forever` touches the clicker, and it is not re-entered.
    let clicker = unsafe { &mut *core::ptr::addr_of_mut!(CLICKER) };
    let pressed = device!(ctx, multifunction_button).is_pressed();
    clicker.sample(pressed, ctx);

    ctx.delay.delay_ms(SAMPLE_MS);
}
