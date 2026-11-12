use crate::{service, Context};

/// The peer's MAC, matching `esp_now.send: address`.
const PEER: [u8; 6] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
const DEBOUNCE_MS: u32 = 20;

/// The intended shape: `<esp_now>` owns peer registration and `send`; the
/// button and debounce are already real.
pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("esp-now: doorbell -> 11:22:33:44:55:66 (proposed)");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let doorbell = device!(ctx, doorbell);

    if doorbell.is_pressed() {
        delay.delay_ms(DEBOUNCE_MS);
        if doorbell.is_pressed() {
            match service!(ctx, peer).send(&PEER, b"DOORBELL") {
                Ok(()) => ctx.logger.info("esp-now: DOORBELL sent"),
                Err(_) => ctx.logger.warn("esp-now: send failed"),
            }

            // One message per press.
            while doorbell.is_pressed() {
                delay.delay_ms(10);
            }
        }
    }

    delay.delay_ms(10);
}
