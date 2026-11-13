use crate::{service, Context};
use embassy_executor::Spawner;

/// `interval: 10s`
const PROBE_MS: u32 = 10_000;
/// Four misses in a row is a down host, not a dropped packet.
const MISSES_FOR_DOWN: u8 = 4;

/// The intended shape: `<ping>` owns the ICMP socket, the sequence number and
/// the RTT arithmetic. It is a proposal.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger.info("ping: connecting...");
    service!(ctx, home).wait_connected().await;
    ctx.logger.info("ping: watching 192.168.1.1 for the default gateway");
}

pub async fn forever(ctx: &'static Context) {
    static mut MISSES: u8 = 0;

    // SAFETY: only `forever` touches this, and it is not re-entered.
    let misses = unsafe { &mut *core::ptr::addr_of_mut!(MISSES) };
    let gateway = service!(ctx, gateway);

    match gateway.ping().await {
        Ok(rtt_ms) => {
            *misses = 0;
            ctx.logger.info(format_args!(
                "Default Gateway Reachable: true, {rtt_ms} ms"
            ));
        }
        Err(_) => {
            *misses = misses.saturating_add(1);
            if *misses >= MISSES_FOR_DOWN {
                ctx.logger.warn(format_args!(
                    "Default Gateway Reachable: false ({misses} misses)"
                ));
            }
        }
    }

    ctx.delay.delay_ms(PROBE_MS).await;
}
