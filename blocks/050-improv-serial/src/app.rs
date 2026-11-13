use crate::{service, Context};
use embassy_executor::Spawner;

/// Improv's advertised state, in the protocol's own numbering.
const STATE_AUTHORIZED: u8 = 0x02;
const STATE_PROVISIONED: u8 = 0x04;

/// The intended shape: `<improv>` owns the serial framer, answers the RPCs from
/// the browser, and reports credentials back. It is a proposal, and it would
/// have to share UART0 with the logger.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let pairing = service!(ctx, pairing);

    ctx.logger.info(format_args!(
        "improv-serial: pairing state {:#04X} (proposed)",
        pairing.state()
    ));

    // A provisioned device would then associate with the chosen network.
    if pairing.state() == STATE_PROVISIONED {
        ctx.logger.info("improv: credentials stored, rebooting into station mode");
    } else {
        ctx.logger.info("improv: waiting for a browser to pair");
    }
}

pub async fn forever(ctx: &'static Context) {
    let pairing = service!(ctx, pairing);

    if pairing.state() != STATE_AUTHORIZED && pairing.state() != STATE_PROVISIONED {
        ctx.logger.warn("improv-serial: no host on the console");
    }

    ctx.delay.delay_ms(5_000).await;
}
