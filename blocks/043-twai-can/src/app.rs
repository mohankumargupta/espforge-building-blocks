use crate::Context;

/// `can_id: 0x100` — the identifier this node sends on.
const TX_ID: u32 = 0x100;
/// `can_id: 0x200` — the identifier that switches the relay.
const RELAY_ID: u32 = 0x200;
/// How often this node publishes its own frame.
const TX_INTERVAL_MS: u32 = 1_000;

/// The intended shape: `<can_listener>` owns the acceptance mask and calls
/// back per frame; the peripheral computes the CRC and retries on arbitration
/// loss. The controller itself is a proposal.
pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("twai-can: 500 kbps, TX 0x100, listening for 0x200 (proposed)");
}

pub fn forever(ctx: &mut Context) {
    let bus = device!(ctx, can);
    let relay = device!(ctx, auxiliary_relay);

    // Standard 11-bit frame, one payload byte.
    if bus.send(TX_ID, false, &[0x01]).is_err() {
        ctx.logger.warn("twai-can: transmit failed (bus off?)");
    }

    match bus.try_receive() {
        Some(frame) if frame.id == RELAY_ID => {
            if frame.data.first() == Some(&1) {
                relay.on();
                ctx.logger.info("can: auxiliary relay on");
            } else {
                relay.off();
                ctx.logger.info("can: auxiliary relay off");
            }
        }
        Some(frame) => ctx.logger.info(format_args!(
            "can: unhandled id 0x{:03X}",
            frame.id
        )),
        None => {}
    }

    ctx.delay.delay_ms(TX_INTERVAL_MS);
}
