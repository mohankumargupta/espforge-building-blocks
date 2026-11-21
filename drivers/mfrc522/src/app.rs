//! Happy-path fixture for the `<mfrc522>` driver (planmfrc522.md).
//!
//! The driver itself is the generated `reader` device: `Devices::new` builds
//! `mfrc522::Mfrc522` and the manifest's `then` step runs `init()`, which
//! soft-resets the reader, checks `VersionReg` for 0x92 or 0x91 and switches
//! the antenna on. This app only adds machine-checkable output:
//!
//! - `TEST: mfrc522 boot`
//! - `TEST: tag uid=deadbeef` once a second
//! - `TEST: tag none` while no card is in the field
//!
//! Pins match `drivers/mfrc522/board.json` (CS 7, SCK 6, MOSI 3, MISO 2).

use crate::Context;

/// One poll a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: mfrc522 boot");
}

pub fn forever(ctx: &mut Context) {
    let reader = device!(ctx, reader);
    match reader.new_card_present() {
        Ok(atqa) => match reader.select(&atqa) {
            Ok(uid) => {
                let bytes = uid.as_bytes();
                ctx.logger.info(format_args!(
                    "TEST: tag uid={:02x}{:02x}{:02x}{:02x}",
                    bytes[0], bytes[1], bytes[2], bytes[3]
                ));
            }
            Err(error) => ctx
                .logger
                .error(format_args!("TEST: tag select failed: {error:?}")),
        },
        Err(mfrc522::Error::Timeout) | Err(mfrc522::Error::IncompleteFrame) => {
            ctx.logger.info("TEST: tag none")
        }
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: tag read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
