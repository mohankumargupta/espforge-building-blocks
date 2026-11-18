//! The espforge `i2c_scanner` example, unchanged in behaviour.
//!
//! Run against the emulator with no `--cosim-board`, the bus is the
//! emulator's own: its built-in EEPROM answers at 0x50. The pad choice
//! (SDA 5 / SCL 6) matches `drivers/sht31/board.json` so the scan tests
//! the same pins the SHT31 fixture uses.

use crate::Context;

pub fn setup(ctx: &mut Context) {
    let logger = ctx.logger;
    let delay = ctx.delay;
    let i2c = device!(ctx, i2c0);

    logger.info("TEST: i2c scanner begin");

    // A 0-byte write ACKs at every address with a device. 0x50 is the
    // emulator's built-in EEPROM; with no board, nothing else answers.
    for address in 1..127 {
        match i2c.write(address, &[]) {
            Ok(_) => logger.info(format_args!("Found device at address 0x{:02x}", address)),
            Err(_) => {}
        }
        delay.delay_ms(10);
    }

    logger.info("Scan complete");
}

pub fn forever(ctx: &mut Context) {
    ctx.delay.delay_ms(1000);
}
