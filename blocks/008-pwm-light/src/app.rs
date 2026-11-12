use crate::Context;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("pwm-light: desk lamp");
    let lamp = device!(ctx, desk_lamp);
    lamp.turn_on();
    lamp.set_brightness(50);
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let lamp = device!(ctx, desk_lamp);

    // Each step is a `transition="1s"` fade, not a jump.
    for brightness in [25, 50, 75, 100] {
        lamp.set_brightness(brightness);
        delay.delay_ms(1000);
    }
    for brightness in [75, 50, 25, 0] {
        lamp.set_brightness(brightness);
        delay.delay_ms(1000);
    }
}
