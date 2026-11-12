use crate::Context;
use espforge_runtime::Rgb;

/// The intended shape: `<ws2812>` is a proposal. Pixels are a framebuffer and
/// `show()` clocks it out over RMT. The `pulse` effect is this loop, not a
/// declarative effect list.
pub fn setup(ctx: &mut Context) {
    ctx.logger.info("rmt-led-strip: desk ambient light");
    let strip = device!(ctx, desk_light);
    strip.fill(Rgb::new(0, 0, 0));
    strip.show();
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let strip = device!(ctx, desk_light);

    // `pulse` with `transition_length: 500ms`: breathe the strip red.
    for level in [0, 32, 64, 128, 255, 128, 64, 32] {
        strip.fill(Rgb::new(level, 0, 0));
        strip.show();
        delay.delay_ms(500);
    }
}
