use crate::Context;
use embassy_executor::Spawner;

/// The controller reports this many connection slots; a proxy keeps them all
/// open for Home Assistant rather than one per entity.
const CONNECTION_SLOTS: usize = 3;

/// The intended shape: `<bluetooth_proxy>` starts a passive scan and a
/// forwarding task, and `active` lets the remote side open connections. All of
/// it is proposed.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger.info(format_args!(
        "ble-proxy: passive scan plus {CONNECTION_SLOTS} proxied links (proposed)"
    ));
}

pub async fn forever(ctx: &'static Context) {
    match service!(ctx, proxy).active_connections() {
        Ok(links) => ctx.logger.info(format_args!(
            "ble-proxy: {} of {CONNECTION_SLOTS} links in use",
            links
        )),
        Err(_) => ctx.logger.warn("ble-proxy: radio not ready"),
    }

    ctx.delay.delay_ms(30_000).await;
}
