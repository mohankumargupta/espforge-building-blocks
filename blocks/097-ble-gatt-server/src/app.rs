use crate::Context;

/// The characteristic's initial value, as configured.
const INITIAL_VALUE: &[u8] = b"espforge-ready";
/// A server with no client just idles; this is the status report period.
const REPORT_MS: u32 = 30_000;

/// The intended shape: `<gatt_service>` builds the attribute table at compile
/// time and `<characteristic>` exposes read/write/notify on it. It is a
/// proposal.
pub fn setup(ctx: &mut Context) {
    ctx.logger.info(format_args!(
        "ble-gatt-server: custom_service, value {:?}, read+write (proposed)",
        core::str::from_utf8(INITIAL_VALUE).unwrap_or("?")
    ));
}

pub fn forever(ctx: &mut Context) {
    let service = service!(ctx, custom_service);

    if service.is_subscribed() {
        // A notification carries the current value to every subscriber.
        service.notify(INITIAL_VALUE);
        ctx.logger.info(format_args!(
            "gatt: notified {} subscriber(s)",
            service.subscriber_count()
        ));
    } else {
        ctx.logger.info("gatt: advertising, no client connected");
    }

    ctx.delay.delay_ms(REPORT_MS);
}
