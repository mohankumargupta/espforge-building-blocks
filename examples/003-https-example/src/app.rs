//! 003 - HTTPS client: one GET over TLS 1.3 to a public server.
//!
//! `<https_client>` does the DNS lookup, the TCP connection and the TLS
//! handshake through `network="$home"`, so nothing here names an address. The
//! `TEST:` lines are what the gate in the justfile greps for.
//!
//! The handshake is unverified today: certificate checking needs a trust store
//! in the runtime's embedded-tls build, so the board declares
//! `insecure="true"` and this example is a test of the encrypted path, not of
//! the server's identity.

use crate::Context;
use embassy_executor::Spawner;

/// Same page as 002, so the only difference is the transport.
const URL: &str = "https://httpbin.org/robots.txt";

/// Runs once at startup: get on the network and report the address.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    let station = service!(ctx, home);

    logger.info("TEST: https boot");
    logger.info("https-example: waiting for association and DHCP...");
    station.wait_connected().await;

    match station.stack().config_v4() {
        Some(config) => logger.info(format_args!("TEST: ip {}", config.address.address())),
        None => logger.info("TEST: ip none"),
    }
}

/// Per-tick loop: one GET, log the body, sleep.
pub async fn forever(ctx: &'static Context) {
    let logger = ctx.logger;
    let client = service!(ctx, client);
    let mut rx_buf = [0u8; 1024];

    logger.info(format_args!("TEST: https begin {URL}"));
    match client.get(URL, &mut rx_buf).await {
        Ok(body) => match core::str::from_utf8(body) {
            Ok(text) => logger.info(format_args!("TEST: https ok {} bytes: {text}", body.len())),
            Err(_) => logger.info(format_args!("TEST: https ok {} bytes (not utf-8)", body.len())),
        },
        Err(error) => logger.info(format_args!("TEST: https failed: {error:?}")),
    }

    ctx.delay.delay_ms(10000).await;
}
