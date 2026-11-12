use crate::Context;

/// `update_interval` for the composed node's status LED.
const BLINK_MS: u32 = 500;

/// Composition is a compiler feature, so this app is just the merged result:
/// the status LED from the node's own devices section.
pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("packages: composed office node (includes are proposed)");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let status = device!(ctx, status);

    status.on();
    delay.delay_ms(BLINK_MS);
    status.off();
    delay.delay_ms(BLINK_MS);
}
