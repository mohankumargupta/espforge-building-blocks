use crate::{service, Context};
use embassy_executor::Spawner;

/// Where the new image is served. On the emulator the guest reaches the host
/// loopback through its gateway (`testota.md` §4.5).
const UPDATE_URL: &str = "http://192.168.4.1:8000/blink.bin";

/// The image `UPDATE_URL` is allowed to activate: `begin` takes the length and
/// the zlib CRC32 up front, and `finish` verifies both before touching
/// `otadata`. Recompute them for whatever the server actually holds
/// (`testota.md` §4.2); while `IMAGE_SIZE` is 0 the block only joins Wi-Fi.
const IMAGE_SIZE: u32 = 0;
const IMAGE_CRC32: u32 = 0;

/// Stream the image into the inactive slot and reboot into it.
///
/// The download is the transport, `ctx.ota` is the updater: chunks go straight
/// to flash, so a whole image never has to fit in RAM. A truncated or corrupt
/// image is a logged failure — `finish()` verifies it, so nothing is activated.
async fn update(ctx: &'static Context) {
    let logger = ctx.logger;

    if IMAGE_SIZE == 0 {
        logger.info("wifi-ota: no update image configured");
        return;
    }

    logger.info(format_args!(
        "wifi-ota: ota begin size={IMAGE_SIZE} crc={IMAGE_CRC32:#010x}"
    ));
    if let Err(error) = ctx.ota.begin(IMAGE_SIZE, IMAGE_CRC32) {
        logger.error(format_args!("wifi-ota: ota begin failed: {error}"));
        return;
    }

    // `get_stream` hands the body over as it arrives; the decile throttle keeps
    // one progress line per 10% instead of one per 2 KiB chunk.
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
                    logger.info(format_args!("wifi-ota: {percent}%"));
                }
            }
            Ok::<(), ()>(())
        })
        .await;

    if let Err(error) = streamed {
        logger.error(format_args!("wifi-ota: download failed: {error:?}"));
        return;
    }
    if !complete {
        logger.error("wifi-ota: image incomplete");
        return;
    }

    match ctx.ota.finish() {
        Ok(()) => {
            logger.info("wifi-ota: update verified, rebooting");
            ctx.ota.reboot();
        }
        Err(error) => logger.error(format_args!("wifi-ota: ota finish failed: {error}")),
    }
}

/// Runs once at startup: wait for association and DHCP, report the address,
/// confirm the running slot, then try one update.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    let station = service!(ctx, home);

    logger.info("wifi-ota: connecting...");
    station.wait_connected().await;

    match station.stack().config_v4() {
        Some(config) => logger.info(format_args!("Connected, IP address: {}", config.address)),
        None => logger.info("Connected, but no IPv4 address"),
    }

    // The link is the health check: with `rollback="true"` a freshly updated
    // image that never confirms is reverted on the next reset.
    if let Err(error) = ctx.ota.mark_app_valid() {
        logger.warn(format_args!("wifi-ota: could not confirm the image: {error}"));
    }

    update(ctx).await;
}

pub async fn forever(ctx: &'static Context) {
    ctx.delay.delay_ms(1000).await;
}
