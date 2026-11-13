use crate::{service, Context};
use embassy_executor::Spawner;

/// The mDNS port and the link-local multicast group, neither configurable.
const PORT: u16 = 5353;
/// A responder announces on start and then answers queries; this is only a
/// periodic re-announcement.
const ANNOUNCE_MS: u32 = 60_000;

/// The intended shape: `<mdns>` answers A/AAAA/PTR/SRV/TXT queries for
/// `sensor-node.local` and lists the `<mdns_service>` records. It is a
/// proposal.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger.info("mdns: connecting...");
    service!(ctx, home).wait_connected().await;

    match service!(ctx, names).start() {
        Ok(()) => ctx.logger.info(format_args!(
            "mdns: sensor-node.local on port {PORT}, publishing _custom_sensor._tcp"
        )),
        Err(_) => ctx.logger.warn("mdns: could not join the multicast group"),
    }
}

pub async fn forever(ctx: &'static Context) {
    let names = service!(ctx, names);

    ctx.logger.info(format_args!(
        "mdns: {} service record(s), {} queries answered",
        names.service_count(),
        names.queries_answered()
    ));

    ctx.delay.delay_ms(ANNOUNCE_MS).await;
}
