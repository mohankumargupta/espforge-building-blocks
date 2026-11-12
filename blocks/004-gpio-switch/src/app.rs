use crate::Context;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("gpio-switch: onboard LED on");
    let led = device!(ctx, onboard_led);
    led.on();
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    delay.delay_ms(1000);
}
