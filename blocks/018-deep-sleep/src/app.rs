use crate::Context;

/// The intended shape: `<deep_sleep>` is a proposal. One boot is one reading
/// and one sleep; the chip resets on wake, so there is no state to carry here.
pub fn setup(ctx: &mut Context) {
    ctx.logger.info("deep-sleep: awake, taking a reading");
    device!(ctx, status_led).on();

    // `run_duration: 15s` is a ceiling; asking to sleep now ends this run
    // deterministically once the work is done.
    service!(ctx, deep_sleep_control).sleep_now();
}

pub fn forever(ctx: &mut Context) {
    // Unreachable after `sleep_now()`. The generated entry point still needs
    // both functions, so this is the fallback for a refused sleep.
    ctx.delay.delay_ms(1000);
}
