use crate::{service, Context};
use embassy_executor::Spawner;

/// `address: 10.0.0.15` on the overlay, while Wi-Fi has its own address.
const OVERLAY_ADDRESS: &str = "10.0.0.15";
/// A WireGuard handshake is retried every few seconds until it completes.
const REPORT_MS: u32 = 10_000;

/// The intended shape: `<wireguard>` owns the handshake, the rekey timer and
/// the UDP socket, and provides "network" so clients ride the tunnel. It is a
/// proposal; there is no UDP socket and no crypto in the build.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger.info("wireguard: connecting Wi-Fi first...");
    service!(ctx, home).wait_connected().await;

    let tunnel = service!(ctx, tunnel);
    match tunnel.wait_handshake().await {
        Ok(()) => ctx.logger.info(format_args!(
            "wireguard: tunnel up, overlay address {OVERLAY_ADDRESS}"
        )),
        Err(_) => ctx.logger.warn("wireguard: handshake failed"),
    }
}

pub async fn forever(ctx: &'static Context) {
    let tunnel = service!(ctx, tunnel);

    if tunnel.is_connected() {
        ctx.logger.info(format_args!(
            "wireguard: {} peer, last handshake {} s ago",
            tunnel.peer_endpoint(),
            tunnel.handshake_age_secs()
        ));
    } else {
        ctx.logger.warn("wireguard: overlay down");
    }

    ctx.delay.delay_ms(REPORT_MS).await;
}
