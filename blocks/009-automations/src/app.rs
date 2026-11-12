use crate::Context;

/// `on_press: then: light.toggle` compiled into the app: one press toggles the
/// lamp once, with a small debounce on both edges.
const DEBOUNCE_MS: u32 = 20;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("automations: wall switch toggles the desk lamp");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let button = device!(ctx, wall_switch);
    let lamp = device!(ctx, desk_lamp);

    if button.is_pressed() {
        delay.delay_ms(DEBOUNCE_MS);
        if button.is_pressed() {
            lamp.toggle();
            ctx.logger.info("automation: toggle desk_lamp");

            // Hold the action until release, so it fires once per press.
            while button.is_pressed() {
                delay.delay_ms(10);
            }
        }
    }

    delay.delay_ms(10);
}
