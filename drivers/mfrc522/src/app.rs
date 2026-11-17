//! Happy-path fixture for the project-local `<mfrc522>` driver
//! (planmfrc522.md).
//!
//! The driver itself is the generated `reader` device: `Devices::new` builds
//! `mfrc522_driver::Mfrc522`, which soft-resets the reader, checks
//! `VersionReg` for 0x92 or 0x91 and switches the antenna on. This app only
//! adds machine-checkable output:
//!
//! - `TEST: mfrc522 boot`
//! - `TEST: tag uid=deadbeef sak=0x08 atqa=0x0004` once a second
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
    match device!(ctx, reader).read_tag() {
        Ok(Some(tag)) => ctx.logger.info(format_args!(
            "TEST: tag uid={:02x}{:02x}{:02x}{:02x} sak=0x{:02x} atqa=0x{:04x}",
            tag.uid[0], tag.uid[1], tag.uid[2], tag.uid[3], tag.sak, tag.atqa
        )),
        Ok(None) => ctx.logger.info("TEST: tag none"),
        Err(error) => ctx
            .logger
            .error(format_args!("TEST: tag read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
