//! WebSockets: one text frame over `ws://`, then one over `wss://`.
//!
//! `<ws_client>` and `<wss_client>` hand back the same `WsSession`, so the two
//! halves below differ only in the scheme and in how much buffer they borrow.
//! The upgrade (HTTP GET + `Sec-WebSocket-Accept` check) and the frame masking
//! are the runtime's; nothing here speaks the protocol by hand.
//!
//! The `TEST:` lines are what the gate in the justfile greps for: one `ws ok`
//! and one `wss ok` per round prove each transport carried an echo.

use crate::Context;
use embassy_executor::Spawner;

/// A plain `ws://` endpoint that echoes whatever text frame it receives.
///
/// Not `echo.websocket.org`: that host answers port 80 with a 301 redirect to
/// TLS, and the upgrade checks the reply's status, so the session never opens.
const WS_URL: &str = "ws://websocket-echo.com/";
/// A `wss://` echo endpoint. The handshake is unverified (see the banner).
///
/// Not `websocket-echo.com` on 443: that server aborts this client's TLS 1.3
/// handshake with a fatal `handshake_failure`, so pick a host that accepts the
/// runtime's single cipher suite.
const WSS_URL: &str = "wss://echo.websocket.org/";

/// Runs once at startup: wait for Wi-Fi and report the lease.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    let station = service!(ctx, home);

    logger.info("TEST: websockets boot");
    logger.info("websockets: waiting for association and DHCP...");
    station.wait_connected().await;

    match station.stack().config_v4() {
        Some(config) => logger.info(format_args!("TEST: ip {}", config.address.address())),
        None => logger.info("TEST: ip none"),
    }
}

/// Per-tick half one: a plain WebSocket session.
///
/// `conn_buf` holds the socket's own 1024-byte receive and transmit buffers for
/// as long as the session lives, so the borrow is what keeps it alive.
async fn try_ws(ctx: &'static Context) {
    let logger = ctx.logger;
    let ws = service!(ctx, ws);
    let mut conn_buf = [0u8; 4096];
    let mut rx_buf = [0u8; 512];

    logger.info(format_args!("TEST: ws begin {WS_URL}"));
    match ws.connect(WS_URL, &mut conn_buf).await {
        Ok(mut session) => {
            if let Err(error) = session.send_text("Hello from ESP32!").await {
                logger.info(format_args!("TEST: ws send failed: {error:?}"));
            }

            match session.recv(&mut rx_buf).await {
                Ok(msg) => match core::str::from_utf8(msg.payload) {
                    Ok(text) => logger.info(format_args!(
                        "TEST: ws ok {} bytes: {text}",
                        msg.payload.len()
                    )),
                    Err(_) => logger.info(format_args!(
                        "TEST: ws ok {} bytes (not utf-8)",
                        msg.payload.len()
                    )),
                },
                Err(error) => logger.info(format_args!("TEST: ws recv failed: {error:?}")),
            }

            // 1000 is "normal closure"; the echo server answers with its own.
            let _ = session.close(1000).await;
        }
        Err(error) => logger.info(format_args!("TEST: ws failed: {error:?}")),
    }
}

/// Per-tick half two: the same exchange over TLS.
///
/// The buffer is `rx | tx` socket buffers plus the two TLS record buffers
/// (4096 read, 2048 write), which is `WssClient::required_buffer()` at the
/// defaults. All of it is borrowed for the session's lifetime.
async fn try_wss(ctx: &'static Context) {
    let logger = ctx.logger;
    let wss = service!(ctx, secure_ws);
    let mut conn_buf = [0u8; 8192];
    let mut rx_buf = [0u8; 512];

    logger.info(format_args!("TEST: wss begin {WSS_URL}"));
    match wss.connect(WSS_URL, &mut conn_buf).await {
        Ok(mut session) => {
            if let Err(error) = session.send_text("Hello from ESP32!").await {
                logger.info(format_args!("TEST: wss send failed: {error:?}"));
            }

            match session.recv(&mut rx_buf).await {
                Ok(msg) => match core::str::from_utf8(msg.payload) {
                    Ok(text) => logger.info(format_args!(
                        "TEST: wss ok {} bytes: {text}",
                        msg.payload.len()
                    )),
                    Err(_) => logger.info(format_args!(
                        "TEST: wss ok {} bytes (not utf-8)",
                        msg.payload.len()
                    )),
                },
                Err(error) => logger.info(format_args!("TEST: wss recv failed: {error:?}")),
            }

            let _ = session.close(1000).await;
        }
        Err(error) => logger.info(format_args!("TEST: wss failed: {error:?}")),
    }
}

/// Per-tick loop: one session of each kind, then sleep.
///
/// The two halves are separate functions so the two buffer sets are never live
/// at once: the TLS half's 8 KiB would otherwise stack on top of the plain
/// half's 4 KiB on the embassy task stack.
pub async fn forever(ctx: &'static Context) {
    let logger = ctx.logger;

    try_ws(ctx).await;
    try_wss(ctx).await;

    logger.info("TEST: websockets round done");
    ctx.delay.delay_ms(15000).await;
}
