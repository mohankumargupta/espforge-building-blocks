use crate::Context;

/// The two zones must tile `0..60` with no overlap and no gap.
const ZONE_LEDS: usize = 30;
/// A slow cross-fade is enough to see both zones move independently.
const STEP_MS: u32 = 100;

/// The intended shape: each `<light_zone>` writes into the shared strip buffer
/// and the strip flushes once. Nothing of it exists yet.
pub fn setup(ctx: &mut Context) {
    ctx.logger.info(
        "led-zones: 60-pixel strip split into 0-29 and 30-59 (proposed)",
    );
}

pub fn forever(ctx: &mut Context) {
    static mut LEVEL: u8 = 0;
    static mut UP: bool = true;

    // SAFETY: only `forever` touches these, and it is not re-entered.
    let (level, up) = unsafe { (&mut LEVEL, &mut UP) };

    *level = if *up {
        level.saturating_add(5)
    } else {
        level.saturating_sub(5)
    };
    if *level == 0 || *level == 255 {
        *up = !*up;
    }

    // Opposite phases make it obvious the zones are independent views.
    let left = (0, *level, *level);
    let right = (*level, 0, 255 - *level);

    device!(ctx, desk_left).set_color(left, ZONE_LEDS);
    device!(ctx, desk_right).set_color(right, ZONE_LEDS);
    device!(ctx, physical_strip).flush();

    ctx.logger.info(format_args!(
        "zones: left {left:?}, right {right:?}, level {level}"
    ));
    ctx.delay.delay_ms(STEP_MS);
}
