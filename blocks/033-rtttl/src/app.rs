use crate::{service, Context};

/// The ESPHome `rtttl.play:` string, verbatim.
const ALERT_TUNE: &str = "beep:d=4,o=5,b=200:8c6,8g,8c6";

/// The intended shape: `<rtttl>` parses the notation and retunes `<pwm>` for
/// each note without blocking `forever`. Both elements are proposals.
pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("rtttl: buzzer on GPIO7 at 50% duty (proposed)");
}

pub fn forever(ctx: &mut Context) {
    service!(ctx, tunes).play(ALERT_TUNE);
    ctx.delay.delay_ms(5_000);
}
