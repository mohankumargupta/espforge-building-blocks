use crate::Context;

/// ESPHome's `logger:` block has no ESPF element: the generated `main()`
/// installs esp-println's logger before `setup` runs. So the whole project is
/// one boot line plus an idle loop.
pub fn setup(ctx: &mut Context) {
    ctx.logger.info("minimal: booted");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    delay.delay_ms(1000);
}
