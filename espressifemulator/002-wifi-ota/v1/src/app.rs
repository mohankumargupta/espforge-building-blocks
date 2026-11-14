//! Test-only v1 firmware for `testota.md` §4.3.
//!
//! Same `<ota />` + `<wifi_station>` as `blocks/002-wifi-ota`, with `TEST:`
//! log lines and deliberately **no** `mark_app_valid()`: T3 needs the freshly
//! updated image to fail confirmation and roll back into `ota_0`.

use crate::{service, Context};
use embassy_executor::Spawner;

/// Served by `just ota-serve` on the host; the emulator's gateway
/// (`192.168.4.1`) redirects to host loopback.
const UPDATE_URL: &str = "http://192.168.4.1:8000/blink.bin";

/// The image `UPDATE_URL` is allowed to activate. `begin` takes the length and
/// zlib CRC32 up front and `finish` verifies both before touching `otadata`.
/// `just ota-patch-v1 [image]` rewrites these two lines; `just ota-image-info`
/// prints the values for any image (testota.md §4.2, T6).
const IMAGE_SIZE: u32 = 0;
const IMAGE_CRC32: u32 = 0;

pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let station = service!(ctx, home);
    let logger = ctx.logger;

    logger.info("TEST: wifi-ota: connecting...");
    station.wait_connected().await;

    match station.stack().config_v4() {
        Some(config) => logger.info(format_args!("TEST: ip {}", config.address)),
        None => {
            logger.error("TEST: no IPv4 address");
            return;
        }
    }

    if IMAGE_SIZE == 0 {
        logger.warn("TEST: no payload configured; run `just ota-patch-v1`");
        return;
    }

    logger.info(format_args!(
        "TEST: ota begin size={IMAGE_SIZE} crc={IMAGE_CRC32:#010x}"
    ));
    if let Err(error) = ctx.ota.begin(IMAGE_SIZE, IMAGE_CRC32) {
        logger.error(format_args!("TEST: ota begin failed: {error}"));
        return;
    }

    // Chunks go straight to the inactive slot, so the image never has to fit
    // in RAM. The decile throttle keeps one progress line per 10%.
    let http = service!(ctx, http);
    let mut complete = false;
    let mut last_decile = -1i16;
    let streamed = http
        .get_stream(UPDATE_URL, |chunk| {
            complete = ctx.ota.write(chunk).map_err(|_| ())?;
            if let Some(percent) = ctx.ota.progress() {
                let decile = i16::from(percent / 10);
                if decile != last_decile {
                    last_decile = decile;
                    logger.info(format_args!("TEST: ota {percent}%"));
                }
            }
            Ok::<(), ()>(())
        })
        .await;

    if let Err(error) = streamed {
        logger.error(format_args!("TEST: download failed: {error:?}"));
        return;
    }
    if !complete {
        logger.error("TEST: image incomplete");
        return;
    }

    match ctx.ota.finish() {
        Ok(()) => {
            logger.info("TEST: ota done, rebooting");
            ctx.ota.reboot();
        }
        Err(error) => logger.error(format_args!("TEST: ota finish failed: {error}")),
    }
}

pub async fn forever(ctx: &'static Context) {
    ctx.delay.delay_ms(1000).await;
}
