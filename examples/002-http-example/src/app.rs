//! 002 - HTTP client: one GET to a public server, over the station's stack.
//!
//! The DNS lookup and the TCP connection are both done by the `<http_client>`
//! service through `network="$home"`, so nothing here names an address. The
//! `TEST:` lines are what the gate in the justfile greps for.

use crate::Context;
use embassy_executor::Spawner;

/// Plain HTTP on purpose: this is a test of the network path, not of TLS.
const URL: &str = "http://httpbin.org/robots.txt";

/// Runs once at startup: get on the network and report the address.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    let station = service!(ctx, home);

    logger.info("TEST: http boot");
    logger.info("http-example: waiting for association and DHCP...");
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

    logger.info(format_args!("TEST: http begin {URL}"));
    match client.get(URL, &mut rx_buf).await {
        Ok(body) => match core::str::from_utf8(body) {
            Ok(text) => logger.info(format_args!("TEST: http ok {} bytes: {text}", body.len())),
            Err(_) => logger.info(format_args!("TEST: http ok {} bytes (not utf-8)", body.len())),
        },
        Err(error) => logger.info(format_args!("TEST: http failed: {error:?}")),
    }

    ctx.delay.delay_ms(10000).await;
}
