use crate::Context;

/// NEC address/command for the power key, from the ESPHome YAML.
const POWER_KEY: (u16, u16) = (0x00FF, 0x45BA);
/// One NEC frame is ~67 ms; polling faster only wastes cycles.
const POLL_MS: u32 = 20;

/// The intended shape: `<ir_receiver>` measures marks and spaces in hardware
/// (RMT), decodes NEC, and hands the app an address/command pair. No part of
/// that pipeline exists yet.
pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("ir-remote: NEC receiver on GPIO4 (proposed)");
}

pub fn forever(ctx: &mut Context) {
    let receiver = device!(ctx, ir_rx);

    if let Some(frame) = receiver.nec() {
        if frame == POWER_KEY {
            ctx.logger.info("IR Remote Power Key: pressed");
        }
    }

    ctx.delay.delay_ms(POLL_MS);
}
