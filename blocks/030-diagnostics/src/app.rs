use crate::{service, Context};
use embassy_executor::Spawner;

/// `update_interval: 60s` for both health readings.
const REPORT_INTERVAL_MS: u32 = 60_000;

/// The intended shape: `<diagnostics>` reads uptime, RSSI and heap from the
/// runtime, and `<restart_button>` asks for a reset. Both are proposals.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger.info("diagnostics: waiting for Wi-Fi");
    service!(ctx, home).wait_connected().await;
    ctx.logger.info("diagnostics: link up, reporting health");
}

pub async fn forever(ctx: &'static Context) {
    let health = service!(ctx, health);

    ctx.logger.info(format_args!(
        "System Uptime: {} s",
        health.uptime_secs()
    ));
    ctx.logger.info(format_args!(
        "Wi-Fi RSSI: {} dBm",
        health.rssi_dbm()
    ));
    ctx.logger.info(format_args!(
        "Free heap: {} bytes",
        health.free_heap_bytes()
    ));

    if device!(ctx, restart_node).pressed() {
        ctx.logger.warn("diagnostics: restart requested");
        health.restart();
    }

    ctx.delay.delay_ms(REPORT_INTERVAL_MS).await;
}
