use crate::Context;

/// `delayed_on: 50ms`: the input must read pressed for this long before the
/// press counts. The sampling period is the resolution of the filter.
const DELAYED_ON_MS: u32 = 50;
const SAMPLE_MS: u32 = 5;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("binary-sensor: button with delayed_on(50ms)");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let button = device!(ctx, physical_button);

    // Sample until the press has lasted DELAYED_ON_MS, or the line goes idle.
    let mut held = 0;
    while button.is_pressed() && held < DELAYED_ON_MS {
        delay.delay_ms(SAMPLE_MS);
        held += SAMPLE_MS;
    }

    if held >= DELAYED_ON_MS {
        ctx.logger.info("Physical button: pressed");
        // One event per press: wait for the release before sampling again.
        while button.is_pressed() {
            delay.delay_ms(SAMPLE_MS);
        }
    }

    delay.delay_ms(SAMPLE_MS);
}
