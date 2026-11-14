//! T4 payload: the `blink` example plus `<ota />`.
//!
//! The block boots once, confirms its slot, then blinks. T3's payload omits
//! the confirmation and is rolled back; this one must survive resets.

use crate::Context;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("blink example");

    // A freshly updated image that never confirms is reverted by the
    // bootloader on the next reset (rollback="true").
    if let Err(error) = ctx.ota.mark_app_valid() {
        ctx.logger.warn(format_args!(
            "blink-confirm: could not confirm the image: {error}"
        ));
    }
}

pub fn forever(ctx: &mut Context) {
    let led = device!(ctx, red_led);
    led.toggle();
    ctx.delay.delay_ms(1000);
}
